# 首键暂存补发 + 漏键诊断 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 中文状态下落在 Chromium 禁用+空上下文的首个字母键先暂存、再原序补发进组字;同时记录"字母键根本没交给输入法"(Mullion 路径)的诊断日志。

**Architecture:** 纯逻辑放 `tsf/PIMETextService/` 下两个无 Windows 依赖的头文件(`FirstKeyStash.h`、`MissedKeyWatch.h`),用本机 g++ 单测;TSF 接线放 libIME2 `TextService` 的两个新源文件(`TextServiceStash.cpp`、`TextServiceKeyWatch.cpp`),经虚函数向 PIME 层取"中文状态 / 开关"并写 tsf.log;后端只在 `init` 回复里带 `firstKeyStash` 开关并在重写 `fuzzy.txt` 时保留该行。

**Tech Stack:** C++17(mingw-w64 交叉编译,win32 线程模型,无 std::mutex)、TSF、Win32 SendInput / SetTimer / SetWindowsHookEx;Rust(pyime-backend)。

**设计文档:** `docs/superpowers/specs/2026-11-05-first-key-stash-design.md`(实现前通读)。

## Global Constraints

- 注释、日志、commit message 一律简体中文;commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 暂存超时 `kTimeoutMs` = 500;在途放弃 `kInFlightMaxMs` = 1000;轮询定时器 50 ms;补发标记 `dwExtraInfo` = `0x50594D45`。
- `fuzzy.txt` 中一行 `首键暂存` = 关闭;引擎 crate(`crates/pyime-engine`)不得改动。
- tsf.log 不记是哪个键(`tsf/PIMETextService/TsfLog.h` 口径)。
- mingw 无 std::mutex;每线程一个 TextService,定时器/钩子回调用 `thread_local` 找实例。
- 不改既有缺陷(`KeyEvent::scanCode()` 取值错误、托盘重写丢注释行),只在报告里提。
- 单文件单次写入 ≤ 约 150 行;大改动分多次 Edit。
- 不 push、不打 tag、不改版本号(发布前另问用户)。

## File Structure

| 文件 | 动作 | 职责 |
|---|---|---|
| `crates/pyime-backend/src/ime.rs` | 改 | `first_key_stash()`;`apply_fuzzy` 保留 `首键暂存` 行 |
| `crates/pyime-backend/src/session.rs` | 改 | `init` 回复带 `firstKeyStash` |
| `crates/pyime-backend/src/session/tests.rs` | 改 | 两条新测试 |
| `tsf/PIMETextService/FirstKeyStash.h` | 新建 | 暂存状态机 + `buildReplay` |
| `tsf/tests/first_key_stash_test.cpp` | 新建 | 状态机单测 |
| `tsf/PIMETextService/MissedKeyWatch.h` | 新建 | 漏键判定 + 限频 |
| `tsf/tests/missed_key_watch_test.cpp` | 新建 | 漏键判定单测 |
| `tsf/libIME2/src/TextService.h` | 改 | 新虚函数、成员 |
| `tsf/libIME2/src/TextService.cpp` | 改 | 按键入口、焦点、隔间、失活接线 |
| `tsf/libIME2/src/TextServiceStash.cpp` | 新建 | 暂存接线(SendInput、定时器、隔间监听) |
| `tsf/libIME2/src/TextServiceKeyWatch.cpp` | 新建 | 线程级 WH_GETMESSAGE 钩子 |
| `tsf/PIMETextService/PIMEClient.{h,cpp}` | 改 | 记 `firstKeyStash`,给出 `chineseMode()` |
| `tsf/PIMETextService/PIMETextService.{h,cpp}` | 改 | 覆写三个虚函数 |
| `scripts/build-tsf.sh` | 改 | 新源文件、新单测、`-limm32` |
| `test-kit/tsf.txt`、`CLAUDE.md` | 改 | 实机清单第十一节、说明 |

---

### Task 1: 后端 `firstKeyStash` 开关

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs`(`read_fuzzy` 附近加常量与函数;`apply_fuzzy` 约 390 行)
- Modify: `crates/pyime-backend/src/session.rs:242-248`(`"init"` 分支)
- Test: `crates/pyime-backend/src/session/tests.rs`(文件末尾追加)

**Interfaces:**
- Produces: `Ime::first_key_stash(&self) -> bool`;`init` 回复字段 `"firstKeyStash": bool`(Task 4 的 DLL 读它)。

- [ ] **Step 1: 写失败测试**(追加到 `session/tests.rs` 末尾)

```rust
#[test]
fn init回复带首键暂存开关_默认开() {
    let mut s = session();
    let r = s.handle(&json!({"method": "init", "seqNum": 1, "isWindows8Above": true}));
    assert_eq!(r["firstKeyStash"], true);
}

