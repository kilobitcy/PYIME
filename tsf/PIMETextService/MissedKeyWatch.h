// PYIME:漏键诊断(纯逻辑,无 Windows 依赖,可在 Linux 上单测)。设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md 需求 2。
// 线程级消息钩子看到字母键按下,而输入法的按键入口始终没收到这个键(窗口无输入法上下文时系统不进 TSF),
// 到抬起时判为"漏给输入法"。钩子与输入法入口的先后不定,按时间窗判断;一秒内至多报一条。
// 钩子取出该键的 WM_CHAR 时可据 hookChar 拦下补发(设计 2026-11-08)。
#ifndef PYIME_MISSED_KEY_WATCH_H
#define PYIME_MISSED_KEY_WATCH_H

#include <cstdint>

namespace PIME {

struct CharKey {
    uint32_t vk;  // 0 表示不是输入法要接管的字符
    bool shift;
};

// 中文状态下输入法接管的字符在美式键盘上的键与 Shift:小写字母,以及服务端 punct.rs 会转成中文标点的符号。
// 其他布局下对不上按下的键,不会补发
inline CharKey keyForChar(uint32_t ch) {
    if (ch >= 'a' && ch <= 'z') return {ch - 'a' + 'A', false};
    switch (ch) {
    case ',': return {0xBC, false};
    case '.': return {0xBE, false};
    case ';': return {0xBA, false};
    case '\'': return {0xDE, false};
    case '[': return {0xDB, false};
    case ']': return {0xDD, false};
    case '\\': return {0xDC, false};
    case '`': return {0xC0, false};
    case '<': return {0xBC, true};
    case '>': return {0xBE, true};
    case ':': return {0xBA, true};
    case '"': return {0xDE, true};
    case '{': return {0xDB, true};
    case '}': return {0xDD, true};
    case '?': return {0xBF, true};
    case '!': return {'1', true};
    case '$': return {'4', true};
    case '(': return {'9', true};
    case ')': return {'0', true};
    }
    return {0, false};
}

class MissedKeyWatch {
public:
    static constexpr uint64_t kSeenWindowMs = 500;
    static constexpr uint64_t kReportGapMs = 1000;

    // 钩子取出 WM_KEYDOWN;eligible = 中文状态、无 Ctrl/Alt;shift = 按下时 Shift 是否按着
    void hookDown(uint32_t vk, bool eligible, uint64_t nowMs, bool shift = false) {
        if (vk >= 256) return;
        Pending& p = pending_[vk];
        if (p.active) return;  // 自动重复只看第一次
        p.active = true;
        p.eligible = eligible;
        p.shift = shift;
        p.downMs = nowMs;
    }

    // 输入法按键入口收到这个键
    void sinkSaw(uint32_t vk, uint64_t nowMs) {
        if (vk < 256) sawMs_[vk] = nowMs;
    }

    // 钩子取出 WM_CHAR:对应的键按下钩子看到过(500 ms 内、按下时符合条件、Shift 与该字符相符)而输入法入口没收到,
    // 返回该键的 vk 并记为已补发(每次按下至多一次);否则返回 0。只认 keyForChar 收录的字符
    uint32_t hookChar(uint32_t ch, uint64_t nowMs) {
        const CharKey k = keyForChar(ch);
        if (!k.vk) return 0;
        const uint32_t vk = k.vk;
        Pending& p = pending_[vk];
        if (!p.active || !p.eligible || p.shift != k.shift || p.replayed || nowMs - p.downMs > kSeenWindowMs) return 0;
        const uint64_t saw = sawMs_[vk];
        if (saw != 0 && saw + kSeenWindowMs >= p.downMs) return 0;
        p.replayed = true;
        return vk;
    }

    // 该键已拦下补发、物理键仍按着:此时再来它的 WM_CHAR,多半是补发的键再次未经输入法
    bool replayedHeld(uint32_t ch) const {
        const CharKey k = keyForChar(ch);
        if (!k.vk) return false;
        const Pending& p = pending_[k.vk];
        return p.active && p.replayed;
    }

    // 钩子取出 WM_KEYUP;返回 true 表示该记一条
    bool hookUp(uint32_t vk, uint64_t nowMs) {
        if (vk >= 256) return false;
        const Pending p = pending_[vk];
        pending_[vk] = Pending{};
        if (!p.active || !p.eligible || p.shift || vk < 'A' || vk > 'Z') return false;  // 只报不带 Shift 的字母
        if (p.replayed) return false;  // 已拦下补发,拦下时已记日志
        const uint64_t saw = sawMs_[vk];
        if (saw != 0 && saw + kSeenWindowMs >= p.downMs) return false;
        if (reported_ && nowMs - lastReportMs_ < kReportGapMs) return false;
        reported_ = true;
        lastReportMs_ = nowMs;
        return true;
    }

private:
    struct Pending {
        bool active = false;
        bool eligible = false;
        bool shift = false;
        bool replayed = false;  // 已拦下字符补发
        uint64_t downMs = 0;
    };
    Pending pending_[256];
    uint64_t sawMs_[256] = {};
    bool reported_ = false;
    uint64_t lastReportMs_ = 0;
};

}

#endif
