// PYIME:首键暂存补发的 TSF 接线。状态机在 PIMETextService/FirstKeyStash.h,设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md。
#include "TextService.h"
#include <cstdio>
#include <vector>

namespace Ime {

namespace {

constexpr ULONG_PTR kStashMagic = 0x50594D45;  // "PYME":补发按键的 dwExtraInfo 标记
constexpr UINT kStashPollMs = 50;
constexpr const char* kStashAbandonText = "首键暂存:补发的键 1 s 内未收齐,放弃在途,本会话停用暂存";
// TIMERPROC 在建定时器的线程上执行,每线程至多一个 TextService
thread_local TextService* t_stashOwner = nullptr;

bool isInjected() { return (ULONG_PTR)::GetMessageExtraInfo() == kStashMagic; }
bool isShiftVk(WPARAM vk) { return vk == VK_SHIFT || vk == VK_LSHIFT || vk == VK_RSHIFT; }
bool held(int vk) { return ::GetKeyState(vk) < 0; }
bool down(int vk) { return ::GetAsyncKeyState(vk) < 0; }

PIME::KeyKind kindOf(WPARAM vk) {
    if (held(VK_CONTROL) || held(VK_MENU))
        return PIME::KeyKind::Other;  // 快捷键不吃
    // 编辑/导航键:吃掉排队、原序补发,免得 Tab 抢在字母前把焦点带走
    if (vk == VK_TAB || vk == VK_ESCAPE || vk == VK_DELETE || vk == VK_INSERT || vk == VK_PRIOR || vk == VK_NEXT
        || vk == VK_END || vk == VK_HOME || vk == VK_LEFT || vk == VK_UP || vk == VK_RIGHT || vk == VK_DOWN)
        return PIME::KeyKind::Text;
    if (vk >= 'A' && vk <= 'Z')
        return PIME::KeyKind::Letter;
    if ((vk >= '0' && vk <= '9') || (vk >= VK_NUMPAD0 && vk <= VK_DIVIDE) || vk == VK_SPACE || vk == VK_RETURN
        || vk == VK_BACK || (vk >= VK_OEM_1 && vk <= VK_OEM_3) || (vk >= VK_OEM_4 && vk <= VK_OEM_8) || vk == VK_OEM_102)
        return PIME::KeyKind::Text;
    return PIME::KeyKind::Other;
}

const char* reasonText(PIME::StashReason r) {
    switch (r) {
    case PIME::StashReason::Focus: return "焦点切换";
    case PIME::StashReason::Compartment: return "隔间变化";
    case PIME::StashReason::NextKey: return "下一个键";
    case PIME::StashReason::Timeout: return "超时";
    }
    return "?";
}

}  // namespace

// 取指定上下文的隔间值(既有 contextCompartmentValue 忽略 context 参数,查的是焦点上下文)
DWORD TextService::stashCompartmentValue(const GUID& key, ITfContext* context) const {
    if (!context)
        return 0;
    auto compartment = contextCompartment(key, context);
    return compartment ? compartmentValue(compartment) : 0;
}

// 返回 true 表示本次按键已处理完(*pfEaten 已设),调用方直接返回
bool TextService::stashKeyDown(ITfContext* context, WPARAM wParam, LPARAM lParam, BOOL* pfEaten, bool fromTest) {
    keyWatch_.sinkSaw((uint32_t)wParam, ::GetTickCount64());
    const bool injected = isInjected();
    if (injected && isShiftVk(wParam)) {  // 补发时临时抬起/按回的 Shift:不经服务端,免得被当成单击 Shift 切英文
        *pfEaten = FALSE;
        return true;
    }
    const uint64_t now = ::GetTickCount64();
    const bool pollFlush = stash_.poll(now);
    if (stash_.takeAbandoned())  // 先取放弃标记:flushStash 可能 discard,discard 会清它
        onDiagnostic(kStashAbandonText);
    if (pollFlush)
        flushStash();

    const bool blocked = isKeyboardDisabled(context) || !isKeyboardOpened();
    PIME::KeyIn k{};
    k.vk = (uint32_t)wParam;
    k.lParam = (uint32_t)lParam;
    k.kind = kindOf(wParam);
    k.shift = held(VK_SHIFT);
    k.injected = injected;
    k.stashable = !injected && k.kind == PIME::KeyKind::Letter && !k.shift && !PIME::isRepeatKey(k.lParam)
        && firstKeyStashEnabled() && isChineseMode() && !isComposing() && isKeyboardOpened()
        && stashCompartmentValue(GUID_COMPARTMENT_KEYBOARD_DISABLED, context) != 0
        && stashCompartmentValue(GUID_COMPARTMENT_EMPTYCONTEXT, context) != 0
        && !(stashSkipContext_ && context == stashSkipContext_);

    const bool wasIdle = stash_.idle();
    // OnTestKeyDown 与 OnKeyDown 之间夹着 filterKeyDown RPC,可能超过状态机 50 ms 去重窗:
    // 同一键在 OnKeyDown 里沿用 OnTestKeyDown 的时刻
    uint64_t keyMs = now;
    if (fromTest) {
        hasTestKey_ = true;
        testVk_ = (uint32_t)wParam;
        testLParam_ = (uint32_t)lParam;
        testKeyMs_ = now;
    }
    else if (hasTestKey_ && testVk_ == (uint32_t)wParam && testLParam_ == (uint32_t)lParam) {
        hasTestKey_ = false;
        keyMs = testKeyMs_;
    }
    const PIME::StashDecision d = stash_.keyDown(k, keyMs);
    if (injected && !d.eat && context && stashSkipContext_ != context
        && stashCompartmentValue(GUID_COMPARTMENT_KEYBOARD_DISABLED, context) != 0
        && stashCompartmentValue(GUID_COMPARTMENT_EMPTYCONTEXT, context) != 0) {
        stashSkipContext_ = context;
        onDiagnostic("首键暂存:补发后仍为禁用上下文,焦点切换前不再暂存");
    }
    if (wasIdle && stash_.stashing())
        watchStashContext(context);
    if (d.flush)
        flushStash();
    syncStash();
    if (d.eat) {
        if (!injected && k.shift)
            stashSkipShiftUp_ = true;  // 物理 Shift 抬起放行给程序、不经服务端,免得被当成单击 Shift 切英文
        *pfEaten = TRUE;
        return true;
    }
    if (injected && blocked) {  // 补发后上下文仍禁用:直接放行,不再报"字母键未交给输入法"
        if (!stashPassLogged_) {
            stashPassLogged_ = true;
            onDiagnostic("首键暂存:补发后仍为禁用上下文,放行");
        }
        *pfEaten = FALSE;
        return true;
    }
    return false;
}

bool TextService::stashKeyUp(WPARAM wParam, BOOL* pfEaten) {
    const bool injected = isInjected();
    if (injected && isShiftVk(wParam)) {  // 补发时注入的 Shift:不经服务端
        *pfEaten = FALSE;
        return true;
    }
    if (!injected && isShiftVk(wParam) && stashSkipShiftUp_) {
        stashSkipShiftUp_ = false;
        *pfEaten = FALSE;
        return true;
    }
    if (hasTestKey_ && testVk_ == (uint32_t)wParam)
        hasTestKey_ = false;
    stash_.keyUp((uint32_t)wParam);
    return false;
}

// 暂存开始:监听按键上下文的两个隔间,任一变为 0 即补发(Chromium 可能只改隔间、不换文档管理器)
void TextService::watchStashContext(ITfContext* context) {
    stashDisabledSink_.unadvise();
    stashEmptySink_.unadvise();
    stashContext_ = context;
    if (!context)
        return;
    if (auto source = contextCompartment(GUID_COMPARTMENT_KEYBOARD_DISABLED, context).query<ITfSource>())
        stashDisabledSink_ = SinkAdvice(source, IID_ITfCompartmentEventSink, (ITfCompartmentEventSink*)this);
    if (auto source = contextCompartment(GUID_COMPARTMENT_EMPTYCONTEXT, context).query<ITfSource>())
        stashEmptySink_ = SinkAdvice(source, IID_ITfCompartmentEventSink, (ITfCompartmentEventSink*)this);
}

bool TextService::stashCompartmentChanged(REFGUID rguid) {
    if (!IsEqualGUID(rguid, GUID_COMPARTMENT_KEYBOARD_DISABLED) && !IsEqualGUID(rguid, GUID_COMPARTMENT_EMPTYCONTEXT))
        return false;
    if (stashContext_ && stashCompartmentValue(rguid, stashContext_) == 0
        && stash_.trigger(PIME::StashReason::Compartment))
        flushStash();
    syncStash();
    return true;  // 只有暂存才监听这两个隔间,不转给服务端
}

void TextService::flushStash() {
    const uint64_t now = ::GetTickCount64();
    const PIME::StashReason reason = stash_.reason();
    const std::vector<PIME::StashKey> keys = stash_.takeFlush(now);
    if (keys.empty())
        return;
    // 前台已换到别的线程(Alt+Tab 等):丢弃,不把字母注入到别的程序
    HWND fg = ::GetForegroundWindow();
    if (!fg || ::GetWindowThreadProcessId(fg, nullptr) != ::GetCurrentThreadId()) {
        stash_.discard();
        onDiagnostic("首键暂存:前台已不是本线程,丢弃");
        return;
    }
    char line[128];
    std::snprintf(line, sizeof(line), "首键暂存:%s 等待 %llu ms 共 %u 键", reasonText(reason),
        (unsigned long long)stash_.waitedMs(), (unsigned)keys.size());
    onDiagnostic(line);

    std::vector<INPUT> inputs;
    const PIME::HeldMods mods{down(VK_LSHIFT), down(VK_RSHIFT), down(VK_LCONTROL), down(VK_RCONTROL),
        down(VK_LMENU), down(VK_RMENU), down(VK_LWIN), down(VK_RWIN)};
    for (const PIME::Stroke& s : PIME::buildReplay(keys, mods)) {
        INPUT in{};
        in.type = INPUT_KEYBOARD;
        in.ki.wVk = (WORD)s.vk;
        in.ki.wScan = (WORD)s.scan;
        in.ki.dwFlags = (s.up ? KEYEVENTF_KEYUP : 0) | (s.extended ? KEYEVENTF_EXTENDEDKEY : 0);
        in.ki.dwExtraInfo = kStashMagic;
        inputs.push_back(in);
    }
    stashPassLogged_ = false;
    if (::SendInput((UINT)inputs.size(), inputs.data(), sizeof(INPUT)) != inputs.size()) {
        stash_.discard();
        onDiagnostic("首键暂存:SendInput 失败,丢弃");
    }
}

void TextService::discardStash(const char* why) {
    const size_t n = stash_.discard();
    if (n) {
        char line[96];
        std::snprintf(line, sizeof(line), "首键暂存:%s,丢弃 %u 键", why, (unsigned)n);
        onDiagnostic(line);
    }
    stashSkipShiftUp_ = false;
    stashSkipContext_ = nullptr;
    syncStash();
}

// 按状态收放隔间监听与轮询定时器
void TextService::syncStash() {
    if (!stash_.stashing()) {
        stashDisabledSink_.unadvise();
        stashEmptySink_.unadvise();
        stashContext_ = nullptr;
    }
    if (stash_.idle()) {
        if (stashTimer_) {
            ::KillTimer(nullptr, stashTimer_);
            stashTimer_ = 0;
        }
        if (t_stashOwner == this)
            t_stashOwner = nullptr;
    }
    else if (!stashTimer_) {
        t_stashOwner = this;
        stashTimer_ = ::SetTimer(nullptr, 0, kStashPollMs, onStashTimer);
    }
}

void CALLBACK TextService::onStashTimer(HWND, UINT, UINT_PTR, DWORD) {
    TextService* self = t_stashOwner;
    if (!self)
        return;
    const bool pollFlush = self->stash_.poll(::GetTickCount64());
    if (self->stash_.takeAbandoned())
        self->onDiagnostic(kStashAbandonText);
    if (pollFlush)
        self->flushStash();
    self->syncStash();
}

}  // namespace Ime