#[test]
fn fuzzy含首键暂存行即关闭_托盘重写后保留() {
    let p = crate::ime::temp_user_path("stash-off");
    let f = p.with_file_name("fuzzy.txt");
    std::fs::write(&f, "首键暂存\n").unwrap();
    let mut s = Session::new(Ime::with_user_path(engine(), p.clone()));
    let r = s.handle(&json!({"method": "init", "seqNum": 1, "isWindows8Above": true}));
    assert_eq!(r["firstKeyStash"], false);
    assert_eq!(menu_cmd(&mut s, tray::CMD_TYPO)["return"], true);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "纠错\n首键暂存\n");
    assert_eq!(menu_cmd(&mut s, tray::CMD_FUZZY + 4)["return"], true);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "r=l\n纠错\n首键暂存\n");
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -q -p pyime-backend 首键暂存`
Expected: 两条 FAIL(`r["firstKeyStash"]` 为 Null;重写后的文件缺 `首键暂存`)。

- [ ] **Step 3: 实现**

`ime.rs` 在 `fn read_fuzzy` 之前加:

```rust
/// `fuzzy.txt` 中表示关闭首键暂存补发的一行。属于前端功能,引擎不认识,由后端自己读写。
const STASH_OFF: &str = "首键暂存";

fn stash_off_in(text: &str) -> bool {
    text.trim_start_matches('\u{feff}').lines().any(|l| l.trim() == STASH_OFF)
}
```

`impl Ime` 内(`pub fn fuzzy` 旁)加:

```rust
    /// 首键暂存补发是否开启:`fuzzy.txt` 含 `首键暂存` 行即关闭。每次 `init` 现读,改文件后重启输入法生效。
    pub fn first_key_stash(&self) -> bool {
        !fuzzy_path(self.user_path.as_deref()).and_then(|p| read_optional(&p)).is_some_and(|t| stash_off_in(&t))
    }
```

`apply_fuzzy` 改为:

```rust
    fn apply_fuzzy(&self, cfg: FuzzyConfig) {
        let mut text = cfg.export_off();
        self.engine.borrow_mut().set_fuzzy(cfg);
        let Some(path) = fuzzy_path(self.user_path.as_deref()) else { return };
        // 托盘重写时保留用户手加的首键暂存开关
        if read_optional(&path).is_some_and(|t| stash_off_in(&t)) {
            text.push_str(STASH_OFF);
            text.push('\n');
        }
        let _ = path.parent().map(std::fs::create_dir_all);
        if let Err(e) = std::fs::write(&path, text) {
            crate::log::line(&format!("写入 {path:?} 失败: {e}"));
        }
    }
```

`session.rs` 的 `"init"` 分支末尾(`if let Some(english)` 块之后)加:

```rust
                resp["firstKeyStash"] = json!(self.ime.first_key_stash());
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -q -p pyime-backend`
Expected: 全部 PASS(含两条新测试),计数行 `test result: ok`。再跑 `cargo clippy -q -p pyime-backend` 无新告警。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/ime.rs crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "后端:init 回复带首键暂存开关,fuzzy.txt 含「首键暂存」即关闭,托盘重写时保留该行"
```

---
### Task 2: 暂存状态机 `FirstKeyStash.h`

> 复核后修订(2026-11-05,用户确认):新增 `KeyKind::Modifier` 与 `StashKey::downOnly`,去重加 50 ms 时间窗;以仓库中 `FirstKeyStash.h` 为准,下方代码为初版。

**Files:**
- Create: `tsf/PIMETextService/FirstKeyStash.h`
- Create: `tsf/tests/first_key_stash_test.cpp`
- Modify: `scripts/build-tsf.sh`(末尾"纯逻辑单测"段)

**Interfaces:**
- Produces(namespace `PIME`):`enum class KeyKind { Letter, Text, Other }`;`struct StashKey { uint32_t vk; uint32_t lParam; bool shift; }`;
  `struct KeyIn { uint32_t vk; uint32_t lParam; KeyKind kind; bool shift; bool injected; bool stashable; }`;
  `enum class StashReason { Focus, Compartment, NextKey, Timeout }`;`struct StashDecision { bool eat; bool flush; }`;
  `bool isRepeatKey(uint32_t lParam)`;`struct Stroke { uint32_t vk; uint32_t scan; bool extended; bool up; }`;
  `std::vector<Stroke> buildReplay(const std::vector<StashKey>&, bool lshiftHeld, bool rshiftHeld)`;
  `class FirstKeyStash` 方法:`keyDown(const KeyIn&, uint64_t) -> StashDecision`、`keyUp(uint32_t)`、`trigger(StashReason) -> bool`、
  `poll(uint64_t) -> bool`、`takeFlush(uint64_t) -> std::vector<StashKey>`、`discard() -> size_t`、`idle()/stashing()/inFlight() -> bool`、
  `reason() -> StashReason`、`waitedMs() -> uint64_t`、`takeAbandoned() -> bool`。

- [ ] **Step 1: 写失败测试** `tsf/tests/first_key_stash_test.cpp`

