# 模糊惩罚调参 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 多字词模糊惩罚从 3.0 降到 2.5,单字保持 3.0,使首次输入的多字模糊词排名提前而不挤掉高频精确单字。

**Architecture:** 只改 `crates/pyime-engine/src/engine.rs`:`FUZZY_PENALTY` 改 2.5,新增 `SINGLE_CHAR_FUZZY_EXTRA = 0.5`,
`grow` / `grow_partial` 的打分改用新方法 `penalty(single)`。测试先行:合成词典单测为主护栏,真实词典回归用例为补充。

**Tech Stack:** Rust(cargo test / clippy),无新依赖。

## Global Constraints

- 设计依据:`docs/superpowers/specs/2026-10-18-fuzzy-penalty-tuning-design.md`。
- 注释、测试名、commit message 一律简体中文;测试函数沿用中文命名。
- 不加新依赖;不改 `user.txt` 格式;`FUZZY_DISCOUNT_CAP` 保持 2.0。
- `FUZZY_PENALTY = 2.5`;`SINGLE_CHAR_FUZZY_EXTRA = 0.5`;单字 = `grow` 中 `depth == 0`、`grow_partial` 中 `key.is_empty()`。
- `set_fuzzy_penalty` 设多字词惩罚,单字随之平移。
- 改已有文件用精确改动,不顺手重构(例如 `engine.rs:10` 错位的 WORD_COST 注释不动)。
- 不删、不放宽既有断言;既有单测因分界线移动失败时只调造数据词频。
- 每个 commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 分支 `feat/fuzzy-penalty-tuning`;不 push、不打 tag。

---

### Task 1: 多字词降惩罚、单字保持(含测试与注释)

**Files:**
- Modify: `crates/pyime-engine/src/engine.rs`(常数 8–9 行附近;`set_fuzzy_penalty` 后加方法;`grow_partial` 约 213 行;`grow` 约 233 行;测试模块 415、512、641 行与文件末尾)
- Modify: `crates/pyime-engine/src/user.rs:5`、`:9`(注释)
- Modify: `crates/pyime-engine/tests/regression.rs`(`CASES` 末尾)

**Interfaces:**
- Produces:`const SINGLE_CHAR_FUZZY_EXTRA: f64`(私有);`fn penalty(&self, single: bool) -> f64`(私有);
  `pub const FUZZY_PENALTY: f64 = 2.5`(值变,名不变)。

- [ ] **Step 1: 写失败的单测**

在 `engine.rs` 测试模块末尾(最后一个 `}` 之前)加:

```rust
    #[test]
    fn 多字词模糊惩罚低于单字() {
        // 模糊词词频为精确词 16 倍(ln16 ≈ 2.77):多字词惩罚 2.5,模糊词胜出;单字惩罚 3.0,精确字仍居首
        let e = engine(&[("zi lei", "子类", 1000), ("zhi lei", "之类", 16_000)]);
        assert_eq!(texts(&e, "zilei")[0], "之类");
        let e = engine(&[("si", "四", 1000), ("shi", "是", 16_000)]);
        assert_eq!(texts(&e, "si")[0], "四");
    }
```

- [ ] **Step 2: 确认失败**

Run: `cargo test -q -p pyime-engine --lib 多字词模糊惩罚低于单字`
Expected: FAIL,`left: "子类"`,`right: "之类"`。

- [ ] **Step 3: 加真实词典回归用例**

`tests/regression.rs` 的 `CASES` 中,在 `("chelue", "策略", 3),` 一行之后加:

```rust
    ("jiashan", "加上", 1),     // 多字模糊 an=ang:惩罚 2.5 时首选
    ("yinp", "硬盘", 3),        // 缩写 + in=ing
    ("ne", "呢", 1),            // 护住单字:高频 了(n=l)不得压过
    ("si", "死", 1),            // 护住单字:高频 是(s=sh)不得压过
    ("lan", "蓝", 1),           // 护住单字:高频 然(r=l)不得压过
```

Run: `PYIME_DICT=$PWD/installer/pyime.dict PYIME_GRAM=/tmp/gram/pyime.gram cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture 2>&1 | grep -E '通过|→'`
Expected(现状):`通过 34/39`;失败列表为既有 3 条(zaizuijia、fengzhong、jintianxingqijia)
加 `jiashan → 加上: 名次 Some(2),要求 ≤1`、`yinp → 硬盘: 名次 Some(4),要求 ≤3`;ne/si/lan 不在失败列表。

- [ ] **Step 4: 实现**

`engine.rs` 常数处,把

```rust
/// 每次模糊替换的惩罚(自然对数域,约等于词频缩小 e^3 ≈ 20 倍)。
pub const FUZZY_PENALTY: f64 = 3.0;
```

改为

```rust
/// 多字词每次模糊替换的惩罚(自然对数域,约等于词频缩小 e^2.5 ≈ 12 倍)。
pub const FUZZY_PENALTY: f64 = 2.5;
/// 单字词模糊惩罚的额外部分:单字里"了""是"等虚词词频极高,需更高惩罚(默认合计 3.0)。
const SINGLE_CHAR_FUZZY_EXTRA: f64 = 0.5;
```

