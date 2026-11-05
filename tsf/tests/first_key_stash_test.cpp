// PIME::FirstKeyStash 的本机单测(Linux g++ 编译运行,由 scripts/build-tsf.sh 调用)。
#include "FirstKeyStash.h"
#include <cstdio>

static int failures = 0;
#define CHECK(cond) do { if (!(cond)) { std::printf("失败 %s:%d: %s\n", __FILE__, __LINE__, #cond); ++failures; } } while (0)

using namespace PIME;
static const uint32_t kLp = 0x00200001;           // 扫描码 0x20、重复计数 1
static const uint32_t kRepeat = kLp | (1u << 30);  // 自动重复
static KeyIn stashable(uint32_t vk, uint32_t lp = kLp) { return {vk, lp, KeyKind::Letter, false, false, true}; }
static KeyIn normal(uint32_t vk, KeyKind kind = KeyKind::Letter, uint32_t lp = kLp) { return {vk, lp, kind, false, false, false}; }
static KeyIn injected(uint32_t vk) { return {vk, kLp, KeyKind::Letter, false, true, false}; }

int main() {
    {   // 空闲时不可暂存的键照常处理
        FirstKeyStash s;
        StashDecision d = s.keyDown(normal('D'), 10);
        CHECK(!d.eat && !d.flush && s.idle());
    }
    {   // 可暂存即吃掉;同一键第二入口沿用决定、不重复入队;超时补发
        FirstKeyStash s;
        CHECK(s.keyDown(stashable('D'), 10).eat);
        CHECK(s.keyDown(stashable('D'), 10).eat);
        CHECK(s.stashing());
        CHECK(!s.poll(509));
        CHECK(s.poll(510) && s.reason() == StashReason::Timeout);
        std::vector<StashKey> k = s.takeFlush(510);
        CHECK(k.size() == 1 && k[0].vk == 'D' && s.inFlight() && s.waitedMs() == 500);
    }
    {   // 焦点切换触发补发;空闲时触发无效
        FirstKeyStash s;
        CHECK(!s.trigger(StashReason::Focus));
        s.keyDown(stashable('D'), 10);
        CHECK(s.trigger(StashReason::Focus) && s.reason() == StashReason::Focus);
    }
    {   // 暂存中来了正常上下文的字母:吃掉、排在后面、立即补发,顺序不乱
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        StashDecision d = s.keyDown(normal('U'), 20);
        CHECK(d.eat && d.flush && s.reason() == StashReason::NextKey);
        std::vector<StashKey> k = s.takeFlush(20);
        CHECK(k.size() == 2 && k[0].vk == 'D' && k[1].vk == 'U');
    }
    {   // 暂存中来了其他键(方向键等):不吃,立即补发
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        StashDecision d = s.keyDown(normal(0x25, KeyKind::Other), 20);
        CHECK(!d.eat && d.flush);
    }
    {   // 暂存中来了自动重复:不吃,立即补发
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        StashDecision d = s.keyDown(normal('D', KeyKind::Letter, kRepeat), 40);
        CHECK(!d.eat && d.flush);
    }
    {   // 在途:物理文本键排入下一轮;注入键按入口去重计数,收齐后补发下一轮
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.keyDown(normal('U'), 20);
        s.takeFlush(20);
        CHECK(s.keyDown(normal('I'), 21).eat);
        CHECK(!s.keyDown(injected('D'), 22).flush);
        CHECK(!s.keyDown(injected('D'), 22).flush);  // 第二入口不算
        s.keyUp('D');
        StashDecision d = s.keyDown(injected('U'), 23);
        CHECK(!d.eat && d.flush);
        std::vector<StashKey> k = s.takeFlush(23);
        CHECK(k.size() == 1 && k[0].vk == 'I' && s.inFlight());
        CHECK(!s.keyDown(injected('I'), 24).flush && s.idle());
    }
    {   // 在途超时放弃;有下一轮键时要求补发
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.trigger(StashReason::Focus);
        s.takeFlush(10);
        CHECK(!s.poll(1009));
        CHECK(!s.poll(1010) && s.takeAbandoned() && s.idle());
        CHECK(!s.takeAbandoned());
        s.keyDown(stashable('A'), 2000);
        s.trigger(StashReason::Focus);
        s.takeFlush(2000);
        s.keyDown(normal('B'), 2001);
        CHECK(s.poll(3000) && s.takeAbandoned());
        CHECK(s.takeFlush(3000).size() == 1);
    }
    {   // 抬起后同一键再按是新的一次,不当第二入口
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.keyUp('D');
        s.keyDown(stashable('D'), 30);
        CHECK(s.takeFlush(30).size() == 2);
    }
    {   // 丢弃
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.keyDown(stashable('U', 0x00160001), 11);
        CHECK(s.discard() == 2 && s.idle());
    }
    {   // 补发序列:无 Shift
        std::vector<Stroke> r = buildReplay({{'D', 0x01200001, false}}, false, false);
        CHECK(r.size() == 2 && r[0].vk == 'D' && !r[0].up && r[1].up && r[0].scan == 0x20 && r[0].extended);
    }
    {   // 补发序列:用户按着左 Shift,暂存键无 Shift → 先抬起、发完按回
        std::vector<Stroke> r = buildReplay({{'D', kLp, false}}, true, false);
        CHECK(r.size() == 4 && r[0].vk == 0xA0 && r[0].up && r[1].vk == 'D' && r[3].vk == 0xA0 && !r[3].up);
    }
    {   // 补发序列:暂存键带 Shift、用户已松开 → 临时按下、发完抬起;右 Shift 按着时最后按回
        std::vector<Stroke> r = buildReplay({{'U', kLp, true}}, false, true);
        CHECK(r.size() == 6);
        CHECK(r[0].vk == 0xA1 && r[0].up && r[1].vk == 0xA0 && !r[1].up && r[2].vk == 'U');
        CHECK(r[4].vk == 0xA0 && r[4].up && r[5].vk == 0xA1 && !r[5].up);
    }
    if (failures == 0) std::printf("first_key_stash_test: 全部通过\n");
    return failures == 0 ? 0 : 1;
}