```cpp
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `g++ -std=c++17 -Wall -Itsf/PIMETextService tsf/tests/first_key_stash_test.cpp -o /tmp/fks && /tmp/fks`
Expected: 编译失败,`FirstKeyStash.h: No such file or directory`。

- [ ] **Step 3: 实现** `tsf/PIMETextService/FirstKeyStash.h`

```cpp
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
```

`scripts/build-tsf.sh` 末尾"纯逻辑单测"段追加:

```bash
g++ -std=c++17 -Wall -I"$SRC/PIMETextService" "$SRC/tests/first_key_stash_test.cpp" -o "$SHIM/first_key_stash_test"
"$SHIM/first_key_stash_test"
```

- [ ] **Step 4: 跑测试确认通过**

Run: `g++ -std=c++17 -Wall -Itsf/PIMETextService tsf/tests/first_key_stash_test.cpp -o /tmp/fks && /tmp/fks`
Expected: 无告警,输出 `first_key_stash_test: 全部通过`,退出码 0。

- [ ] **Step 5: Commit**

```bash
git add tsf/PIMETextService/FirstKeyStash.h tsf/tests/first_key_stash_test.cpp scripts/build-tsf.sh
git commit -m "前端:首键暂存状态机与补发序列(纯逻辑 + 本机单测)"
```

---

### Task 3: 漏键判定 `MissedKeyWatch.h`

**Files:**
- Create: `tsf/PIMETextService/MissedKeyWatch.h`
- Create: `tsf/tests/missed_key_watch_test.cpp`
- Modify: `scripts/build-tsf.sh`(纯逻辑单测段)

**Interfaces:**
- Produces(namespace `PIME`):`class MissedKeyWatch` 方法 `hookDown(uint32_t vk, bool eligible, uint64_t nowMs)`、
  `sinkSaw(uint32_t vk, uint64_t nowMs)`、`hookUp(uint32_t vk, uint64_t nowMs) -> bool`(true = 该记一条,已限频)。

- [ ] **Step 1: 写失败测试** `tsf/tests/missed_key_watch_test.cpp`

```cpp
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `g++ -std=c++17 -Wall -Itsf/PIMETextService tsf/tests/missed_key_watch_test.cpp -o /tmp/mkw && /tmp/mkw`
Expected: 编译失败,`MissedKeyWatch.h: No such file or directory`。

- [ ] **Step 3: 实现** `tsf/PIMETextService/MissedKeyWatch.h`

```cpp
// PYIME:漏键诊断(纯逻辑,无 Windows 依赖,可在 Linux 上单测)。设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md 需求 2。
// 线程级消息钩子看到字母键按下,而输入法的按键入口始终没收到这个键(窗口无输入法上下文时系统不进 TSF),
// 到抬起时判为"漏给输入法"。钩子与输入法入口的先后不定,按时间窗判断;一秒内至多报一条。
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

    // 钩子取出 WM_KEYUP;返回 true 表示该记一条
    bool hookUp(uint32_t vk, uint64_t nowMs) {
        if (vk >= 256) return false;
        const Pending p = pending_[vk];
        pending_[vk] = Pending{};
        if (!p.active || !p.eligible) return false;
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
        uint64_t downMs = 0;
    };
    Pending pending_[256];
    uint64_t sawMs_[256] = {};
    bool reported_ = false;
    uint64_t lastReportMs_ = 0;
};

}

#endif
```

`scripts/build-tsf.sh` 纯逻辑单测段追加:

```bash
g++ -std=c++17 -Wall -I"$SRC/PIMETextService" "$SRC/tests/missed_key_watch_test.cpp" -o "$SHIM/missed_key_watch_test"
"$SHIM/missed_key_watch_test"
```

- [ ] **Step 4: 跑测试确认通过**

Run: `g++ -std=c++17 -Wall -Itsf/PIMETextService tsf/tests/missed_key_watch_test.cpp -o /tmp/mkw && /tmp/mkw`
Expected: 输出 `missed_key_watch_test: 全部通过`,退出码 0。

- [ ] **Step 5: Commit**

```bash
git add tsf/PIMETextService/MissedKeyWatch.h tsf/tests/missed_key_watch_test.cpp scripts/build-tsf.sh
git commit -m "前端:漏键判定(纯逻辑 + 本机单测)"
```

---

### Task 4: TSF 接线——首键暂存补发

**Files:**
- Modify: `tsf/libIME2/src/TextService.h`(虚函数段约 203 行后;private 段约 279 行后)
- Modify: `tsf/libIME2/src/TextService.cpp`(`Deactivate` 约 609、`OnSetFocus(ITfDocumentMgr*…)` 652、`OnSetFocus(BOOL)` 708、四个按键入口 742–819、`OnChange` 约 835)
- Create: `tsf/libIME2/src/TextServiceStash.cpp`
- Modify: `tsf/PIMETextService/PIMEClient.h`、`PIMEClient.cpp:143-154`(`handleRpcResponse`)
- Modify: `tsf/PIMETextService/PIMETextService.h:79`、`PIMETextService.cpp:233`
- Modify: `scripts/build-tsf.sh`(libIME2 源文件列表)

**Interfaces:**
- Consumes: Task 1 的 `init` 回复字段 `firstKeyStash`;Task 2 的 `PIME::FirstKeyStash`、`PIME::buildReplay`、`PIME::KeyIn` 等。
- Produces:`Ime::TextService` 虚函数 `virtual bool isChineseMode() const`、`virtual bool firstKeyStashEnabled() const`、
  `virtual void onDiagnostic(const char* line)`(默认分别 false / false / 空);Task 5 复用 `isChineseMode()`、`onDiagnostic()`,
  并在 `stashKeyDown` 开头的 `keyWatch_.sinkSaw(...)` 调用(本任务先不加,Task 5 加)。