在 `pub fn set_fuzzy_penalty` 整个函数之后加:

```rust

    /// 一次模糊替换的惩罚;单字词另加 `SINGLE_CHAR_FUZZY_EXTRA`。
    fn penalty(&self, single: bool) -> f64 {
        self.fuzzy_penalty + if single { SINGLE_CHAR_FUZZY_EXTRA } else { 0.0 }
    }
```

`grow_partial` 的 score 行中 `- self.fuzzy_penalty * f64::from(a.fuzzy)` 改为 `- self.penalty(key.is_empty()) * f64::from(a.fuzzy)`;
`grow` 的 score 行中同一片段改为 `- self.penalty(depth == 0) * f64::from(a.fuzzy)`。行内其余部分不动。

- [ ] **Step 5: 修正过时注释与造数据**

`engine.rs` 测试模块(行号为 Step 4 之前的,请按内容定位):
- 415 行 `// 惩罚约 20 倍:高频模糊词压过极低频的精确词,但不会压过同量级的精确词`
  改为 `// 单字惩罚 3.0 约 20 倍:高频模糊词压过极低频的精确词,但不会压过同量级的精确词`。
- 512 行 `// 之 的词频是 字 的 e^3 倍,惩罚 4 时输给精确的 字;接受两次 z=zh 后折扣约 1.1,反超`
  改为 `// 之 的词频是 字 的 20 倍(≈ e^3),单字惩罚 3.0 时输给精确的 字;接受两次 z=zh 后折扣约 1.1,反超`。
- 641 行(测试 `学过模糊词不使低频整段模糊词反超精确整句`)把 `("qing kuang xia", "情况下", 50)` 改为
  `("qing kuang xia", "情况下", 20)`。原因:情况下 两处模糊,惩罚由 6.0 降为 5.0,词频 50 时学习前就已首选,
  测试失去前提;20 仍能拦住 18fc708 的 `scored.first()` 回归(已在临时工作区验证:20 与 10 在旧写法下失败、新写法下通过)。

`user.rs`:
- 5 行 `/// 精确层基础加分(自然对数域):大于模糊惩罚 4,选过一次即可翻盘。`
  改为 `/// 精确层基础加分(自然对数域):大于模糊惩罚(多字 2.5、单字 3.0),选过一次即可翻盘。`
- 9 行 `/// 模糊规则个人折扣的上限(自然对数域);默认惩罚 3.0,用得再多也至少剩 1.0。`
  改为 `/// 模糊规则个人折扣的上限(自然对数域);默认惩罚多字 2.5、单字 3.0,用得再多也至少剩 0.5、1.0。`

- [ ] **Step 6: 跑测试**

Run: `cargo test -q 2>&1 | grep -E 'test result|FAILED|panicked'`
Expected:全部 `ok`(pyime-engine lib 57 passed,含新测试)。若有既有单测失败,先分析分界线,只调造数据词频,不改断言,并在报告中逐条说明。

Run: `PYIME_DICT=$PWD/installer/pyime.dict PYIME_GRAM=/tmp/gram/pyime.gram cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture 2>&1 | grep -E '通过|→'`
Expected:`通过 36/39`,失败列表只剩既有 3 条(zaizuijia、fengzhong、jintianxingqijia)。

Run: `cargo clippy --all-targets 2>&1 | grep -cE '^warning: [^`]'`
Expected:8(与 main 现状相同,不新增警告)。

- [ ] **Step 7: Commit**

```bash
git add crates/pyime-engine/src/engine.rs crates/pyime-engine/src/user.rs crates/pyime-engine/tests/regression.rs
git commit -m "引擎:多字词模糊惩罚降到 2.5,单字保持 3.0,首次输入的多字模糊词排名提前

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 回放验收(不改代码)

**Files:** 无改动;产出写入报告。

- [ ] **Step 1: 构建并回放 5 次**

```bash
cargo build --release -q -p pyime-backend --bin pyime-replay
for i in 1 2 3 4 5; do target/release/pyime-replay installer/pyime.dict /tmp/input-log.jsonl --gram /tmp/gram/pyime.gram | tail -1; done
```

Expected:5 次中最差值 首选 ≥ 519、前 3 ≥ 589、前 5 ≥ 607、未出现 8。

- [ ] **Step 2: 与旧基线逐条对比**

```bash
target/release/pyime-replay installer/pyime.dict /tmp/input-log.jsonl --gram /tmp/gram/pyime.gram -v > /tmp/replay-tuned.log
diff /tmp/replay-final.log /tmp/replay-tuned.log
```

`/tmp/replay-final.log` 为 84ee74e 的 `-v` 输出(仅含名次 ≥ 4)。Expected:与设计文档"实验"一节一致——
bengji→本级 5→4 等;新增或变差的条目逐条列出并说明。不达标则停止并上报,不自行改参数。
