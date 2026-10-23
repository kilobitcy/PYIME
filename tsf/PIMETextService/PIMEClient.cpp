//
//	Copyright (C) 2015 - 2020 Hong Jen Yee (PCMan) <pcman.tw@gmail.com>
//
//	This library is free software; you can redistribute it and/or
//	modify it under the terms of the GNU Library General Public
//	License as published by the Free Software Foundation; either
//	version 2 of the License, or (at your option) any later version.
//
//	This library is distributed in the hope that it will be useful,
//	but WITHOUT ANY WARRANTY; without even the implied warranty of
//	MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
//	Library General Public License for more details.
//
//	You should have received a copy of the GNU Library General Public
//	License along with this library; if not, write to the
//	Free Software Foundation, Inc., 51 Franklin St, Fifth Floor,
//	Boston, MA  02110-1301, USA.
//
// PYIME 修改:tolower 加 :: 前缀、getPipeName 失败返回 wstring(),使 mingw 可编译。
// PYIME 修改:改连 PYIME 自有服务端管道;重叠 I/O 超时、退避、断线清理。

#include "PIMEClient.h"
#include "libIME2/src/Utils.h"
#include <algorithm>
#include <json/json.h>

#include "PIMETextService.h"
#include <cstdlib>
#include <ctime>
#include <memory>
#include <fstream>
#include <cctype>
#include <algorithm>
#include <Winnls.h>  // for IS_HIGH_SURROGATE() macro for checking UTF16 surrogate pairs
#include <VersionHelpers.h>  // Provided by Windows SDK >= 8.1
#include "Backoff.h"
#include "TsfLog.h"

using namespace std;