- [ ] **Step 1: `TextService.h` 加声明**

在 `virtual void onKeyPassedThrough(const char* detail) {}` 之后加:

```cpp
    // PYIME:首键暂存与漏键诊断所需的前端状态,由 PIME::TextService 覆写
    virtual bool isChineseMode() const { return false; }
    virtual bool firstKeyStashEnabled() const { return false; }
    virtual void onDiagnostic(const char* line) {}
```

文件顶部 `#include` 段加 `#include "FirstKeyStash.h"`。private 段 `void reportPassThrough(...)` 之后加:

```cpp
    // PYIME:首键暂存补发(TextServiceStash.cpp)
    bool stashKeyDown(ITfContext* context, WPARAM wParam, LPARAM lParam, BOOL* pfEaten);
    bool stashKeyUp(WPARAM wParam, BOOL* pfEaten);
    void watchStashContext(ITfContext* context);
    void flushStash();
    void discardStash(const char* why);
    void syncStash();
    bool stashCompartmentChanged(REFGUID rguid);
    static void CALLBACK onStashTimer(HWND hwnd, UINT msg, UINT_PTR id, DWORD time);
    PIME::FirstKeyStash stash_;
    UINT_PTR stashTimer_ = 0;
    ComPtr<ITfContext> stashContext_;
    SinkAdvice stashDisabledSink_;
    SinkAdvice stashEmptySink_;
    bool stashPassLogged_ = false;
```

- [ ] **Step 2: 新建 `tsf/libIME2/src/TextServiceStash.cpp`**

```cpp
// PYIME:首键暂存补发的 TSF 接线。状态机在 PIMETextService/FirstKeyStash.h,设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md。
#include "TextService.h"
#include <cstdio>
#include <vector>

namespace Ime {

namespace {

constexpr ULONG_PTR kStashMagic = 0x50594D45;  // "PYME":补发按键的 dwExtraInfo 标记
constexpr UINT kStashPollMs = 50;
// TIMERPROC 在建定时器的线程上执行,每线程至多一个 TextService
thread_local TextService* t_stashOwner = nullptr;

bool isInjected() { return (ULONG_PTR)::GetMessageExtraInfo() == kStashMagic; }
bool isShiftVk(WPARAM vk) { return vk == VK_SHIFT || vk == VK_LSHIFT || vk == VK_RSHIFT; }
bool held(int vk) { return ::GetKeyState(vk) < 0; }
bool isModifierVk(WPARAM vk) {
    return vk == VK_CONTROL || vk == VK_LCONTROL || vk == VK_RCONTROL || vk == VK_MENU || vk == VK_LMENU
        || vk == VK_RMENU || vk == VK_LWIN || vk == VK_RWIN;
}

PIME::KeyKind kindOf(WPARAM vk) {
    if (isModifierVk(vk))
        return PIME::KeyKind::Modifier;  // 暂存中吃下、排在字母后补发(先判:按下自身时 GetKeyState 可能已为按下)
    if (held(VK_CONTROL) || held(VK_MENU))
        return PIME::KeyKind::Other;  // 快捷键不吃
    if (vk >= 'A' && vk <= 'Z')
        return PIME::KeyKind::Letter;
    if ((vk >= '0' && vk <= '9') || (vk >= VK_NUMPAD0 && vk <= VK_DIVIDE) || vk == VK_SPACE || vk == VK_RETURN
        || vk == VK_BACK || (vk >= VK_OEM_1 && vk <= VK_OEM_3) || (vk >= VK_OEM_4 && vk <= VK_OEM_8) || vk == VK_OEM_102)
        return PIME::KeyKind::Text;
    return PIME::KeyKind::Other;
}

const char* reasonText(PIME::StashReason r) {
    switch (r) {
    case PIME::StashReason::Focus: return "焦点切换";
    case PIME::StashReason::Compartment: return "隔间变化";
    case PIME::StashReason::NextKey: return "下一个键";
    case PIME::StashReason::Timeout: return "超时";
    }
    return "?";
}

}  // namespace

// 返回 true 表示本次按键已处理完(*pfEaten 已设),调用方直接返回
bool TextService::stashKeyDown(ITfContext* context, WPARAM wParam, LPARAM lParam, BOOL* pfEaten) {
    const bool injected = isInjected();
    if (injected && isShiftVk(wParam)) {  // 补发时临时抬起/按回的 Shift:不经服务端,免得被当成单击 Shift 切英文
        *pfEaten = FALSE;
        return true;
    }
    const uint64_t now = ::GetTickCount64();
    if (stash_.poll(now))
        flushStash();
    if (stash_.takeAbandoned())
        onDiagnostic("首键暂存:补发的键 1 s 内未收齐,放弃在途");

    const bool blocked = isKeyboardDisabled(context) || !isKeyboardOpened();
    PIME::KeyIn k{};
    k.vk = (uint32_t)wParam;
    k.lParam = (uint32_t)lParam;
    k.kind = kindOf(wParam);
    k.shift = held(VK_SHIFT);
    k.injected = injected;
    k.stashable = !injected && k.kind == PIME::KeyKind::Letter && !k.shift && !PIME::isRepeatKey(k.lParam)
        && firstKeyStashEnabled() && isChineseMode() && !isComposing() && isKeyboardOpened()
        && contextCompartmentValue(GUID_COMPARTMENT_KEYBOARD_DISABLED, context) != 0
        && contextCompartmentValue(GUID_COMPARTMENT_EMPTYCONTEXT, context) != 0;

    const bool wasIdle = stash_.idle();
    const PIME::StashDecision d = stash_.keyDown(k, now);
    if (wasIdle && stash_.stashing())
        watchStashContext(context);
    if (d.flush)
        flushStash();
    syncStash();
    if (d.eat) {
        *pfEaten = TRUE;
        return true;
    }
    if (injected && blocked) {  // 补发后上下文仍禁用:直接放行,不再报"字母键未交给输入法"
        if (!stashPassLogged_) {
            stashPassLogged_ = true;
            onDiagnostic("首键暂存:补发后仍为禁用上下文,放行");
        }
        *pfEaten = FALSE;
        return true;
    }
    return false;
}

bool TextService::stashKeyUp(WPARAM wParam, BOOL* pfEaten) {
    if (isInjected() && isShiftVk(wParam)) {
        *pfEaten = FALSE;
        return true;
    }
    stash_.keyUp((uint32_t)wParam);
    return false;
}

// 暂存开始:监听按键上下文的两个隔间,任一变为 0 即补发(Chromium 可能只改隔间、不换文档管理器)
void TextService::watchStashContext(ITfContext* context) {
    stashDisabledSink_.unadvise();
    stashEmptySink_.unadvise();
    stashContext_ = context;
    if (!context)
        return;
    if (auto source = contextCompartment(GUID_COMPARTMENT_KEYBOARD_DISABLED, context).query<ITfSource>())
        stashDisabledSink_ = SinkAdvice(source, IID_ITfCompartmentEventSink, (ITfCompartmentEventSink*)this);
    if (auto source = contextCompartment(GUID_COMPARTMENT_EMPTYCONTEXT, context).query<ITfSource>())
        stashEmptySink_ = SinkAdvice(source, IID_ITfCompartmentEventSink, (ITfCompartmentEventSink*)this);
}

bool TextService::stashCompartmentChanged(REFGUID rguid) {
    if (!IsEqualGUID(rguid, GUID_COMPARTMENT_KEYBOARD_DISABLED) && !IsEqualGUID(rguid, GUID_COMPARTMENT_EMPTYCONTEXT))
        return false;
    if (stashContext_ && contextCompartmentValue(rguid, stashContext_) == 0
        && stash_.trigger(PIME::StashReason::Compartment))
        flushStash();
    syncStash();
    return true;  // 只有暂存才监听这两个隔间,不转给服务端
}

```

