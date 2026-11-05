// PYIME:漏键诊断。本线程 WH_GETMESSAGE 钩子(不是全局钩子)看到字母键,而输入法按键入口始终没收到时,
// 在 tsf.log 记一条(不记是哪个键)。判定逻辑在 PIMETextService/MissedKeyWatch.h,设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md 需求 2。
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
        const MSG* msg = reinterpret_cast<const MSG*>(lParam);
        if (msg->message == WM_KEYDOWN)
            t_watchOwner->keyWatchDown(msg->wParam);
        else if (msg->message == WM_KEYUP)
            t_watchOwner->keyWatchUp(msg->wParam);
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
    ComPtr<ITfDocumentMgr> docMgr;
    const bool hasDoc = threadMgr_ && threadMgr_->GetFocus(&docMgr) == S_OK && docMgr;
    bool hasImc = false;
    if (HWND focus = ::GetFocus()) {
        if (HIMC imc = ::ImmGetContext(focus)) {
            hasImc = true;
            ::ImmReleaseContext(focus, imc);
        }
    }
    char since[32] = "无";
    if (lastCommitMs_)
        std::snprintf(since, sizeof(since), "%llu ms", (unsigned long long)(now - lastCommitMs_));
    char line[160];
    std::snprintf(line, sizeof(line), "字母键未经输入法:焦点文档=%s 窗口输入法上下文=%s 距上次上屏 %s",
        hasDoc ? "有" : "无", hasImc ? "有" : "无", since);
    onDiagnostic(line);
}

}  // namespace Ime
