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
    }
    {   // 放弃在途时有下一轮键:要求补发(新对象,放弃后旧对象已停用)
        FirstKeyStash s;
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
    {   // 暂存中按下 Ctrl(Other):不吃,立即补发
        FirstKeyStash s;
        s.keyDown(stashable('W'), 10);
        StashDecision d = s.keyDown(normal(0xA2, KeyKind::Other), 20);
        CHECK(!d.eat && d.flush && s.reason() == StashReason::NextKey);
        CHECK(s.takeFlush(20).size() == 1);
    }
    {   // 在途时物理修饰键、方向键、自动重复不吃
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.trigger(StashReason::Focus);
        s.takeFlush(10);
        CHECK(!s.keyDown(normal(0xA2, KeyKind::Other), 11).eat);
        CHECK(!s.keyDown(normal(0x25, KeyKind::Other), 12).eat);
        CHECK(!s.keyDown(normal('U', KeyKind::Letter, kRepeat), 13).eat);
    }
    {   // 注入的屏蔽键、修饰键(Other)不计入在途:之后的物理字母仍排到下一轮
        FirstKeyStash s;
        s.keyDown(stashable('W'), 10);
        s.trigger(StashReason::Focus);
        s.takeFlush(10);
        KeyIn mask{0xE8, kLp, KeyKind::Other, false, true, false};
        CHECK(!s.keyDown(mask, 11).flush && s.inFlight());
        CHECK(s.keyDown(normal('X'), 12).eat);
        s.keyUp('X');
        StashDecision d = s.keyDown(injected('W'), 13);
        CHECK(!d.eat && d.flush);
        CHECK(s.takeFlush(13).size() == 1);
    }
    {   // 在途 1 s 未收齐:放弃,下一轮补发一次且不再进在途,此后停用暂存
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.trigger(StashReason::Focus);
        s.takeFlush(10);
        CHECK(s.keyDown(normal('U'), 20).eat);
        CHECK(s.poll(1011) && s.takeAbandoned() && s.off());
        CHECK(s.takeFlush(1011).size() == 1 && s.idle());
        CHECK(!s.keyDown(stashable('X'), 1020).eat && s.idle());
        CHECK(!s.keyDown(normal('U'), 1030).eat);
        s.discard();
        CHECK(s.off());
    }
    {   // 在途放弃时没有下一轮:回到空闲、停用
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.trigger(StashReason::Focus);
        s.takeFlush(10);
        CHECK(!s.poll(1010) && s.takeAbandoned() && s.off() && s.idle());
    }
    {   // discard 清掉未取的放弃标记(补发前 discard 时不留陈旧日志)
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.trigger(StashReason::Focus);
        s.takeFlush(10);
        s.poll(1010);
        s.discard();
        CHECK(!s.takeAbandoned() && s.off());
    }
    {   // 未经输入法:空闲时接受、不立即补发;200 ms 超时补发,原因为 Missed;补发后在途
        FirstKeyStash s;
        StashDecision d = s.addMissed({'H', kLp, false}, 10);
        CHECK(d.eat && !d.flush && s.stashing());
        CHECK(!s.poll(209));
        CHECK(s.poll(210) && s.reason() == StashReason::Missed);
        CHECK(s.takeFlush(210).size() == 1 && s.inFlight());
        CHECK(!s.addMissed({'H', kLp, false}, 220).eat);  // 在途时不接受
        CHECK(!s.keyDown(injected('H'), 230).eat && s.idle());
    }
    {   // 未经输入法后下一个键到了输入法:吃掉、排在其后立即补发
        FirstKeyStash s;
        s.addMissed({'G', kLp, false}, 10);
        StashDecision d = s.keyDown(normal('U'), 60);
        CHECK(d.eat && d.flush && s.reason() == StashReason::NextKey);
        std::vector<StashKey> k = s.takeFlush(60);
        CHECK(k.size() == 2 && k[0].vk == 'G' && k[1].vk == 'U');
    }
    {   // 未经输入法连续两键:都排队,按第一键起算 200 ms
        FirstKeyStash s;
        s.addMissed({'G', kLp, false}, 10);
        CHECK(s.addMissed({'U', kLp, false}, 100).eat);
        CHECK(!s.poll(209) && s.poll(210));
        CHECK(s.takeFlush(210).size() == 2);
    }
    {   // 首键暂存之后:超时仍为 500 ms、原因为 Timeout(不受上一轮未经输入法影响)
        FirstKeyStash s;
        s.addMissed({'G', kLp, false}, 10);
        s.poll(210);
        s.takeFlush(210);
        s.keyDown(injected('G'), 220);
        s.keyDown(stashable('D'), 300);
        CHECK(!s.poll(799) && s.poll(800) && s.reason() == StashReason::Timeout);
    }
    {   // 暂存中:排在已暂存字母之后,沿用首键暂存 500 ms 计时
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        StashDecision d = s.addMissed({'U', kLp, false}, 20);
        CHECK(d.eat && !d.flush && !s.poll(509) && s.poll(510));
        std::vector<StashKey> k = s.takeFlush(510);
        CHECK(k.size() == 2 && k[0].vk == 'D' && k[1].vk == 'U');
    }
    {   // disable:立即停用,不再暂存、不接受未经输入法的键
        FirstKeyStash s;
        s.disable();
        CHECK(s.off() && !s.keyDown(stashable('D'), 10).eat && !s.addMissed({'H', kLp, false}, 10).eat);
    }
    {   // 在途中 disable:立即收尾,下一轮键马上可取出且不再进在途;迟到的注入键不吃
        FirstKeyStash s;
        s.addMissed({'H', kLp, false}, 10);
        s.takeFlush(10);
        CHECK(s.keyDown(normal('U'), 20).eat);
        CHECK(s.disable() && s.off() && s.stashing());
        std::vector<StashKey> k = s.takeFlush(20);
        CHECK(k.size() == 1 && k[0].vk == 'U' && s.idle());
        CHECK(!s.keyDown(injected('H'), 30).eat);
    }
    {   // 停用后不接受
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.trigger(StashReason::Focus);
        s.takeFlush(10);
        s.poll(1010);
        CHECK(s.off() && !s.addMissed({'H', kLp, false}, 1020).eat);
    }
    {   // 补发序列:按着左 Ctrl、暂存键带 Shift 且用户已松开 Shift → Ctrl 抬、Shift 按、键、Shift 抬、Ctrl 按
        std::vector<Stroke> r = buildReplay({{'U', kLp, true}}, HeldMods{false, false, true, false, false, false, false, false});
        CHECK(r.size() == 6 && r[0].vk == 0xA2 && r[0].up && r[1].vk == 0xA0 && !r[1].up && r[2].vk == 'U');
        CHECK(r[4].vk == 0xA0 && r[4].up && r[5].vk == 0xA2 && !r[5].up);
    }
    {   // 下一个键吃掉并补发后,它的第二入口不重复入队
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        CHECK(s.keyDown(normal('U'), 20).eat);
        CHECK(s.takeFlush(20).size() == 2);
        StashDecision d = s.keyDown(normal('U'), 20);
        CHECK(d.eat && !d.flush);
        CHECK(s.takeFlush(20).empty());
    }
    {   // 去重时间窗:同一键超过 50 ms 再来(抬起丢了)算新的一次
        FirstKeyStash s;
        s.keyDown(stashable('D'), 10);
        s.keyDown(stashable('D'), 100);
        CHECK(s.takeFlush(100).size() == 2);
    }
    {   // 补发序列:无 Shift
        std::vector<Stroke> r = buildReplay({{'D', 0x01200001, false}}, HeldMods{});
        CHECK(r.size() == 2 && r[0].vk == 'D' && !r[0].up && r[1].up && r[0].scan == 0x20 && r[0].extended);
    }
    {   // 补发序列:用户按着左 Shift,暂存键无 Shift → 先抬起、发完按回
        std::vector<Stroke> r = buildReplay({{'D', kLp, false}}, HeldMods{true, false, false, false, false, false, false, false});
        CHECK(r.size() == 4 && r[0].vk == 0xA0 && r[0].up && r[1].vk == 'D' && r[3].vk == 0xA0 && !r[3].up);
    }
    {   // 补发序列:暂存键带 Shift、用户已松开 → 临时按下、发完抬起;右 Shift 按着时最后按回
        std::vector<Stroke> r = buildReplay({{'U', kLp, true}}, HeldMods{false, true, false, false, false, false, false, false});
        CHECK(r.size() == 6);
        CHECK(r[0].vk == 0xA1 && r[0].up && r[1].vk == 0xA0 && !r[1].up && r[2].vk == 'U');
        CHECK(r[4].vk == 0xA0 && r[4].up && r[5].vk == 0xA1 && !r[5].up);
    }
    {   // 按着左 Ctrl:先抬起、发完按回,无屏蔽键
        std::vector<Stroke> r = buildReplay({{'W', kLp, false}}, HeldMods{false, false, true, false, false, false, false, false});
        CHECK(r.size() == 4 && r[0].vk == 0xA2 && r[0].up && r[1].vk == 'W' && r[3].vk == 0xA2 && !r[3].up);
    }
    {   // 按着左 Alt:先发屏蔽键 0xE8,再抬起 Alt,发完按回
        std::vector<Stroke> r = buildReplay({{'W', kLp, false}}, HeldMods{false, false, false, false, true, false, false, false});
        CHECK(r.size() == 6 && r[0].vk == 0xE8 && !r[0].up && r[1].vk == 0xE8 && r[1].up);
        CHECK(r[2].vk == 0xA4 && r[2].up && r[3].vk == 'W' && r[5].vk == 0xA4 && !r[5].up);
    }
    {   // 按着右 Win 与左 Shift:一次屏蔽键;抬起顺序 Shift、Win,按回顺序 Win、Shift;右 Win 为扩展键
        std::vector<Stroke> r = buildReplay({{'W', kLp, false}}, HeldMods{true, false, false, false, false, false, false, true});
        CHECK(r.size() == 8 && r[0].vk == 0xE8 && r[2].vk == 0xA0 && r[2].up && r[3].vk == 0x5C && r[3].up && r[3].extended);
        CHECK(r[4].vk == 'W' && r[6].vk == 0x5C && !r[6].up && r[7].vk == 0xA0 && !r[7].up);
    }
    if (failures == 0) std::printf("first_key_stash_test: 全部通过\n");
    return failures == 0 ? 0 : 1;
}
