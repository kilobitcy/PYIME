// PYIME:漏键诊断。本线程 WH_GETMESSAGE 钩子(不是全局钩子)看到字母键,而输入法按键入口始终没收到时,
// 在 tsf.log 记一条(不记是哪个键)。判定逻辑在 PIMETextService/MissedKeyWatch.h,设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md 需求 2。
// 取出 WM_CHAR 时若对应字母键按下未经输入法,拦下字符、经首键暂存补发(设计 2026-11-08-missed-key-replay-design.md)。
#include "TextService.h"
#include <imm.h>
#include <cstdio>

namespace Ime {

namespace {
thread_local TextService* t_watchOwner = nullptr;  // 钩子回调在本线程执行
}

void TextService::installKeyWatch() {
    if (keyWatchHook_)
        return;
    t_watchOwner = this;
    keyWatchHook_ = ::SetWindowsHookExW(WH_GETMESSAGE, keyWatchProc, nullptr, ::GetCurrentThreadId());
    if (!keyWatchHook_)
        onDiagnostic("漏键诊断:钩子安装失败");
}

void TextService::uninstallKeyWatch() {
    if (keyWatchHook_) {
        ::UnhookWindowsHookEx(keyWatchHook_);
        keyWatchHook_ = nullptr;
    }
    if (t_watchOwner == this)
        t_watchOwner = nullptr;
}

LRESULT CALLBACK TextService::keyWatchProc(int code, WPARAM wParam, LPARAM lParam) {
    if (code == HC_ACTION && (wParam & PM_REMOVE) && t_watchOwner) {
        MSG* msg = reinterpret_cast<MSG*>(lParam);
        if (msg->message == WM_KEYDOWN)
            t_watchOwner->keyWatchDown(msg->wParam);
        else if (msg->message == WM_KEYUP)
            t_watchOwner->keyWatchUp(msg->wParam);
        else if (msg->message == WM_CHAR && t_watchOwner->keyWatchChar(msg->wParam, msg->lParam))
            msg->message = WM_NULL;  // 拦下:字母不进文本,改由补发走输入法
    }
    return ::CallNextHookEx(nullptr, code, wParam, lParam);
}

void TextService::keyWatchDown(WPARAM vk) {
    const bool eligible = vk >= 'A' && vk <= 'Z' && isChineseMode()
        && ::GetKeyState(VK_CONTROL) >= 0 && ::GetKeyState(VK_MENU) >= 0 && ::GetKeyState(VK_SHIFT) >= 0;
    keyWatch_.hookDown((uint32_t)vk, eligible, ::GetTickCount64());
}

void TextService::keyWatchUp(WPARAM vk) {
    const uint64_t now = ::GetTickCount64();
    if (!keyWatch_.hookUp((uint32_t)vk, now))
        return;
    bool hasDoc = false, hasImc = false;
    focusInputState(hasDoc, hasImc);
    char since[32] = "无";
    if (lastCommitMs_)
        std::snprintf(since, sizeof(since), "%llu ms", (unsigned long long)(now - lastCommitMs_));
    char line[160];
    std::snprintf(line, sizeof(line), "字母键未经输入法:焦点文档=%s 窗口输入法上下文=%s 距上次上屏 %s",
        hasDoc ? "有" : "无", hasImc ? "有" : "无", since);
    onDiagnostic(line);
}

// 焦点文档(TSF)与焦点窗口的输入法上下文(IMM)是否存在
void TextService::focusInputState(bool& hasDoc, bool& hasImc) const {
    ComPtr<ITfDocumentMgr> docMgr;
    hasDoc = threadMgr_ && threadMgr_->GetFocus(&docMgr) == S_OK && docMgr;
    hasImc = false;
    if (HWND focus = ::GetFocus()) {
        if (HIMC imc = ::ImmGetContext(focus)) {
            hasImc = true;
            ::ImmReleaseContext(focus, imc);
        }
    }
}

// 取出 WM_CHAR:对应字母键按下未经输入法时,把该键排入首键暂存补发。返回 true 表示调用方应拦下该消息。
// 只在焦点文档与输入法上下文都在时拦(实机证据的形态;密码框等本就不进输入法的窗口不拦);
// 焦点不在本线程不拦(补发会进别处);在途时又有键未经输入法(多半是补发的键)即停用暂存,免得循环、打乱顺序。
// 延后补发期间焦点移走(失去焦点、补发时校验失败)或 SendInput 失败,按首键暂存原有语义丢弃并记日志
bool TextService::keyWatchChar(WPARAM ch, LPARAM lParam) {
    if (!firstKeyStashEnabled() || !isChineseMode() || isComposing() || stash_.off())
        return false;
    bool hasDoc = false, hasImc = false;
    focusInputState(hasDoc, hasImc);
    if (!hasDoc || !hasImc || !stashFocusOnThisThread())
        return false;
    const uint64_t now = ::GetTickCount64();
    const uint32_t vk = keyWatch_.hookChar((uint32_t)ch, now);
    if (stash_.inFlight()) {
        // 补发的键(物理键仍按着时即其 pending 已补发)或另一个键在补发期间未经输入法:停用并立即收尾在途,
        // 已排下一轮的键马上补发,不等 1 s 兜底;这个字符放行
        if (vk || keyWatch_.replayedHeld((uint32_t)ch)) {
            onDiagnostic("字母键未经输入法:补发期间仍未经输入法,本会话停用暂存");
            if (stash_.disable())
                flushStash();
            syncStash();
        }
        return false;
    }
    if (!vk)
        return false;
    // 扫描码与扩展键标志取自 WM_CHAR 的 lParam(与按下时相同),重复计数置 1
    const PIME::StashKey k{vk, ((uint32_t)lParam & 0x01FF0000u) | 1u, false};
    // 不立即补发(刚取得焦点时紧随的注入键同样未经输入法):200 ms 后或下一个键到输入法时补发
    if (!stash_.addMissed(k, now).eat)
        return false;
    syncStash();
    onDiagnostic("字母键未经输入法:已拦下字符,延后补发");
    return true;
}

}  // namespace Ime
