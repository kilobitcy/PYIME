// PIME::MissedKeyWatch 的本机单测(Linux g++ 编译运行,由 scripts/build-tsf.sh 调用)。
#include "MissedKeyWatch.h"
#include <cstdio>

static int failures = 0;
#define CHECK(cond) do { if (!(cond)) { std::printf("失败 %s:%d: %s\n", __FILE__, __LINE__, #cond); ++failures; } } while (0)

int main() {
    using PIME::MissedKeyWatch;
    {   // 钩子先看到、输入法随后收到:不报
        MissedKeyWatch w;
        w.hookDown('D', true, 100);
        w.sinkSaw('D', 101);
        CHECK(!w.hookUp('D', 150));
    }
    {   // 输入法先收到、钩子后看到(钩子顺序不定):不报
        MissedKeyWatch w;
        w.sinkSaw('D', 99);
        w.hookDown('D', true, 100);
        CHECK(!w.hookUp('D', 150));
    }
    {   // 输入法始终没收到:报
        MissedKeyWatch w;
        w.hookDown('D', true, 100);
        CHECK(w.hookUp('D', 150));
    }
    {   // 不符合条件(英文状态、带修饰键):不报
        MissedKeyWatch w;
        w.hookDown('D', false, 100);
        CHECK(!w.hookUp('D', 150));
    }
    {   // 很久以前收到过同一键不算
        MissedKeyWatch w;
        w.sinkSaw('D', 100);
        w.hookDown('D', true, 5000);
        CHECK(w.hookUp('D', 5050));
    }
    {   // 自动重复只看第一次按下;一秒内至多报一条
        MissedKeyWatch w;
        w.hookDown('D', true, 1000);
        w.hookDown('D', true, 1030);
        CHECK(w.hookUp('D', 1100));
        w.hookDown('U', true, 1200);
        CHECK(!w.hookUp('U', 1250));
        w.hookDown('I', true, 2200);
        CHECK(w.hookUp('I', 2250));
    }
    {   // 没看到按下的抬起、越界键码:不报
        MissedKeyWatch w;
        CHECK(!w.hookUp('D', 100));
        w.hookDown(300, true, 100);
        CHECK(!w.hookUp(300, 150));
    }
    if (failures == 0) std::printf("missed_key_watch_test: 全部通过\n");
    return failures == 0 ? 0 : 1;
}
