// PYIME:请求失败后是否立即重连并重发一次(纯逻辑,无 Windows 依赖,可在 Linux 上单测)。
// 只对按键类请求、"管道已断"类错误、且无组字时重发:超时说明服务端卡住,重发会让宿主 UI 线程再冻一轮;
// 有组字时新会话里没有这段拼音,与 DLL 的组字状态对不上;非按键请求(如宿主退出时的 onDeactivate)不值得重连。
#ifndef PYIME_RPC_RETRY_H
#define PYIME_RPC_RETRY_H

#include <cstdint>
#include <cstring>
#include <initializer_list>

namespace PIME {

// Win32 错误码(winerror.h),写成数值以便脱离 Windows 头文件单测
constexpr uint32_t kErrorBrokenPipe = 109;        // ERROR_BROKEN_PIPE
constexpr uint32_t kErrorNoData = 232;            // ERROR_NO_DATA
constexpr uint32_t kErrorPipeNotConnected = 233;  // ERROR_PIPE_NOT_CONNECTED

inline bool isPipeGone(uint32_t err) {
    return err == kErrorBrokenPipe || err == kErrorNoData || err == kErrorPipeNotConnected;
}

inline bool isKeyMethod(const char* method) {
    for (const char* m : {"filterKeyDown", "onKeyDown", "filterKeyUp", "onKeyUp"})
        if (std::strcmp(method, m) == 0)
            return true;
    return false;
}

inline bool shouldRetryRpc(const char* method, uint32_t err, bool composing) {
    return !composing && isKeyMethod(method) && isPipeGone(err);
}

}

#endif