namespace PIME {

// PYIME:连接与请求超时(毫秒)。超时即断线、放行当前键,不冻宿主 UI 线程。
static const DWORD kConnectTimeoutMs = 200;
static const DWORD kRpcTimeoutMs = 2000;
// PYIME:单条回复上限,与服务端单条消息上限 1 MiB 一致
static const size_t kMaxReplyBytes = 1u << 20;

// PYIME:退避状态按进程共享(同一进程可有多个 Client,分属不同 UI 线程)。
// mingw 线程模型为 win32,没有 std::mutex,用 SRWLOCK。
static Backoff g_backoff;
static SRWLOCK g_backoffLock = SRWLOCK_INIT;

static bool backoffBlocked() {
    ::AcquireSRWLockShared(&g_backoffLock);
    bool blocked = g_backoff.blocked(::GetTickCount64());
    ::ReleaseSRWLockShared(&g_backoffLock);
    return blocked;
}

static DWORD backoffFail() {
    ::AcquireSRWLockExclusive(&g_backoffLock);
    DWORD delay = g_backoff.fail(::GetTickCount64());
    ::ReleaseSRWLockExclusive(&g_backoffLock);
    return delay;
}

static void backoffSucceed() {
    ::AcquireSRWLockExclusive(&g_backoffLock);
    g_backoff.succeed();
    ::ReleaseSRWLockExclusive(&g_backoffLock);
}

static std::string uuidToString(const UUID& uuid) {
    std::string result;
    LPOLESTR buf = nullptr;
    if (SUCCEEDED(::StringFromCLSID(uuid, &buf))) {
        result = utf16ToUtf8(buf);
        ::CoTaskMemFree(buf);
        // convert GUID to lwoer case
        // PYIME: 显式用全局 ::tolower,避免 mingw 下重载歧义
        transform(result.begin(), result.end(), result.begin(), ::tolower);
    }
    return result;
}

bool uuidFromString(const char* uuidStr, UUID& result) {
    std::wstring utf16UuidStr = utf8ToUtf16(uuidStr);
    return SUCCEEDED(CLSIDFromString(utf16UuidStr.c_str(), &result));
}


Client::Client(TextService* service, REFIID langProfileGuid):
	textService_(service),
	pipe_(INVALID_HANDLE_VALUE),
	ioEvent_(::CreateEventW(nullptr, TRUE, FALSE, nullptr)),
	overlapped_(),
	lastIoError_(0),
	abandoning_(false),
	nextSeqNum_(0),
	isActivated_(false),
    guid_{uuidToString(langProfileGuid)},
    shouldWaitConnection_{true} {
}

Client::~Client(void) {
	closeRpcConnection();
    resetTextServiceState();
	if (ioEvent_)
		::CloseHandle(ioEvent_);
	LangBarButton::clearIconCache();
}

// pack a keyEvent object into a json value
//static
void Client::addKeyEventToRpcRequest(Json::Value& request, Ime::KeyEvent& keyEvent) {
	request["charCode"] = keyEvent.charCode();
	request["keyCode"] = keyEvent.keyCode();
	request["repeatCount"] = keyEvent.repeatCount();
	request["scanCode"] = keyEvent.scanCode();
	request["isExtended"] = keyEvent.isExtended();
	Json::Value keyStates(Json::arrayValue);
	const BYTE* states = keyEvent.keyStates();
	for(int i = 0; i < 256; ++i) {
		keyStates.append(states[i]);
	}
	request["keyStates"] = keyStates;
}

bool Client::handleRpcResponse(Json::Value& msg, Ime::EditSession* session) {
	bool success = msg.get("success", false).asBool();
	if (success) {
		updateStatus(msg, session);
	}
	return success;
}

void Client::updateUI(const Json::Value& data) {
	for (auto it = data.begin(); it != data.end(); ++it) {
		const char* name = it.memberName();
		const Json::Value& value = *it;
		if (value.isString() && strcmp(name, "candFontName") == 0) {
			wstring fontName = utf8ToUtf16(value.asCString());
			textService_->setCandFontName(fontName);
		}
		else if (value.isInt() && strcmp(name, "candFontSize") == 0) {
			textService_->setCandFontSize(value.asInt());
		}
		else if (value.isInt() && strcmp(name, "candPerRow") == 0) {
			textService_->setCandPerRow(value.asInt());
		}
		else if (value.isBool() && strcmp(name, "candUseCursor") == 0) {
			textService_->setCandUseCursor(value.asBool());
		}
	}
}

void Client::updateSelectionKeys(Json::Value& msg) {
    // set sel keys before update candidates
    const auto& setSelKeysVal = msg["setSelKeys"];
    if (setSelKeysVal.isString()) {
        // keys used to select candidates
        std::wstring selKeys = utf8ToUtf16(setSelKeysVal.asCString());
        textService_->setSelKeys(selKeys);
    }
}

void Client::updateMessageWindow(Json::Value& msg, Ime::EditSession* session, bool& endComposition) {
    const auto& showMessageVal = msg["showMessage"];
    if (showMessageVal.isObject()) {
        const Json::Value& message = showMessageVal["message"];
        const Json::Value& duration = showMessageVal["duration"];
        if (message.isString() && duration.isInt()) {
            if (!textService_->isComposing()) {
                textService_->startComposition(session->context());
                endComposition = true;
            }
            textService_->showMessage(session, utf8ToUtf16(message.asCString()), duration.asInt());
        }
    }

    // hide message
    const auto& hideMessageVal = msg["hideMessage"];
    if (hideMessageVal.isBool() && hideMessageVal.asBool()) {
        textService_->hideMessage();
    }
}

void Client::updateCommitString(Json::Value& msg, Ime::EditSession* session) {
    // handle comosition and commit strings
    const auto& commitStringVal = msg["commitString"];
    if (commitStringVal.isString()) {
        std::wstring commitString = utf8ToUtf16(commitStringVal.asCString());
        if (!commitString.empty()) {
            if (!textService_->isComposing()) {
                textService_->startComposition(session->context());
            }
            textService_->setCompositionString(session, commitString.c_str(), commitString.length());
            // FIXME: update the position of candidate and message window when the composition string is changed.
            if (textService_->candidateWindow_ != nullptr) {
                textService_->updateCandidatesWindow(session);
            }
            if (textService_->messageWindow_ != nullptr) {
                textService_->updateMessageWindow(session);
            }
            textService_->endComposition(session->context());
        }
    }
}

void Client::updateComposition(Json::Value& msg, Ime::EditSession* session, bool& endComposition) {
    const auto& compositionStringVal = msg["compositionString"];
    bool emptyComposition = false;
    bool hasCompositionString = false;
    std::wstring compositionString;
    if (compositionStringVal.isString()) {
        // composition buffer
        compositionString = utf8ToUtf16(compositionStringVal.asCString());
        hasCompositionString = true;
        if (compositionString.empty()) {
            emptyComposition = true;
            if (textService_->isComposing() && !textService_->showingCandidates()) {
                // when the composition buffer is empty and we are not showing the candidate list, end composition.
                textService_->setCompositionString(session, L"", 0);
                endComposition = true;
            }
        }
        else {
            if (!textService_->isComposing()) {
                textService_->startComposition(session->context());
            }
            textService_->setCompositionString(session, compositionString.c_str(), compositionString.length());
        }
        // FIXME: update the position of candidate and message window when the composition string is changed.
        if (textService_->candidateWindow_ != nullptr) {
            textService_->updateCandidatesWindow(session);
        }
        if (textService_->messageWindow_ != nullptr) {
            textService_->updateMessageWindow(session);
        }
    }

    const auto& compositionCursorVal = msg["compositionCursor"];
    if (compositionCursorVal.isInt()) {
        // composition cursor
        if (!emptyComposition) {
            int compositionCursor = compositionCursorVal.asInt();
            if (!textService_->isComposing()) {
                textService_->startComposition(session->context());
            }
            // NOTE:
            // This fixes PIME bug #166: incorrect handling of UTF-16 surrogate pairs.
            // The TSF API unfortunately treat a UTF-16 surrogate pair as two characters while
            // they actually represent one unicode character only. To workaround this TSF bug,
            // we get the composition string, and try to move the cursor twice when a UTF-16
            // surrogate pair is found.
            if (!hasCompositionString)
                compositionString = textService_->compositionString(session);
            int fixedCursorPos = 0;
            for (int i = 0; i < compositionCursor; ++i) {
                ++fixedCursorPos;
                if (IS_HIGH_SURROGATE(compositionString[i])) // this is the first part of a UTF16 surrogate pair (Windows uses UTF16-LE)
                    ++fixedCursorPos;
            }
            textService_->setCompositionCursor(session, fixedCursorPos);
        }
    }
    if (endComposition) {
        textService_->endComposition(session->context());
    }
}

void Client::updateLanguageButtons(Json::Value& msg) {
    // language buttons
    const auto& addButtonVal = msg["addButton"];
    if (addButtonVal.isArray()) {
        for (const auto& btn : addButtonVal) {
            // FIXME: when to clear the id <=> button map??
            auto langBtn = Ime::ComPtr<PIME::LangBarButton>::takeover(PIME::LangBarButton::fromJson(textService_, btn));
            if (langBtn != nullptr) {
                buttons_.emplace(langBtn->id(), langBtn); // insert into the map
                textService_->addButton(langBtn);
            }
        }
    }

    const auto& removeButtonVal = msg["removeButton"];
    if (removeButtonVal.isArray()) {
        // FIXME: handle windows-mode-icon
        for (const auto& btn : removeButtonVal) {
            if (btn.isString()) {
                string id = btn.asString();
                auto map_it = buttons_.find(id);
                if (map_it != buttons_.end()) {
                    textService_->removeButton(map_it->second);
                    buttons_.erase(map_it); // remove from the map
                }
            }
        }
    }
    const auto& changeButtonVal = msg["changeButton"];
    if (changeButtonVal.isArray()) {
        // FIXME: handle windows-mode-icon
        for (const auto& btn : changeButtonVal) {
            if (btn.isObject()) {
                string id = btn["id"].asString();
                auto map_it = buttons_.find(id);
                if (map_it != buttons_.end()) {
                    map_it->second->updateFromJson(btn);
                }
            }
        }
    }
}

void Client::updatePreservedKeys(Json::Value& msg) {
    const auto& addPreservedKeyVal = msg["addPreservedKey"];
    if (addPreservedKeyVal.isArray()) {
        // preserved keys
        for (auto& key : addPreservedKeyVal) {
            if (key.isObject()) {
                UINT keyCode = key["keyCode"].asUInt();
                UINT modifiers = key["modifiers"].asUInt();
                UUID guid = { 0 };
                if (uuidFromString(key["guid"].asCString(), guid)) {
                    textService_->addPreservedKey(keyCode, modifiers, guid);
                }
            }
        }
    }

    const auto& removePreservedKeyVal = msg["removePreservedKey"];
    if (removePreservedKeyVal.isArray()) {
        for (auto& item : removePreservedKeyVal) {
            if (item.isString()) {
                UUID guid = { 0 };
                if (uuidFromString(item.asCString(), guid)) {
                    textService_->removePreservedKey(guid);
                }
            }
        }
    }
}

void Client::updateKeyboardStatus(Json::Value& msg) {
    const auto& openKeyboardVal = msg["openKeyboard"];
    if (openKeyboardVal.isBool()) {
        textService_->setKeyboardOpen(openKeyboardVal.asBool());
    }
}

void Client::updateStatus(Json::Value& msg, Ime::EditSession* session) {
	// We need to handle ordering of some types of the requests.
	// For example, setCompositionCursor() should happen after setCompositionCursor().
    updateSelectionKeys(msg);

	// show message
    bool endComposition = false;
    updateMessageWindow(msg, session, endComposition);

	if (session != nullptr) { // if an edit session is available
        updateCandidateList(msg, session);

        updateCommitString(msg, session);

        updateComposition(msg, session, endComposition);

	}

    updateLanguageButtons(msg);

	// preserved keys
    updatePreservedKeys(msg);

	// keyboard status
    updateKeyboardStatus(msg);

	// other configurations
	const auto& customizeUIVal = msg["customizeUI"];
	if (customizeUIVal.isObject()) {
		// customize the UI
		updateUI(customizeUIVal);
	}
}

void Client::updateCandidateList(Json::Value& msg, Ime::EditSession* session) {
    // handle candidate list
    const auto& showCandidatesVal = msg["showCandidates"];
    if (showCandidatesVal.isBool()) {
        if (showCandidatesVal.asBool()) {
            // start composition if we are not composing.
            // this is required to get correctly position the candidate window
            if (!textService_->isComposing()) {
                textService_->startComposition(session->context());
            }
            textService_->showCandidates(session);
        }
        else {
            textService_->hideCandidates();
        }
    }

    const auto& candidateListVal = msg["candidateList"];
    if (candidateListVal.isArray()) {
        // handle candidates
        // FIXME: directly access private member is dirty!!!
        vector<wstring>& candidates = textService_->candidates_;
        candidates.clear();
        for (const auto& candidate : candidateListVal) {
            candidates.emplace_back(utf8ToUtf16(candidate.asCString()));
        }
        textService_->updateCandidates(session);
        if (!showCandidatesVal.asBool()) {
            textService_->hideCandidates();
        }
    }

    const auto& candidateCursorVal = msg["candidateCursor"];
    if (candidateCursorVal.isInt()) {
        if (textService_->candidateWindow_ != nullptr) {
            textService_->candidateWindow_->setCurrentSel(candidateCursorVal.asInt());
            textService_->refreshCandidates();
        }
    }
}

// handlers for the text service
void Client::onActivate() {
    Json::Value req = createRpcRequest("onActivate");
	req["isKeyboardOpen"] = textService_->isKeyboardOpened();

	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret)) {
	}
	isActivated_ = true;
}

