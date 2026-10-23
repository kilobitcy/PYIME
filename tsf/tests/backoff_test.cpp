// PIME::Backoff 的本机单测(Linux g++ 编译运行,由 scripts/build-tsf.sh 调用)。
#include "Backoff.h"
#include <cstdio>

static int failures = 0;
#define CHECK(cond) do { if (!(cond)) { std::printf("失败 %s:%d: %s\n", __FILE__, __LINE__, #cond); ++failures; } } while (0)

int main() {
    using PIME::Backoff;
    {   // 初始不退避
        Backoff b;
        CHECK(!b.blocked(0));
        CHECK(b.delayMs() == 0);
    }
    {   // 首次失败退避 1s,到点即可重试
        Backoff b;
        CHECK(b.fail(100) == 1000);
        CHECK(b.blocked(100));
        CHECK(b.blocked(1099));
        CHECK(!b.blocked(1100));
    }
    {   // 连续失败倍增,封顶 30s
        Backoff b;
        const unsigned expect[] = {1000, 2000, 4000, 8000, 16000, 30000, 30000};
        for (unsigned e : expect) CHECK(b.fail(0) == e);
    }
    {   // 成功一次清零,下次失败从 1s 重新开始
        Backoff b;
        b.fail(0);
        b.fail(0);
        b.succeed();
        CHECK(!b.blocked(0));
        CHECK(b.fail(0) == 1000);
    }
    if (failures == 0) std::printf("backoff_test: 全部通过\n");
    return failures == 0 ? 0 : 1;
}
