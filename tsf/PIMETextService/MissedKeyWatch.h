// PYIME:漏键诊断(纯逻辑,无 Windows 依赖,可在 Linux 上单测)。设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md 需求 2。
// 线程级消息钩子看到字母键按下,而输入法的按键入口始终没收到这个键(窗口无输入法上下文时系统不进 TSF),
// 到抬起时判为"漏给输入法"。钩子与输入法入口的先后不定,按时间窗判断;一秒内至多报一条。
// 钩子取出该键的 WM_CHAR 时可据 hookChar 拦下补发(设计 2026-11-08)。
#ifndef PYIME_MISSED_KEY_WATCH_H
#define PYIME_MISSED_KEY_WATCH_H

#include <cstdint>

namespace PIME {

class MissedKeyWatch {
public:
    static constexpr uint64_t kSeenWindowMs = 500;
    static constexpr uint64_t kReportGapMs = 1000;

    // 钩子取出 WM_KEYDOWN;eligible = 中文状态、无修饰键的字母
    void hookDown(uint32_t vk, bool eligible, uint64_t nowMs) {
        if (vk >= 256) return;
        Pending& p = pending_[vk];
        if (p.active) return;  // 自动重复只看第一次
        p.active = true;
        p.eligible = eligible;
        p.downMs = nowMs;
    }

    // 输入法按键入口收到这个键
    void sinkSaw(uint32_t vk, uint64_t nowMs) {
        if (vk < 256) sawMs_[vk] = nowMs;
    }

    // 钩子取出 WM_CHAR:对应字母键按下钩子看到过(500 ms 内、按下时符合条件)而输入法入口没收到,
    // 返回该字母的 vk 并记为已补发(每次按下至多一次);否则返回 0。只认 a–z
    uint32_t hookChar(uint32_t ch, uint64_t nowMs) {
        if (ch < 'a' || ch > 'z') return 0;
        const uint32_t vk = ch - 'a' + 'A';
        Pending& p = pending_[vk];
        if (!p.active || !p.eligible || p.replayed || nowMs - p.downMs > kSeenWindowMs) return 0;
        const uint64_t saw = sawMs_[vk];
        if (saw != 0 && saw + kSeenWindowMs >= p.downMs) return 0;
        p.replayed = true;
        return vk;
    }

    // 该字母已拦下补发、物理键仍按着:此时再来它的 WM_CHAR,多半是补发的键再次未经输入法
    bool replayedHeld(uint32_t ch) const {
        if (ch < 'a' || ch > 'z') return false;
        const Pending& p = pending_[ch - 'a' + 'A'];
        return p.active && p.replayed;
    }

    // 钩子取出 WM_KEYUP;返回 true 表示该记一条
    bool hookUp(uint32_t vk, uint64_t nowMs) {
        if (vk >= 256) return false;
        const Pending p = pending_[vk];
        pending_[vk] = Pending{};
        if (!p.active || !p.eligible) return false;
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