void Client::onDeactivate() {
    Json::Value req = createRpcRequest("onDeactivate");
	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret)) {
	}
	LangBarButton::clearIconCache();
	isActivated_ = false;
}

bool Client::filterKeyDown(Ime::KeyEvent& keyEvent) {
    Json::Value req = createRpcRequest("filterKeyDown");
	addKeyEventToRpcRequest(req, keyEvent);

	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret)) {
		return ret["return"].asBool();
	}
	return false;
}

bool Client::onKeyDown(Ime::KeyEvent& keyEvent, Ime::EditSession* session) {
    RpcSessionScope scope(*this, session);  // PYIME:断线清理时可在此会话中清空组字串
    Json::Value req = createRpcRequest("onKeyDown");
	addKeyEventToRpcRequest(req, keyEvent);

	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret, session)) {
		return ret["return"].asBool();
	}
	return false;
}

bool Client::filterKeyUp(Ime::KeyEvent& keyEvent) {
    Json::Value req = createRpcRequest("filterKeyUp");
	addKeyEventToRpcRequest(req, keyEvent);

	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret)) {
		return ret["return"].asBool();
	}
	return false;
}

bool Client::onKeyUp(Ime::KeyEvent& keyEvent, Ime::EditSession* session) {
    RpcSessionScope scope(*this, session);  // PYIME:断线清理时可在此会话中清空组字串
    Json::Value req = createRpcRequest("onKeyUp");
	addKeyEventToRpcRequest(req, keyEvent);

	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret, session)) {
		return ret["return"].asBool();
	}
	return false;
}

