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
    {   // 输入法没收到:WM_CHAR 时返回 vk,同一按下只返回一次;抬起不再报
        MissedKeyWatch w;
        w.hookDown('H', true, 100);
        CHECK(w.hookChar('h', 101) == 'H');
        CHECK(w.hookChar('h', 102) == 0);
        CHECK(!w.hookUp('H', 150));
    }
    {   // 输入法收到了、不符合条件、超过 500 ms、非小写字母、没看到按下:返回 0
        MissedKeyWatch w;
        w.hookDown('H', true, 100);
        w.sinkSaw('H', 100);
        CHECK(w.hookChar('h', 101) == 0);
        MissedKeyWatch w2;
        w2.hookDown('H', false, 100);
        CHECK(w2.hookChar('h', 101) == 0);
        MissedKeyWatch w3;
        w3.hookDown('H', true, 100);
        CHECK(w3.hookChar('h', 601) == 0);
        CHECK(w3.hookChar('H', 101) == 0 && w3.hookChar('1', 101) == 0);
        CHECK(w3.hookChar('u', 101) == 0);
    }
    if (failures == 0) std::printf("missed_key_watch_test: 全部通过\n");
    return failures == 0 ? 0 : 1;
}
