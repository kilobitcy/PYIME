// PIME::shouldRetryRpc 的本机单测(Linux g++ 编译运行,由 scripts/build-tsf.sh 调用)。
#include "RpcRetry.h"
#include <cstdio>
#include <initializer_list>

static int failures = 0;
#define CHECK(cond) do { if (!(cond)) { std::printf("失败 %s:%d: %s\n", __FILE__, __LINE__, #cond); ++failures; } } while (0)

int main() {
    using PIME::shouldRetryRpc;
    // 按键类请求 + 管道已断 + 无组字:重发
    for (const char* m : {"filterKeyDown", "onKeyDown", "filterKeyUp", "onKeyUp"}) {
        CHECK(shouldRetryRpc(m, 109, false));  // ERROR_BROKEN_PIPE
        CHECK(shouldRetryRpc(m, 232, false));  // ERROR_NO_DATA
        CHECK(shouldRetryRpc(m, 233, false));  // ERROR_PIPE_NOT_CONNECTED
    }
    // 超时(服务端卡住)、其他错误不重发
    CHECK(!shouldRetryRpc("filterKeyDown", 258, false));  // WAIT_TIMEOUT
    CHECK(!shouldRetryRpc("filterKeyDown", 0, false));
    CHECK(!shouldRetryRpc("filterKeyDown", 6, false));    // ERROR_INVALID_HANDLE
    // 有组字不重发
    CHECK(!shouldRetryRpc("onKeyDown", 109, true));
    // 非按键请求不重发
    for (const char* m : {"init", "onActivate", "onDeactivate", "onCommand", "onMenu", "onCompositionTerminated"})
        CHECK(!shouldRetryRpc(m, 109, false));
    if (failures == 0) std::printf("rpc_retry_test: 全部通过\n");
    return failures == 0 ? 0 : 1;
}