bool Client::onPreservedKey(const GUID& guid) {
    auto guidStr = uuidToString(guid);
	if (!guidStr.empty()) {
        Json::Value req = createRpcRequest("onPreservedKey");
        req["guid"] = std::move(guidStr);

		Json::Value ret;
		callRpcMethod(req, ret);
		if (handleRpcResponse(ret)) {
			return ret["return"].asBool();
		}
	}
	return false;
}

bool Client::onCommand(UINT id, Ime::TextService::CommandType type) {
    Json::Value req = createRpcRequest("onCommand");
	req["id"] = id;
	req["type"] = type;

	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret)) {
		return ret["return"].asBool();
	}
	return false;
}

bool Client::sendOnMenu(std::string button_id, Json::Value& result) {
    Json::Value req = createRpcRequest("onMenu");
	req["id"] = button_id;

	callRpcMethod(req, result);
	if (handleRpcResponse(result)) {
		return true;
	}
	return false;
}

static bool menuFromJson(ITfMenu* pMenu, const Json::Value& menuInfo) {
	if (pMenu != nullptr && menuInfo.isArray()) {
		for (const auto& item: menuInfo) {
			UINT id = item.get("id", 0).asUInt();
			std::wstring text = utf8ToUtf16(item.get("text", "").asCString());
			
			DWORD flags = 0;
			Json::Value submenuInfo;
			ITfMenu* submenu = nullptr;
			if (id == 0 && text.empty())
				flags = TF_LBMENUF_SEPARATOR;
			else {
				if (item.get("checked", false).asBool())
					flags |= TF_LBMENUF_CHECKED;
				if (!item.get("enabled", true).asBool())
					flags |= TF_LBMENUF_GRAYED;

				submenuInfo = item["submenu"];  // FIXME: this is a deep copy. too bad! :-(
				if (submenuInfo.isArray()) {
					flags |= TF_LBMENUF_SUBMENU;
				}
			}
			pMenu->AddMenuItem(id, flags, NULL, NULL, text.c_str(), text.length(), flags & TF_LBMENUF_SUBMENU ? &submenu : nullptr);
			if (submenu != nullptr && submenuInfo.isArray()) {
				menuFromJson(submenu, submenuInfo);
			}
		}
		return true;
	}
	return false;
}

