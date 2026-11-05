// PYIME:首键暂存补发(纯逻辑,无 Windows 依赖,可在 Linux 上单测)。设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md。
// 中文状态下字母键落在禁用+空上下文(Chromium 焦点切换的时间差)时先吃下暂存,等焦点切换、隔间变化、
// 下一个键或超时后,由调用方用一次 SendInput 原序重发。时间单位毫秒,由调用方传入(GetTickCount64)。
#ifndef PYIME_FIRST_KEY_STASH_H
#define PYIME_FIRST_KEY_STASH_H

#include <cstddef>
#include <cstdint>
#include <vector>

namespace PIME {

// Text:数字、标点、空格、回车、退格(暂存期间吃掉、排队补发);Other:修饰键、F 键、方向键等(不吃,只触发补发)
enum class KeyKind { Letter, Text, Other };

struct StashKey {
    uint32_t vk;
    uint32_t lParam;
    bool shift;  // 入队时 Shift 是否按着,补发时按此还原
};

struct KeyIn {
    uint32_t vk;
    uint32_t lParam;
    KeyKind kind;
    bool shift;
    bool injected;   // 带本模块的补发标记
    bool stashable;  // 调用方判定满足暂存条件(开关开、中文、未组字、无修饰键、非重复、禁用+空上下文的字母)
};

enum class StashReason { Focus, Compartment, NextKey, Timeout };

struct StashDecision {
    bool eat = false;    // 吃掉这个键
    bool flush = false;  // 调用方应立即 takeFlush 并补发
};

inline bool isRepeatKey(uint32_t lParam) { return (lParam & (1u << 30)) != 0; }

// 一次注入事件
struct Stroke {
    uint32_t vk;
    uint32_t scan;
    bool extended;
    bool up;
};

// 补发序列:先抬起用户按着的 Shift,逐键按入队时的 Shift 状态临时按下/抬起左 Shift,最后还原。
inline std::vector<Stroke> buildReplay(const std::vector<StashKey>& keys, bool lshiftHeld, bool rshiftHeld) {
    constexpr uint32_t kL = 0xA0, kLScan = 0x2A, kR = 0xA1, kRScan = 0x36;
    std::vector<Stroke> s;
    if (lshiftHeld) s.push_back({kL, kLScan, false, true});
    if (rshiftHeld) s.push_back({kR, kRScan, false, true});
    bool shift = false;
    for (const StashKey& k : keys) {
        if (k.shift != shift) {
            s.push_back({kL, kLScan, false, !k.shift});
            shift = k.shift;
        }
        const uint32_t scan = (k.lParam >> 16) & 0xff;
        const bool ext = (k.lParam & (1u << 24)) != 0;
        s.push_back({k.vk, scan, ext, false});
        s.push_back({k.vk, scan, ext, true});
    }
    if (shift) s.push_back({kL, kLScan, false, true});
    if (rshiftHeld) s.push_back({kR, kRScan, false, false});
    if (lshiftHeld) s.push_back({kL, kLScan, false, false});
    return s;
}

class FirstKeyStash {
public:
    static constexpr uint64_t kTimeoutMs = 500;
    static constexpr uint64_t kInFlightMaxMs = 1000;

    StashDecision keyDown(const KeyIn& k, uint64_t nowMs) {
        // 同一键先后经 OnTestKeyDown、OnKeyDown 到达:第二次沿用第一次的决定,不重复计数
        if (hasLast_ && k.vk == lastVk_ && k.lParam == lastLParam_ && k.injected == lastInjected_) {
            hasLast_ = false;
            return {lastEat_, false};
        }
        const StashDecision d = decide(k, nowMs);
        hasLast_ = true;
        lastVk_ = k.vk;
        lastLParam_ = k.lParam;
        lastInjected_ = k.injected;
        lastEat_ = d.eat;
        return d;
    }

    // 按键抬起:此后同一键再按下是新的一次,不再当第二入口
    void keyUp(uint32_t vk) {
        if (hasLast_ && vk == lastVk_) hasLast_ = false;
    }

    // 焦点切换、隔间变化:暂存中则需补发
    bool trigger(StashReason r) {
        if (state_ != State::Stashing) return false;
        reason_ = r;
        return true;
    }