(续 `TextServiceStash.cpp`)

```cpp
void TextService::flushStash() {
    const uint64_t now = ::GetTickCount64();
    const PIME::StashReason reason = stash_.reason();
    const std::vector<PIME::StashKey> keys = stash_.takeFlush(now);
    if (keys.empty())
        return;
    // 前台已换到别的线程(Alt+Tab 等):丢弃,不把字母注入到别的程序
    HWND fg = ::GetForegroundWindow();
    if (!fg || ::GetWindowThreadProcessId(fg, nullptr) != ::GetCurrentThreadId()) {
        stash_.discard();
        onDiagnostic("首键暂存:前台已不是本线程,丢弃");
        return;
    }
    char line[128];
    std::snprintf(line, sizeof(line), "首键暂存:%s 等待 %llu ms 共 %u 键", reasonText(reason),
        (unsigned long long)stash_.waitedMs(), (unsigned)keys.size());
    onDiagnostic(line);

    std::vector<INPUT> inputs;
    for (const PIME::Stroke& s : PIME::buildReplay(keys, ::GetAsyncKeyState(VK_LSHIFT) < 0, ::GetAsyncKeyState(VK_RSHIFT) < 0)) {
        INPUT in{};
        in.type = INPUT_KEYBOARD;
        in.ki.wVk = (WORD)s.vk;
        in.ki.wScan = (WORD)s.scan;
        in.ki.dwFlags = (s.up ? KEYEVENTF_KEYUP : 0) | (s.extended ? KEYEVENTF_EXTENDEDKEY : 0);
        in.ki.dwExtraInfo = kStashMagic;
        inputs.push_back(in);
    }
    stashPassLogged_ = false;
    if (::SendInput((UINT)inputs.size(), inputs.data(), sizeof(INPUT)) != inputs.size()) {
        stash_.discard();
        onDiagnostic("首键暂存:SendInput 失败,丢弃");
    }
}

void TextService::discardStash(const char* why) {
    const size_t n = stash_.discard();
    if (n) {
        char line[96];
        std::snprintf(line, sizeof(line), "首键暂存:%s,丢弃 %u 键", why, (unsigned)n);
        onDiagnostic(line);
    }
    syncStash();
}

// 按状态收放隔间监听与轮询定时器
void TextService::syncStash() {
    if (!stash_.stashing()) {
        stashDisabledSink_.unadvise();
        stashEmptySink_.unadvise();
        stashContext_ = nullptr;
    }
    if (stash_.idle()) {
        if (stashTimer_) {
            ::KillTimer(nullptr, stashTimer_);
            stashTimer_ = 0;
        }
        if (t_stashOwner == this)
            t_stashOwner = nullptr;
    }
    else if (!stashTimer_) {
        t_stashOwner = this;
        stashTimer_ = ::SetTimer(nullptr, 0, kStashPollMs, onStashTimer);
    }
}

void CALLBACK TextService::onStashTimer(HWND, UINT, UINT_PTR, DWORD) {
    TextService* self = t_stashOwner;
    if (!self)
        return;
    if (self->stash_.poll(::GetTickCount64()))
        self->flushStash();
    if (self->stash_.takeAbandoned())
        self->onDiagnostic("首键暂存:补发的键 1 s 内未收齐,放弃在途");
    self->syncStash();
}

}  // namespace Ime
```