// called when a language bar button needs a menu
// virtual
bool Client::onMenu(LangBarButton* btn, ITfMenu* pMenu) {
	Json::Value result;
	if(sendOnMenu(btn->id(), result)) {
		Json::Value& menuInfo = result["return"];
		return menuFromJson(pMenu, menuInfo);
	}
	return false;
}

static HMENU menuFromJson(const Json::Value& menuInfo) {
	if (menuInfo.isArray()) {
		HMENU menu = ::CreatePopupMenu();
		for (const auto& item: menuInfo) {
			UINT id = item.get("id", 0).asUInt();
			std::wstring text = utf8ToUtf16(item.get("text", "").asCString());

			UINT flags = MF_STRING;
			if (id == 0 && text.empty())
				flags = MF_SEPARATOR;
			else {
				if (item.get("checked", false).asBool())
					flags |= MF_CHECKED;
				if (!item.get("enabled", true).asBool())
					flags |= MF_GRAYED;

				const Json::Value& subMenuValue = item.get("submenu", Json::nullValue);
				if (subMenuValue.isArray()) {
					HMENU submenu = menuFromJson(subMenuValue);
					flags |= MF_POPUP;
					id = UINT_PTR(submenu);
				}
			}
			AppendMenu(menu, flags, id, text.c_str());
		}
		return menu;
	}
	return NULL;
}

// called when a language bar button needs a menu
// virtual
HMENU Client::onMenu(LangBarButton* btn) {
	Json::Value result;
	if (sendOnMenu(btn->id(), result)) {
		Json::Value& menuInfo = result["return"];
		return menuFromJson(menuInfo);
	}
	return NULL;
}

