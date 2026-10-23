// PYIME:连接退避(纯逻辑,无 Windows 依赖,可在 Linux 上单测)。
// 连接失败、请求超时、断线共用一个计数:首次失败后 1s 内不再尝试,
// 之后每次失败间隔翻倍,上限 30s;成功一次即清零。时间单位毫秒,由调用方传入(GetTickCount64)。
#ifndef PYIME_BACKOFF_H
#define PYIME_BACKOFF_H

#include <cstdint>

namespace PIME {

class Backoff {
public:
    static constexpr uint32_t kFirstMs = 1000;
    static constexpr uint32_t kMaxMs = 30000;

    // nowMs 时刻是否仍在退避期内(此时不应尝试连接)
    bool blocked(uint64_t nowMs) const { return nowMs < retryAfterMs_; }

    // 记一次失败,返回本次退避时长
    uint32_t fail(uint64_t nowMs) {
        delayMs_ = delayMs_ == 0 ? kFirstMs : (delayMs_ >= kMaxMs / 2 ? kMaxMs : delayMs_ * 2);
        retryAfterMs_ = nowMs + delayMs_;
        return delayMs_;
    }

    void succeed() {
        delayMs_ = 0;
        retryAfterMs_ = 0;
    }

    uint32_t delayMs() const { return delayMs_; }

private:
    uint64_t retryAfterMs_ = 0;
    uint32_t delayMs_ = 0;
};

}

#endif