- [ ] **Step 3: `TextService.cpp` 接线**(每处改动为精确插入,其余不动)

`Deactivate()` 开头(`// terminate composition properly` 之前)插入:

```cpp
    discardStash("失活");  // PYIME
```

`OnSetFocus(ITfDocumentMgr*, ITfDocumentMgr*)` 函数体改为:

```cpp
    // PYIME:焦点切到新文档,暂存的首键立即补发
    if (pDocMgrFocus && stash_.trigger(PIME::StashReason::Focus))
        flushStash();
    syncStash();
    return S_OK;
```

`OnSetFocus(BOOL fForeground)` 的 `else { onKillFocus(); }` 改为:

```cpp
    else {
        discardStash("失去焦点");  // PYIME:Alt+Tab 走这里而非失活
        onKillFocus();
    }
```

`OnTestKeyDown`、`OnKeyDown` 函数体最前面各插入:

```cpp
    if (stashKeyDown(pContext, wParam, lParam, pfEaten))  // PYIME:首键暂存
        return S_OK;
```

`OnTestKeyUp`、`OnKeyUp` 函数体最前面各插入:

```cpp
    if (stashKeyUp(wParam, pfEaten))  // PYIME:补发时注入的 Shift
        return S_OK;
```

`OnChange(REFGUID rguid)` 中 `onCompartmentChanged(rguid);` 之前插入:

```cpp
    if (stashCompartmentChanged(rguid))  // PYIME:暂存期间监听的按键上下文隔间
        return S_OK;
```

- [ ] **Step 4: PIME 层**

`PIMEClient.h` 的 `public:` 段(`void onDeactivate();` 之后)加:

```cpp
	// PYIME:首键暂存所需状态
	bool chineseMode() const { return hasEnglish_ && !english_; }
	bool firstKeyStash() const { return firstKeyStash_; }
```

private 段 `bool english_ = false;` 之后加:

```cpp
    bool firstKeyStash_ = true; // PYIME:服务端 init 回复的首键暂存开关;旧服务端不带,视为开
```

`PIMEClient.cpp` `handleRpcResponse` 中 `if (msg["english"].isBool()) {...}` 块之后加:

```cpp
		if (msg["firstKeyStash"].isBool())
			firstKeyStash_ = msg["firstKeyStash"].asBool();
```

`PIMETextService.h` 中 `virtual void onKeyPassedThrough(const char* detail);` 之后加:

```cpp
	// PYIME:首键暂存与漏键诊断
	bool isChineseMode() const override;
	bool firstKeyStashEnabled() const override;
	void onDiagnostic(const char* line) override;
```

`PIMETextService.cpp` 中 `onKeyPassedThrough` 函数之后加:

```cpp
bool TextService::isChineseMode() const {
	return client_ && client_->chineseMode();
}

bool TextService::firstKeyStashEnabled() const {
	return client_ && client_->firstKeyStash();
}

void TextService::onDiagnostic(const char* line) {
	tsfLog("%s", line);
}
```

`scripts/build-tsf.sh` 的 libIME2 源文件循环 `for f in ImeModule libIME TextService ...` 中,`TextService` 之后加 `TextServiceStash`。

- [ ] **Step 5: 交叉编译 + 单测**

Run: `scripts/build-tsf.sh > /tmp/build-tsf.log 2>&1; echo 退出码 $?; tail -20 /tmp/build-tsf.log`
Expected: 退出码 0;日志含 `first_key_stash_test: 全部通过`、`missed_key_watch_test: 全部通过`;无 error。
再查依赖:`x86_64-w64-mingw32-objdump -p target/tsf/x64/PYIME.dll | grep 'DLL Name'` 不含 libgcc/libstdc/libwinpthread。
编译失败时先读错误再针对性修(常见:`VK_OEM_8`/`VK_OEM_102` 缺定义 → 查 mingw `winuser.h`;`thread_local` 链接 → 确认 `-static`)。

- [ ] **Step 6: Commit**

```bash
git add tsf/libIME2/src/TextService.h tsf/libIME2/src/TextService.cpp tsf/libIME2/src/TextServiceStash.cpp \
  tsf/PIMETextService/PIMEClient.h tsf/PIMETextService/PIMEClient.cpp \
  tsf/PIMETextService/PIMETextService.h tsf/PIMETextService/PIMETextService.cpp scripts/build-tsf.sh
git commit -m "前端:中文状态字母键落在禁用+空上下文时暂存,焦点切换/隔间变化/下一个键/超时后原序补发"
```