// called when a compartment value is changed
void Client::onCompartmentChanged(const GUID& key) {
    auto guidStr = uuidToString(key);
	if (!guidStr.empty()) {
        Json::Value req = createRpcRequest("onCompartmentChanged");
        req["guid"] = std::move(guidStr);

		Json::Value ret;
		callRpcMethod(req, ret);
		if (handleRpcResponse(ret)) {
		}
	}
}

// called when the keyboard is opened or closed
void Client::onKeyboardStatusChanged(bool opened) {
    Json::Value req = createRpcRequest("onKeyboardStatusChanged");
	req["opened"] = opened;

	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret)) {
	}
}

// called just before current composition is terminated for doing cleanup.
void Client::onCompositionTerminated(bool forced) {
    Json::Value req = createRpcRequest("onCompositionTerminated");
	req["forced"] = forced;

	Json::Value ret;
	callRpcMethod(req, ret);
	if (handleRpcResponse(ret)) {
	}
}

bool Client::init() {
    Json::Value req = createRpcRequest("init");
	req["id"] = guid_.c_str();  // language profile guid
	req["isWindows8Above"] = ::IsWindows8OrGreater();
	req["isMetroApp"] = textService_->isMetroApp();
	req["isUiLess"] = textService_->isUiLess();
	req["isConsole"] = textService_->isConsole();

	Json::Value ret;
    return callRpcMethod(req, ret) && handleRpcResponse(ret);
}


Json::Value Client::createRpcRequest(const char* methodName) {
    Json::Value request;
    request["method"] = methodName;

    // TODO: add other environment info?
    return request;
}

// PYIME:复位事件与 OVERLAPPED,供下一次 WriteFile/ReadFile 使用
OVERLAPPED* Client::startIo() {
    ::ResetEvent(ioEvent_);
    overlapped_ = OVERLAPPED();
    overlapped_.hEvent = ioEvent_;
    return &overlapped_;
}

// PYIME:等待 startIo() 发起的操作;started 为 WriteFile/ReadFile 的返回值。
// 成功返回 true(error 为 ERROR_SUCCESS,或 ERROR_MORE_DATA 表示消息还没读完);
// 超时则 CancelIoEx,并用 GetOverlappedResult(bWait=TRUE) 等取消真正完成后才返回 false(error = WAIT_TIMEOUT),
// 此后 readBuf_/overlapped_ 才可复用——否则内核可能迟到写入,破坏宿主进程内存。
bool Client::finishIo(BOOL started, DWORD timeoutMs, DWORD& transferred, DWORD& error) {
    transferred = 0;
    error = started ? ERROR_SUCCESS : ::GetLastError();
    if (!started && error != ERROR_IO_PENDING && error != ERROR_MORE_DATA)
        return false;  // 没有挂起的 I/O
    if (error == ERROR_IO_PENDING && ::WaitForSingleObject(ioEvent_, timeoutMs) != WAIT_OBJECT_0) {
        ::CancelIoEx(pipe_, &overlapped_);
        ::GetOverlappedResult(pipe_, &overlapped_, &transferred, TRUE);
        error = WAIT_TIMEOUT;
        return false;
    }
    if (::GetOverlappedResult(pipe_, &overlapped_, &transferred, FALSE)) {
        error = ERROR_SUCCESS;
        return true;
    }
    error = ::GetLastError();
    return error == ERROR_MORE_DATA;
}

// PYIME:写一条请求消息、读完整条回复消息,总耗时不超过 timeoutMs。失败时 lastIoError_ 记错误码。
bool Client::callRpcPipe(const std::string& serializedRequest, std::string& serializedReply, DWORD timeoutMs) {
    if (pipe_ == INVALID_HANDLE_VALUE || !ioEvent_) {
        lastIoError_ = ERROR_INVALID_HANDLE;
        return false;
    }
    const ULONGLONG deadline = ::GetTickCount64() + timeoutMs;
    auto remaining = [deadline]() -> DWORD {
        ULONGLONG now = ::GetTickCount64();
        return now >= deadline ? 0 : DWORD(deadline - now);
    };
    DWORD transferred = 0;
    BOOL started = ::WriteFile(pipe_, serializedRequest.data(), DWORD(serializedRequest.size()), nullptr, startIo());
    if (!finishIo(started, remaining(), transferred, lastIoError_))
        return false;
    for (;;) {
        started = ::ReadFile(pipe_, readBuf_, sizeof(readBuf_), nullptr, startIo());
        if (!finishIo(started, remaining(), transferred, lastIoError_))
            return false;
        serializedReply.append(readBuf_, transferred);
        // PYIME:回复累计超过 1 MiB(与服务端单条上限一致)按失败处理,防冒名管道让宿主分配大量内存
        if (serializedReply.size() > kMaxReplyBytes) {
            lastIoError_ = ERROR_MESSAGE_EXCEEDS_MAX_SIZE;
            return false;
        }
        if (lastIoError_ != ERROR_MORE_DATA)
            return true;
    }
}

