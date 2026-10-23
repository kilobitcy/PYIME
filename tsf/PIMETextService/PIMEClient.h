//
//	Copyright (C) 2014 Hong Jen Yee (PCMan) <pcman.tw@gmail.com>
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
// PYIME 修改:改连 PYIME 自有服务端管道;重叠 I/O 超时、退避、断线清理。

#ifndef _PIME_CLIENT_H_
#define _PIME_CLIENT_H_
#define NDEBUG
#include <libIME2/src/TextService.h>
#include <libIME2/src/KeyEvent.h>
#include <libIME2/src/EditSession.h>
#include "PIMELangBarButton.h"

#include <unordered_map>
#include <string>
#include <json/json.h>

namespace PIME {

class TextService;

class Client
{
public:
	// create a client connection to the input method server
	// guid is the GUID of the language profile
	Client(TextService* service, REFIID langProfileGuid);
	~Client(void);

	const std::string guid() const {
		return guid_;
	}

	// handlers for the text service
	void onActivate();
	void onDeactivate();

	bool filterKeyDown(Ime::KeyEvent& keyEvent);
	bool onKeyDown(Ime::KeyEvent& keyEvent, Ime::EditSession* session);

	bool filterKeyUp(Ime::KeyEvent& keyEvent);
	bool onKeyUp(Ime::KeyEvent& keyEvent, Ime::EditSession* session);

	bool onPreservedKey(const GUID& guid);

	bool onCommand(UINT id, Ime::TextService::CommandType type);

	// called when a language bar button needs a menu
	bool onMenu(LangBarButton* btn, ITfMenu* pMenu);

	// called when a language bar button needs a menu
	HMENU onMenu(LangBarButton* btn);

	// called when a compartment value is changed
	void onCompartmentChanged(const GUID& key);

	// called when the keyboard is opened or closed
	void onKeyboardStatusChanged(bool opened);

	// called just before current composition is terminated for doing cleanup.
	void onCompositionTerminated(bool forced);

private:
    static std::wstring getPipeName();
    HANDLE connectPipe(const wchar_t* pipeName, int timeoutMs);

    Json::Value createRpcRequest(const char* methodName);
	bool callRpcMethod(Json::Value& request, Json::Value& response);

    bool waitForRpcConnection();
    // PYIME:重叠 I/O 收发一条消息,总耗时不超过 timeoutMs
    bool callRpcPipe(const std::string& serializedRequest, std::string& serializedReply, DWORD timeoutMs);
    OVERLAPPED* startIo();
    bool finishIo(BOOL started, DWORD timeoutMs, DWORD& transferred, DWORD& error);
    void closeRpcConnection();
    // PYIME:请求失败即断线:关管道、进入退避、结束组字并藏窗
    void onRpcFailure(const char* method, const char* what);
    void abandonComposition();

	bool init();
    void resetTextServiceState();

	void addKeyEventToRpcRequest(Json::Value& request, Ime::KeyEvent& keyEvent);
    bool sendOnMenu(std::string button_id, Json::Value& result);

    bool handleRpcResponse(Json::Value& msg, Ime::EditSession* session = nullptr);

    // Update text service and UI status based on RPC responses.
    void updateSelectionKeys(Json::Value& msg);
    void updateMessageWindow(Json::Value& msg, Ime::EditSession* session, bool& endComposition);
	void updateCommitString(Json::Value& msg, Ime::EditSession* session);
	void updateComposition(Json::Value& msg, Ime::EditSession* session, bool& endComposition);
	void updateLanguageButtons(Json::Value& msg);
	void updatePreservedKeys(Json::Value& msg);
	void updateKeyboardStatus(Json::Value& msg);
    void updateCandidateList(Json::Value& msg, Ime::EditSession* session);
    void updateUI(const Json::Value& data);
    void updateStatus(Json::Value& msg, Ime::EditSession* session = nullptr);

private:
	TextService* textService_;
	std::string guid_;
	HANDLE pipe_;
    HANDLE ioEvent_;        // PYIME:重叠 I/O 完成事件(手动复位)
    OVERLAPPED overlapped_; // PYIME:同一时刻至多一个挂起 I/O
    char readBuf_[4096];    // PYIME:收包缓冲区为成员,取消完成前内核可能仍在写入,不能用栈变量
    DWORD lastIoError_;     // PYIME:最近一次 I/O 错误码,超时为 WAIT_TIMEOUT
    bool abandoning_;       // PYIME:防止 abandonComposition 重入
    Ime::EditSession* rpcSession_ = nullptr;  // PYIME:当前 RPC 所在的编辑会话,断线清理用
    struct RpcSessionScope {
        Client& c; Ime::EditSession* prev;
        RpcSessionScope(Client& c, Ime::EditSession* s) : c(c), prev(c.rpcSession_) { c.rpcSession_ = s; }
        ~RpcSessionScope() { c.rpcSession_ = prev; }
    };
	std::unordered_map<std::string, Ime::ComPtr<PIME::LangBarButton>> buttons_; // map buttons to string IDs
	unsigned int nextSeqNum_;
	bool isActivated_;
    bool shouldWaitConnection_;
};

}

#endif // _PIME_CLIENT_H_