---

### Task 5: TSF 接线——漏键诊断

**Files:**
- Modify: `tsf/libIME2/src/TextService.h`(include 与 private 段)
- Modify: `tsf/libIME2/src/TextService.cpp`(`installEventListeners` 约 514、`uninstallEventListeners` 约 538、`endComposition` 203)
- Modify: `tsf/libIME2/src/TextServiceStash.cpp`(`stashKeyDown` 开头)
- Create: `tsf/libIME2/src/TextServiceKeyWatch.cpp`
- Modify: `scripts/build-tsf.sh`(源文件列表、链接 `-limm32`)

**Interfaces:**
- Consumes: Task 3 的 `PIME::MissedKeyWatch`;Task 4 的 `isChineseMode()`、`onDiagnostic()`、`stashKeyDown()`。

- [ ] **Step 1: `TextService.h`**

include 段加 `#include "MissedKeyWatch.h"`;private 段(Task 4 加的成员之后)加:

```cpp
    // PYIME:漏键诊断(TextServiceKeyWatch.cpp)
    void installKeyWatch();
    void uninstallKeyWatch();
    void keyWatchDown(WPARAM vk);
    void keyWatchUp(WPARAM vk);
    static LRESULT CALLBACK keyWatchProc(int code, WPARAM wParam, LPARAM lParam);
    PIME::MissedKeyWatch keyWatch_;
    HHOOK keyWatchHook_ = nullptr;
    uint64_t lastCommitMs_ = 0;  // 最近一次组字结束(上屏)的时刻
```

- [ ] **Step 2: 新建 `tsf/libIME2/src/TextServiceKeyWatch.cpp`**

```cpp
// PYIME:漏键诊断。本线程 WH_GETMESSAGE 钩子(不是全局钩子)看到字母键,而输入法按键入口始终没收到时,
// 在 tsf.log 记一条(不记是哪个键)。判定逻辑在 PIMETextService/MissedKeyWatch.h,设计见
// docs/superpowers/specs/2026-11-05-first-key-stash-design.md 需求 2。
#include "TextService.h"
#include <imm.h>
#include <cstdio>

namespace Ime {

namespace {
thread_local TextService* t_watchOwner = nullptr;  // 钩子回调在本线程执行
}

void TextService::installKeyWatch() {
    if (keyWatchHook_)
        return;
    t_watchOwner = this;
    keyWatchHook_ = ::SetWindowsHookExW(WH_GETMESSAGE, keyWatchProc, nullptr, ::GetCurrentThreadId());
}

void TextService::uninstallKeyWatch() {
    if (keyWatchHook_) {
        ::UnhookWindowsHookEx(keyWatchHook_);
        keyWatchHook_ = nullptr;
    }
    if (t_watchOwner == this)
        t_watchOwner = nullptr;
}

LRESULT CALLBACK TextService::keyWatchProc(int code, WPARAM wParam, LPARAM lParam) {
    if (code == HC_ACTION && wParam == PM_REMOVE && t_watchOwner) {
        const MSG* msg = reinterpret_cast<const MSG*>(lParam);
        if (msg->message == WM_KEYDOWN)
            t_watchOwner->keyWatchDown(msg->wParam);
        else if (msg->message == WM_KEYUP)
            t_watchOwner->keyWatchUp(msg->wParam);
    }
    return ::CallNextHookEx(nullptr, code, wParam, lParam);
}

void TextService::keyWatchDown(WPARAM vk) {
    const bool eligible = vk >= 'A' && vk <= 'Z' && isChineseMode()
        && ::GetKeyState(VK_CONTROL) >= 0 && ::GetKeyState(VK_MENU) >= 0 && ::GetKeyState(VK_SHIFT) >= 0;
    keyWatch_.hookDown((uint32_t)vk, eligible, ::GetTickCount64());
}

void TextService::keyWatchUp(WPARAM vk) {
    const uint64_t now = ::GetTickCount64();
    if (!keyWatch_.hookUp((uint32_t)vk, now))
        return;
    ComPtr<ITfDocumentMgr> docMgr;
    const bool hasDoc = threadMgr_ && threadMgr_->GetFocus(&docMgr) == S_OK && docMgr;
    bool hasImc = false;
    if (HWND focus = ::GetFocus()) {
        if (HIMC imc = ::ImmGetContext(focus)) {
            hasImc = true;
            ::ImmReleaseContext(focus, imc);
        }
    }
    char since[32] = "无";
    if (lastCommitMs_)
        std::snprintf(since, sizeof(since), "%llu ms", (unsigned long long)(now - lastCommitMs_));
    char line[160];
    std::snprintf(line, sizeof(line), "字母键未经输入法:焦点文档=%s 窗口输入法上下文=%s 距上次上屏 %s",
        hasDoc ? "有" : "无", hasImc ? "有" : "无", since);
    onDiagnostic(line);
}

}  // namespace Ime
```

- [ ] **Step 3: 接线**