// send the request to the server
// a sequence number will be added to the req object automatically.
bool Client::callRpcMethod(Json::Value& request, Json::Value & response) {
    if (shouldWaitConnection_ && !waitForRpcConnection()) {
        return false;  // PYIME:退避期内或连不上,调用方放行当前键
    }
    if (pipe_ == INVALID_HANDLE_VALUE) {
        return false;  // PYIME:init 握手途中已断线;不再触发 onRpcFailure,避免与 abandonComposition 循环
    }

    // Add a sequence number for the request.
    auto seqNum = nextSeqNum_++;
    request["seqNum"] = seqNum;

	Json::FastWriter writer;
	std::string serializedRequest = writer.write(request); // convert the json object to string
    std::string method = request["method"].asString();

    std::string serializedResponse;
    if (!callRpcPipe(serializedRequest, serializedResponse, kRpcTimeoutMs)) {
        onRpcFailure(method.c_str(), lastIoError_ == WAIT_TIMEOUT ? "超时" : "读写失败");
        return false;
    }
	Json::Reader reader;
	if (!reader.parse(serializedResponse, response) || response["seqNum"].asUInt() != seqNum) {
        onRpcFailure(method.c_str(), "回复无法解析或 seqNum 不符");
        return false;
	}
    backoffSucceed();
	return true;
}

// PYIME:请求失败(超时/读写失败/回复异常)= 断线。顺序要紧:先关管道并进入退避,再结束组字——
// endComposition 会回调 onCompositionTerminated → callRpcMethod,此时退避生效,它会立即返回而不再连。
void Client::onRpcFailure(const char* method, const char* what) {
    closeRpcConnection();
    DWORD delay = backoffFail();
    tsfLog("%s 请求失败(%s,err=%lu),断开;%lu ms 内不重连", method, what, (unsigned long)lastIoError_, (unsigned long)delay);
    resetTextServiceState();  // since we lost the connection, the state is unknonw so we reset.
    abandonComposition();
}

// PYIME:断线时清理界面(同 TextService::onKeyboardStatusChanged 关闭键盘时的做法):
// 结束组字、藏候选窗与消息窗,避免随后放行的按键插到残留的组字串旁。
void Client::abandonComposition() {
    if (abandoning_)
        return;
    abandoning_ = true;
    if (textService_->isComposing()) {
        if (rpcSession_) {  // 在按键的编辑会话里:同 updateComposition 置空再结束(PIMEClient.cpp:182-183 的做法)
            textService_->setCompositionString(rpcSession_, L"", 0);
            textService_->endComposition(rpcSession_->context());
        } else if (auto context = textService_->currentContext()) {
            // PYIME:无会话(如 filterKeyDown 路径)时自行请求编辑会话,在会话内置空再结束,避免拼音残留。
            // OnTestKeyDown 内同步读写请求可能被拒,用 TF_ES_ASYNCDONTCARE:能同步则同步,否则排队。
            // lambda 持有 TextService 的 ComPtr(AddRef),不捕获裸 this;排队执行时先确认仍在组字。
            Ime::ComPtr<TextService> ts(textService_);
            auto session = Ime::ComPtr<Ime::EditSession>::make(
                context,
                [ts](Ime::EditSession* es, TfEditCookie) {
                    if (ts->isComposing()) {
                        ts->setCompositionString(es, L"", 0);
                        ts->endComposition(es->context());
                    }
                }
            );
            HRESULT hrSession = S_OK;
            HRESULT hr = context->RequestEditSession(textService_->clientId(), session,
                TF_ES_ASYNCDONTCARE | TF_ES_READWRITE, &hrSession);
            if (FAILED(hr) || FAILED(hrSession))
                tsfLog("断线清理:RequestEditSession 失败(hr=0x%08lx,session=0x%08lx)", (unsigned long)hr, (unsigned long)hrSession);
        }
    }
    if (textService_->showingCandidates())
        textService_->hideCandidates();
    textService_->hideMessage();
    abandoning_ = false;
}