    // 定时轮询:暂存超时返回 true;在途超时放弃在途(takeAbandoned 为真),有下一轮键时返回 true
    bool poll(uint64_t nowMs) {
        if (state_ == State::Stashing && nowMs - startMs_ >= kTimeoutMs) {
            reason_ = StashReason::Timeout;
            return true;
        }
        if (state_ == State::InFlight && nowMs - inFlightSinceMs_ >= kInFlightMaxMs) {
            abandoned_ = true;
            inFlightLeft_ = 0;
            state_ = queue_.empty() ? State::Idle : State::Stashing;
            reason_ = StashReason::NextKey;
            return !queue_.empty();
        }
        return false;
    }

    // 取出待补发的键并进入在途;返回空表示无事可做
    std::vector<StashKey> takeFlush(uint64_t nowMs) {
        std::vector<StashKey> keys;
        keys.swap(queue_);
        if (keys.empty()) {
            if (state_ == State::Stashing) state_ = State::Idle;
            return keys;
        }
        waitedMs_ = nowMs - startMs_;
        state_ = State::InFlight;
        inFlightLeft_ = keys.size();
        inFlightSinceMs_ = nowMs;
        return keys;
    }

    // 失去焦点、失活:丢弃一切,返回丢弃的暂存键数
    size_t discard() {
        const size_t n = queue_.size();
        queue_.clear();
        state_ = State::Idle;
        inFlightLeft_ = 0;
        hasLast_ = false;
        return n;
    }

    bool idle() const { return state_ == State::Idle; }
    bool stashing() const { return state_ == State::Stashing; }
    bool inFlight() const { return state_ == State::InFlight; }
    StashReason reason() const { return reason_; }
    uint64_t waitedMs() const { return waitedMs_; }
    bool takeAbandoned() {
        const bool a = abandoned_;
        abandoned_ = false;
        return a;
    }

private:
    enum class State { Idle, Stashing, InFlight };

    StashDecision decide(const KeyIn& k, uint64_t nowMs) {
        switch (state_) {
        case State::Idle:
            if (!k.injected && k.stashable) {
                push(k, nowMs);
                state_ = State::Stashing;
                return {true, false};
            }
            return {};
        case State::Stashing:
            if (k.injected) return {};
            if (isRepeatKey(k.lParam)) {
                reason_ = StashReason::NextKey;
                return {false, true};
            }
            if (k.stashable) {
                push(k, nowMs);
                return {true, false};
            }
            reason_ = StashReason::NextKey;
            if (k.kind != KeyKind::Other) {
                push(k, nowMs);
                return {true, true};
            }
            return {false, true};
        case State::InFlight:
            if (k.injected) {
                if (inFlightLeft_ > 0 && --inFlightLeft_ == 0) {
                    if (queue_.empty()) {
                        state_ = State::Idle;
                        return {};
                    }
                    state_ = State::Stashing;
                    reason_ = StashReason::NextKey;
                    return {false, true};
                }
                return {};
            }
            // 调用 SendInput 前已在队列里的物理键会先于注入键到达:文本类一律排到下一轮,保证顺序
            if (k.kind != KeyKind::Other && !isRepeatKey(k.lParam)) {
                push(k, nowMs);
                return {true, false};
            }
            return {};
        }
        return {};
    }

    void push(const KeyIn& k, uint64_t nowMs) {
        if (queue_.empty()) startMs_ = nowMs;
        queue_.push_back({k.vk, k.lParam, k.shift});
    }

    State state_ = State::Idle;
    std::vector<StashKey> queue_;  // 暂存中为本轮,在途中为下一轮
    uint64_t startMs_ = 0;
    uint64_t waitedMs_ = 0;
    uint64_t inFlightSinceMs_ = 0;
    size_t inFlightLeft_ = 0;
    StashReason reason_ = StashReason::Timeout;
    bool abandoned_ = false;
    bool hasLast_ = false;
    uint32_t lastVk_ = 0;
    uint32_t lastLParam_ = 0;
    bool lastInjected_ = false;
    bool lastEat_ = false;
};

}

#endif