`TextService.cpp` `installEventListeners()` 末尾(`keyboardOPenCloseSink_ = ...` 块之后)加:

```cpp
    installKeyWatch();  // PYIME:漏键诊断
```

`uninstallEventListeners()` 末尾加:

```cpp
    uninstallKeyWatch();  // PYIME
```

`endComposition(ITfContext* context)` 函数体第一行(`assert(context);` 之后)加:

```cpp
    lastCommitMs_ = ::GetTickCount64();  // PYIME:漏键诊断记"距上次上屏"
```

`TextServiceStash.cpp` `stashKeyDown` 函数体第一行加(输入法入口收到了这个键,注入键也算):

```cpp
    keyWatch_.sinkSaw((uint32_t)wParam, ::GetTickCount64());
```

`scripts/build-tsf.sh`:libIME2 源文件循环中 `TextServiceStash` 之后加 `TextServiceKeyWatch`;链接行 `-luser32` 之后加 `-limm32`。

- [ ] **Step 4: 交叉编译 + 单测**

Run: `scripts/build-tsf.sh > /tmp/build-tsf.log 2>&1; echo 退出码 $?; tail -20 /tmp/build-tsf.log`
Expected: 退出码 0,两条单测"全部通过";`x86_64-w64-mingw32-objdump -p target/tsf/x64/PYIME.dll | grep 'DLL Name'` 含 `IMM32.dll`、不含 libgcc/libstdc/libwinpthread。

- [ ] **Step 5: Commit**

```bash
git add tsf/libIME2/src/TextService.h tsf/libIME2/src/TextService.cpp tsf/libIME2/src/TextServiceStash.cpp \
  tsf/libIME2/src/TextServiceKeyWatch.cpp scripts/build-tsf.sh
git commit -m "前端:线程级消息钩子诊断字母键未经输入法(记焦点文档、窗口输入法上下文、距上次上屏)"
```

---

### Task 6: 实机清单与说明

**Files:**
- Modify: `test-kit/tsf.txt`(文件末尾追加第十一节)
- Modify: `CLAUDE.md`(Project Layout 的 `fuzzy.txt` 说明;待办段)

- [ ] **Step 1: `test-kit/tsf.txt` 末尾追加**

```text
十一、首键暂存补发与漏键诊断(0.1.19)
31. Chrome 网页输入框、Claude 桌面版、微信小程序里,中文状态连续上屏多次后新输入:首字母不再以英文漏出;tsf.log 有"首键暂存:"结局行(焦点切换/隔间变化/下一个键/超时 + 等待毫秒数)。
32. YouTube 等网页非输入区,中文状态按 k:约 0.5 秒内生效;tsf.log 有"首键暂存:超时"与"补发后仍为禁用上下文,放行"。
33. 暂存期间快速连打 dui:组字为 dui,顺序不乱。
34. 暂存期间按着 Shift(如紧接着打大写字母):不切到英文、不闪"英",字母正常组字。
35. 网页非输入区按一个字母后 0.5 秒内 Alt+Tab 到记事本:记事本不出现该字母;tsf.log 有"失去焦点,丢弃"。
36. %LOCALAPPDATA%\PYIME\fuzzy.txt 加一行"首键暂存"并重启输入法:行为回到 0.1.18;再在托盘改一次模糊音,该行仍在。
37. Mullion 里复现首键丢失:tsf.log 有"字母键未经输入法"一行,记下时间与 Mullion 日志对时贴回。
38. 记事本、WPS 正常打字:无变化,tsf.log 无"首键暂存"行。
```

- [ ] **Step 2: `CLAUDE.md`**

Project Layout 中 `fuzzy.txt(每行一个关闭的规则名,如 r=l;含一行 纠错 表示关闭按键纠错;不存在即全开)` 改为
`fuzzy.txt(每行一个关闭的规则名,如 r=l;含一行 纠错 表示关闭按键纠错;含一行 首键暂存 表示关闭首键暂存补发,改后重启输入法生效;不存在即全开)`。
待办段末尾加一条:

```markdown
- 首键暂存补发 + 漏键诊断(2026-11-05,分支 first-key-stash,设计 `docs/superpowers/specs/2026-11-05-first-key-stash-design.md`):Chromium 禁用+空上下文的首个字母暂存后原序补发,超时 500 ms;Mullion 路径(键未交给输入法)只记诊断,补偿待 Mullion 侧日志对时后定。待按 `test-kit/tsf.txt` 第十一节实机验证。
```

- [ ] **Step 3: 全量验证**

Run: `cargo test -q > /tmp/cargo-test.log 2>&1; echo $?; grep "test result" /tmp/cargo-test.log` 与 `scripts/build-tsf.sh > /tmp/build-tsf.log 2>&1; echo $?`
Expected: 两者退出码 0;cargo 各 crate `test result: ok`。

- [ ] **Step 4: Commit**

```bash
git add test-kit/tsf.txt CLAUDE.md
git commit -m "实机清单:第十一节 首键暂存补发与漏键诊断;说明 fuzzy.txt 首键暂存开关"
```

打包(`scripts/package.sh 0.1.19`)、改版本号、push、tag、发布均不在本计划内,完成后先问用户。