// establish a connection to the specified pipe and returns its handle
// PYIME:重叠模式打开,收发可超时;不做对端校验。
HANDLE Client::connectPipe(const wchar_t* pipeName, int timeoutMs) {
    HANDLE pipe = INVALID_HANDLE_VALUE;
    if (WaitNamedPipe(pipeName, timeoutMs)) {
        // PYIME:不要 GENERIC_WRITE(含 FILE_APPEND_DATA = 建管道实例权限),服务端 SDDL 不授 AC 此权限,
        // 否则 AppContainer 宿主打不开;FILE_WRITE_ATTRIBUTES 供下面 SetNamedPipeHandleState。
        pipe = CreateFile(pipeName, GENERIC_READ | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES, 0, NULL, OPEN_EXISTING, FILE_FLAG_OVERLAPPED, NULL);
    }

    if (pipe != INVALID_HANDLE_VALUE) {
        DWORD mode = PIPE_READMODE_MESSAGE;
        // The pipe is connected; change to message-read mode.
        if (!::SetNamedPipeHandleState(pipe, &mode, NULL, NULL)) {
            CloseHandle(pipe);
            pipe = INVALID_HANDLE_VALUE;
        }
    }
	return pipe;
}


// Ensure that we're connected to the PYIME server.
// PYIME:只连一次、最多等 kConnectTimeoutMs;失败进入退避,退避期内直接返回 false(按键放行)。
bool Client::waitForRpcConnection() {
    if (pipe_ != INVALID_HANDLE_VALUE) {
        return true;
    }
    if (backoffBlocked()) {
        return false;
    }

    pipe_ = connectPipe(getPipeName().c_str(), kConnectTimeoutMs);
    if (pipe_ == INVALID_HANDLE_VALUE) {
        DWORD err = ::GetLastError();
        DWORD delay = backoffFail();
        tsfLog("连接 PYIME 服务端失败(err=%lu);%lu ms 内不重连", (unsigned long)err, (unsigned long)delay);
        return false;
    }
    tsfLog("已连接 PYIME 服务端");

    // send initialization info to the server for hand-shake.
    shouldWaitConnection_ = false;  // prevent recursive call of waitForRpcConnection
    if (!init()) {
        closeRpcConnection();
        shouldWaitConnection_ = true;
        return false;
    }

    if (isActivated_) {
        // we lost connection while being activated previously
        // re-initialize the whole text service.
        // activate the text service again.
        onActivate();
    }
    shouldWaitConnection_ = true;
    // if init() or onActivate() RPC fails, the pipe_ might have been closed.
    return pipe_ != INVALID_HANDLE_VALUE;
}

void Client::resetTextServiceState() {
    // we lost connection while being activated previously
    // re-initialize the whole text service.

    // cleanup for the previous instance.
    // remove all buttons

    // some language bar buttons are not unregistered properly
    if (!buttons_.empty()) {
        for (auto& item : buttons_) {
            textService_->removeButton(item.second);
        }
        buttons_.clear();
    }
}

void Client::closeRpcConnection() {
	if (pipe_ != INVALID_HANDLE_VALUE) {
		DisconnectNamedPipe(pipe_);
		CloseHandle(pipe_);
		pipe_ = INVALID_HANDLE_VALUE;
	}
}

// PYIME:\\.\pipe\<用户名>\PYIME(含用户名,避免多用户串话)
wstring Client::getPipeName() {
	wstring pipeName = L"\\\\.\\pipe\\";
	DWORD len = 0;
	::GetUserNameW(NULL, &len); // get the required size of the buffer
	if (len <= 0)
		return wstring();
	unique_ptr<wchar_t[]> username(new wchar_t[len]);
	if (!::GetUserNameW(username.get(), &len))
		return wstring();
	pipeName += username.get();
	pipeName += L"\\PYIME";
	return pipeName;
}


} // namespace PIME
