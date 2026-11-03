# 按键纠错 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 输入没有精确整段词、且"只改一处"后能得到明显更像样的整段词时,把该词放到候选第 2 位;学习记在纠正后的拼音下;托盘可关。

**Architecture:** 新增纯字符串模块 `typo.rs` 生成单处编辑变体;`Engine::candidates_after` 在 `assemble` 之后、英文放置之前调用新的 `Engine::typo`,对变体复用 `grow` 收整段词,按"高于最佳整段词与最佳整句"的门槛筛选后插到下标 1。`Candidate` 新增 `fixed`,会话选中时据此学习。开关放在 `FuzzyConfig.typo`,存 `fuzzy.txt` 的 `纠错` 行,托盘顶层菜单项切换。

**Tech Stack:** Rust(`pyime-engine`、`pyime-backend`),cargo test。

**Spec:** `docs/superpowers/specs/2026-11-03-typo-correction-design.md`

## Global Constraints

- 回复、commit、注释一律简体中文;commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 只 `git add` 本任务涉及的文件;工作区里已有的 `CLAUDE.md`、`Cargo.lock`、`crates/*/Cargo.toml` 改动(版本号,用户未提交)不得加入提交。
- 不 push、不打 tag。
- 常量:`TYPO_MAX = 2`、`TYPO_MIN_LEN = 4`、`TYPO_MAX_LEN = 12`、`TYPO_PENALTY` 初值 `3.0`(Task 6 调定)。
- `fuzzy.txt` 关闭纠错的行名:`纠错`;菜单文字:`按键纠错`;命令 id:`CMD_TYPO = 15`。
- 不得为通过测试削弱既有断言;Task 5 中两处既有菜单测试的更新是新增菜单项的直接后果(列表加一项、下标后移一位),只做这两处。
- 纠错不进整句 Viterbi;不纠单字;不学错拼;用户短语不参与纠错;原有候选为空时不插。

## 文件结构

- Create `crates/pyime-engine/src/typo.rs`:`pub fn edits(input: &str) -> Vec<String>`,只管字符串编辑。
- Modify `crates/pyime-engine/src/lib.rs`:`pub mod typo;`
- Modify `crates/pyime-engine/src/fuzzy.rs`:`FuzzyConfig.typo` 与 `纠错` 行读写。
- Modify `crates/pyime-engine/src/engine.rs`:`Source::Typo`、`Candidate.fixed`、`assemble` 多返回最佳整句分、`Engine::typo`、`candidates_after` 插入。
- Modify `crates/pyime-engine/tests/regression.rs`:`bucket` 加 `Typo`;Task 6 加用例。
- Modify `crates/pyime-backend/src/inputlog.rs`:`select` 多一个 `fixed` 参数。
- Modify `crates/pyime-backend/src/session.rs`:按 `fixed` 学习、含纠错段跳过整句学习、`CMD_TYPO` 分发。
- Modify `crates/pyime-backend/src/bin/pyime-replay.rs`:回放按 `fixed` 学习。
- Modify `crates/pyime-backend/src/tray.rs`、`crates/pyime-backend/src/ime.rs`:菜单项与 `toggle_typo`。
- Modify `crates/pyime-backend/src/session/tests.rs`:会话与菜单测试。
- Modify `test-kit/tsf.txt`:实机验证项。

---

### Task 1: 基线采集 + `typo.rs` 变体生成

**Files:**
- Create: `crates/pyime-engine/src/typo.rs`
- Modify: `crates/pyime-engine/src/lib.rs`

**Interfaces:**
- Produces: `pyime_engine::typo::edits(input: &str) -> Vec<String>`:输入全为 ASCII 小写时返回只改一处的变体(去重、不含原串、不含空串);否则返回空。

- [ ] **Step 1: 采集回归与耗时基线(任何代码改动之前)**

```bash
cd /data/bak20250527/PYIME
PYIME_DICT=$PWD/installer/pyime.dict PYIME_EN=$PWD/installer/pyime.en PYIME_RANKS_OUT=/tmp/typo_base.tsv \
  cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture > /tmp/typo_base_reg.log 2>&1; tail -15 /tmp/typo_base_reg.log
PYIME_DICT=$PWD/installer/pyime.dict PYIME_INPUTS=tuojian,kailv,xiachenxu,zhegewentihao,woxiangquguan \
  cargo test -q -p pyime-engine --release --test real_engine -- --ignored --nocapture > /tmp/typo_base_perf.log 2>&1; grep -v '^$' /tmp/typo_base_perf.log | head -12
```

Expected:`/tmp/typo_base.tsv` 存在;记下回归"通过 x/y"行(若基线已有失败用例,照记,后续以"与基线比对"为准)与各输入耗时。

- [ ] **Step 2: 写失败的测试**

新建 `crates/pyime-engine/src/typo.rs`,先只放测试与空函数:

```rust
//! 按键纠错:把输入只改一处(相邻键替换、删一个字母、相邻互换、补一个常漏的字母),供整段词兜底查询。

pub fn edits(_input: &str) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(input: &str, want: &str) -> bool {
        edits(input).iter().any(|e| e == want)
    }

    #[test]
    fn 四类改法各能得到本意() {
        assert!(has("tuojian", "tuijian")); // 替换:o→i 相邻
        assert!(has("thuijian", "tuijian")); // 删除:多打的 h
        assert!(has("nie", "nei")); // 互换
        assert!(has("xiachenxu", "xiaochenxu")); // 插入:漏打的 o
        assert!(has("wom", "won")); // 替换:m→n 相邻
    }

    #[test]
    fn 不含原串_无重复_只换相邻键() {
        let v = edits("tuojian");
        assert!(!v.iter().any(|e| e == "tuojian"));
        let mut u = v.clone();
        u.sort();
        u.dedup();
        assert_eq!(u.len(), v.len());
        assert!(!has("ab", "ac")); // b 的邻键是 g h v n,不含 c
    }

    #[test]
    fn 非小写字母输入返回空() {
        assert!(edits("ni'hao").is_empty());
        assert!(edits("").is_empty());
    }
}
```

`crates/pyime-engine/src/lib.rs` 在 `pub mod symbol;` 后加一行:

```rust
pub mod typo;
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -q -p pyime-engine typo::`
Expected: `四类改法各能得到本意` 失败(空函数返回空)。

- [ ] **Step 4: 实现**

把 `typo.rs` 中的空函数替换为:

```rust
/// QWERTY 相邻键:同排左右、上下排斜邻。
fn neighbors(c: u8) -> &'static [u8] {
    match c {
        b'q' => b"wa",
        b'w' => b"qeas",
        b'e' => b"wrsd",
        b'r' => b"etdf",
        b't' => b"ryfg",
        b'y' => b"tugh",
        b'u' => b"yihj",
        b'i' => b"uojk",
        b'o' => b"ipkl",
        b'p' => b"ol",
        b'a' => b"qwsz",
        b's' => b"weadzx",
        b'd' => b"erfsxc",
        b'f' => b"rtdgcv",
        b'g' => b"tyfhvb",
        b'h' => b"yugjbn",
        b'j' => b"uihknm",
        b'k' => b"iojlm",
        b'l' => b"opk",
        b'z' => b"asx",
        b'x' => b"sdzc",
        b'c' => b"dfxv",
        b'v' => b"fgcb",
        b'b' => b"ghvn",
        b'n' => b"hjbm",
        b'm' => b"jkn",
        _ => b"",
    }
}

/// 漏打时可补的字母(韵母元音与 zh/ch/sh、ang/eng 里的 h、g)。
const INSERTS: &[u8] = b"aeiouhg";

/// 只改一处的变体:相邻键替换、删一个字母、相邻两字母互换、任意位置补 `INSERTS` 之一。
/// 去重,不含原串;输入为空或含小写字母以外的字符时返回空。
pub fn edits(input: &str) -> Vec<String> {
    let s = input.as_bytes();
    if s.is_empty() || !s.iter().all(u8::is_ascii_lowercase) {
        return Vec::new();
    }
    let mut out: Vec<Vec<u8>> = Vec::new();
    for i in 0..s.len() {
        for &n in neighbors(s[i]) {
            let mut v = s.to_vec();
            v[i] = n;
            out.push(v);
        }
        let mut v = s.to_vec();
        v.remove(i);
        out.push(v);
        if i + 1 < s.len() && s[i] != s[i + 1] {
            let mut v = s.to_vec();
            v.swap(i, i + 1);
            out.push(v);
        }
    }
    for i in 0..=s.len() {
        for &c in INSERTS {
            let mut v = s.to_vec();
            v.insert(i, c);
            out.push(v);
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.into_iter()
        .filter(|v| !v.is_empty() && v.as_slice() != s)
        .filter_map(|v| String::from_utf8(v).ok())
        .filter(|v| seen.insert(v.clone()))
        .collect()
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -q -p pyime-engine typo::`
Expected: 3 passed。

- [ ] **Step 6: 提交**

```bash
git add crates/pyime-engine/src/typo.rs crates/pyime-engine/src/lib.rs
git commit -m "引擎:按键纠错的单处编辑变体生成

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `FuzzyConfig.typo` 开关与 `纠错` 行读写

**Files:**
- Modify: `crates/pyime-engine/src/fuzzy.rs`(`FuzzyConfig` 定义、`Default`、`none`、`parse_off`、`export_off`、tests)

**Interfaces:**
- Produces: `FuzzyConfig { initials: [bool; 5], finals: [bool; 5], pub typo: bool }`;`Default` 时 `typo == true`,`none()` 时 `typo == false`;`parse_off` 遇行 `纠错` 置 `typo = false`;`export_off` 在 `typo == false` 时于十对规则之后追加 `纠错\n`。

- [ ] **Step 1: 写失败的测试**

在 `fuzzy.rs` 的 `mod tests` 末尾加:

```rust
    #[test]
    fn 纠错开关按名字读写() {
        let mut cfg = FuzzyConfig::default();
        assert!(cfg.typo && !FuzzyConfig::none().typo);
        cfg.typo = false;
        cfg.set(4, false);
        assert_eq!(cfg.export_off(), "r=l\n纠错\n");
        let back = FuzzyConfig::parse_off("\u{feff}纠错\r\nr=l\r\n");
        assert!(!back.typo && !back.is_on(4) && back.is_on(3));
        assert!(FuzzyConfig::parse_off("r=l\n").typo);
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-engine fuzzy::`
Expected: 编译失败,`no field typo`。

- [ ] **Step 3: 实现**

`FuzzyConfig` 结构体加字段(放在 `finals` 之后):

```rust
    /// 按键纠错:输入没有精确整段词时查"只改一处"的整段词(见 `typo.rs`),默认开。
    pub typo: bool,
```

`Default::default` 改为 `FuzzyConfig { initials: [true; 5], finals: [true; 5], typo: true }`;`none()` 改为 `FuzzyConfig { initials: [false; 5], finals: [false; 5], typo: false }`。

在 `impl FuzzyConfig` 之前加常量:

```rust
/// `fuzzy.txt` 中表示关闭按键纠错的行。
pub const TYPO_OFF: &str = "纠错";
```

`parse_off` 循环体改为:

```rust
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line == TYPO_OFF {
                cfg.typo = false;
            } else if let Some(i) = names.iter().position(|n| n == line) {
                cfg.set(i, false);
            }
        }
```

`export_off` 改为:

```rust
    /// 写配置文本:关闭的规则名,每行一个;关闭纠错时末尾加 `纠错` 行;全开时为空。
    pub fn export_off(&self) -> String {
        let mut s: String = Self::rule_names().into_iter().enumerate().filter(|&(i, _)| !self.is_on(i)).map(|(_, n)| n + "\n").collect();
        if !self.typo {
            s.push_str(TYPO_OFF);
            s.push('\n');
        }
        s
    }
```

同时把 `parse_off` 的文档注释改为"读配置文本:每行一个关闭的规则名(或 `纠错`),其余开;不认识的行忽略。"

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -q -p pyime-engine` 
Expected: 全部通过(含原有 `规则开关按名字读写_只记关闭的`)。

- [ ] **Step 5: 提交**

```bash
git add crates/pyime-engine/src/fuzzy.rs
git commit -m "模糊音配置:新增按键纠错开关,fuzzy.txt 以 纠错 行表示关闭

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 引擎接入纠错候选

**Files:**
- Modify: `crates/pyime-engine/src/engine.rs`(常量区、`Candidate`、`Source`、`candidates_after`、`assemble`、四处 `Candidate` 字面量、新方法 `typo`、tests)
- Modify: `crates/pyime-engine/tests/regression.rs`(`bucket`)

**Interfaces:**
- Consumes: `crate::typo::edits`(Task 1);`FuzzyConfig.typo`(Task 2)。
- Produces: `Source::Typo`;`Candidate.fixed: Option<String>`(纠错候选为 `Some(纠正后的拼音)`,其余 `None`);纠错候选 `consumed == input.len()`、`cuts`/`parts` 为空。

- [ ] **Step 1: 写失败的测试**

在 `engine.rs` 的 `mod tests` 末尾加:

```rust
    fn typo_engine(extra: &[(&str, &str, u32)]) -> Engine {
        let mut v = vec![("tuo", "拖", 8000), ("jian", "见", 8000), ("tui jian", "推荐", 50000)];
        v.extend_from_slice(extra);
        engine(&v)
    }

    fn has_typo(e: &Engine, input: &str) -> bool {
        e.candidates(input).iter().any(|c| c.source == Source::Typo)
    }

    #[test]
    fn 无精确整段词时纠错词排第二_带纠正后拼音() {
        let c = typo_engine(&[]).candidates("tuojian");
        assert_eq!(c[0].text, "拖见");
        assert_eq!((c[1].text.as_str(), c[1].source, c[1].fixed.as_deref()), ("推荐", Source::Typo, Some("tuijian")));
        assert_eq!(c[1].consumed, 7);
        assert!(c[1].cuts.is_empty() && c[1].parts.is_empty());
        assert!(c.iter().filter(|c| c.source != Source::Typo).all(|c| c.fixed.is_none()));
    }

    #[test]
    fn 模糊整段词不挡纠错() {
        let e = typo_engine(&[("tuo jiang", "脱缰", 2810)]);
        assert_eq!(texts(&e, "tuojian")[..2], ["脱缰", "推荐"]);
    }

    #[test]
    fn 有精确整段词时不纠() {
        assert!(!has_typo(&typo_engine(&[("tuo jian", "拖建", 10)]), "tuojian"));
    }

    #[test]
    fn 末尾残缺音节时不纠() {
        // 变体 tuji 能命中 突击,但 tuij 末尾残缺,不应闪出
        assert!(!has_typo(&typo_engine(&[("tu ji", "突击", 50000)]), "tuij"));
    }

    #[test]
    fn 开关关闭时不纠() {
        let mut e = typo_engine(&[]);
        let mut cfg = FuzzyConfig::default();
        cfg.typo = false;
        e.set_fuzzy(cfg);
        assert!(!has_typo(&e, "tuojian"));
    }

    #[test]
    fn 现有整句得分更高时不纠() {
        let e = engine(&[("tuo", "拖", 1_000_000), ("jian", "见", 1_000_000), ("tui jian", "推荐", 1)]);
        assert!(!has_typo(&e, "tuojian"));
    }

    #[test]
    fn 原有候选为空时不插() {
        assert!(typo_engine(&[]).candidates("thuijian").is_empty());
    }

    #[test]
    fn 不纠单字() {
        // zhusng 的变体 zhuang 命中单字 庄,不应出
        let e = engine(&[("zhu", "猪", 8000), ("zhuang", "庄", 900_000)]);
        assert!(!texts(&e, "zhusng").contains(&"庄".to_string()));
    }

    #[test]
    fn 超长输入不纠() {
        let e = typo_engine(&[("ke", "科", 8000), ("ji", "技", 8000), ("ba", "吧", 8000), ("tui jian ke ji ba", "推荐科技吧", 50000)]);
        assert!(!has_typo(&e, "tuojiankejiba")); // 13 个字母 > TYPO_MAX_LEN
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-engine engine::`
Expected: 编译失败(`Source::Typo`、`fixed` 不存在)。

- [ ] **Step 3: 加类型与常量**

常量区(`SYMBOLS_PER_WORD` 之后)加:

```rust
/// 按键纠错:纠错词在整段词口径得分上的额外惩罚;须高于现有最佳整段词与最佳整句才出。
const TYPO_PENALTY: f64 = 3.0;
/// 纠错候选至多几个。
const TYPO_MAX: usize = 2;
/// 触发纠错的输入长度范围(字节)。
const TYPO_MIN_LEN: usize = 4;
const TYPO_MAX_LEN: usize = 12;
```

`use` 区加 `use crate::typo::edits;`。

`Candidate` 在 `source` 之后加字段:

```rust
    /// 按键纠错候选纠正后的拼音(选中时按它学习);其余来源为 `None`。
    pub fixed: Option<String>,
```

`Source` 枚举末尾加 `Typo,`,并把其文档注释末尾补上"、纠错(输入改一处后的整段词)"。

四处 `Candidate { ... }` 字面量(`assemble` 的 `as_cand`、`place_symbols`、`insert_english`、`sentences`)末尾各加 `fixed: None`。

`regression.rs` 的 `bucket` 中 `match c.source` 加一支:`Source::Typo => "纠错",`。

- [ ] **Step 4: `assemble` 多返回最佳整句分**

`assemble` 签名改为 `fn assemble(&self, input: &str, words: &[Word]) -> (Vec<Candidate>, Option<f64>)`;在 `let scored = sentences(...)` 之后加:

```rust
        // 供按键纠错比分:所有整句里的最高分
        let best_sentence = scored.iter().map(|s| s.1).reduce(f64::max);
```

函数开头的 `let Some(far) = ... else { return Vec::new() };` 改为 `else { return (Vec::new(), None) };`,末尾 `out` 改为 `(out, best_sentence)`。

- [ ] **Step 5: 新增 `Engine::typo` 并在 `candidates_after` 中插入**

在 `impl Engine` 中 `assemble` 之后加:

```rust
    /// 按键纠错:输入没有精确整段词、末尾不是残缺音节、长度在范围内时,对"只改一处"的变体收吃满整段的完整词;
    /// 得分(整段词口径再减 `TYPO_PENALTY`)须高于现有最佳整段词与最佳整句,至多 `TYPO_MAX` 个,跳过已在 `existing` 中的。
    fn typo(&self, input: &str, words: &[Word], best_sentence: Option<f64>, existing: &[Candidate]) -> Vec<Candidate> {
        let len = input.len();
        if !self.cfg.typo || !(TYPO_MIN_LEN..=TYPO_MAX_LEN).contains(&len) || cover(words, len) == Cover::Partial {
            return Vec::new();
        }
        let whole: Vec<&Word> = words.iter().filter(|w| w.start == 0 && w.end == len && !w.partial).collect();
        if whole.iter().any(|w| w.fuzzy == 0) {
            return Vec::new();
        }
        let bar = whole.iter().map(|w| w.score - WORD_COST).chain(best_sentence).fold(f64::MIN, f64::max);
        let mut found: Vec<(f64, Word, String)> = Vec::new();
        for fixed in edits(input) {
            let mut ws = Vec::new();
            self.grow(&fixed, 0, 0, "", Acc::default(), 0, &mut Vec::new(), &mut ws);
            for w in ws.into_iter().filter(|w| w.end == fixed.len() && !w.partial && w.text.chars().count() >= 2) {
                let score = w.score - WORD_COST - TYPO_PENALTY;
                if score > bar {
                    found.push((score, w, fixed.clone()));
                }
            }
        }
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut out: Vec<Candidate> = Vec::new();
        for (_, w, fixed) in found {
            if out.len() == TYPO_MAX {
                break;
            }
            if out.iter().chain(existing).any(|c| c.text == w.text) {
                continue;
            }
            out.push(Candidate { text: w.text, consumed: len, fuzzy: w.fuzzy, pairs: w.pairs, cuts: Vec::new(), parts: Vec::new(), source: Source::Typo, fixed: Some(fixed) });
        }
        out
    }
```

`candidates_after` 中 `let mut out = self.assemble(input, &words);` 改为:

```rust
        let (mut out, best_sentence) = self.assemble(input, &words);
        // 按键纠错候选插在第 2 位起;原本没有候选时不插,保持空格原样上屏字母
        if !out.is_empty() {
            let fixes = self.typo(input, &words, best_sentence, &out);
            for (i, c) in fixes.into_iter().enumerate() {
                out.insert(1 + i, c);
            }
        }
```

(位于 `place_english` 之前,英文放置规则不变。)

- [ ] **Step 6: 运行确认通过**

Run: `cargo test -q -p pyime-engine`
Expected: 全部通过(新增 9 个 + 原有)。若 `无精确整段词时纠错词排第二` 中 `c[0]` 不是"拖见",先打印 `texts(&e, "tuojian")` 分析,不要改断言去迁就。

- [ ] **Step 7: clippy 与回归编译**

Run: `cargo clippy -q -p pyime-engine --all-targets 2>&1 | head -30`
Expected: 无新增告警(`typo` 参数较多若触发 `too_many_arguments` 以外的告警需修;`grow` 已有同类签名)。

- [ ] **Step 8: 提交**

```bash
git add crates/pyime-engine/src/engine.rs crates/pyime-engine/tests/regression.rs
git commit -m "引擎:无精确整段词时给出按键纠错候选(第 2 位,比分门槛)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 会话按纠正后拼音学习、分段不学整句、日志带 `fixed`

**Files:**
- Modify: `crates/pyime-backend/src/inputlog.rs:43-47`(`select`)
- Modify: `crates/pyime-backend/src/session.rs`(`Session` 字段、`Session::new` 字面量第 128 行、`select` 第 440–470 行、`flush_raw` 第 498–510 行)
- Modify: `crates/pyime-backend/src/bin/pyime-replay.rs:81-114`
- Test: `crates/pyime-backend/src/session/tests.rs`

**Interfaces:**
- Consumes: `Candidate.fixed`、`Source::Typo`(Task 3)。
- Produces: `inputlog::select(input, context, typed, text, index, top, fixed: Option<&str>)`;纠错时日志行多 `"fixed": "<纠正后拼音>"`。

- [ ] **Step 1: 写失败的测试**

在 `session/tests.rs` 末尾加:

```rust
fn typo_session() -> Session {
    let mut b = DictBuilder::default();
    b.add("tuo", "拖", 8000);
    b.add("jian", "见", 8000);
    b.add("tui jian", "推荐", 50000);
    b.add("wo men", "我们", 50000);
    Session::new(Ime::with_engine(Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default())))
}

/// tuojian 的键码。
const TUOJIAN: [i64; 7] = [84, 85, O, 74, I, A, N];

#[test]
fn 纠错候选按正确拼音学习_日志记实际键入与纠正() {
    let mut s = typo_session();
    crate::inputlog::take_captured();
    let r = press(&mut s, &TUOJIAN);
    assert_eq!(r["candidateList"][1], "推荐");
    key(&mut s, "onKeyDown", 50); // 数字 2
    let u = s.ime.user_export();
    assert!(u.contains("tuijian\t推荐\t1") && !u.contains("tuojian"), "{u}");
    let sel = crate::inputlog::take_captured().into_iter().find(|l| l["text"] == "推荐").unwrap();
    assert_eq!((sel["typed"].as_str(), sel["fixed"].as_str()), (Some("tuojian"), Some("tuijian")));
    // 退格删光即撤回
    backs(&mut s, 2);
    assert!(!s.ime.user_export().contains("tuijian\t推荐"), "{}", s.ime.user_export());
}

#[test]
fn 非纠错选词日志不带_fixed() {
    let mut s = session();
    crate::inputlog::take_captured();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    let sel = crate::inputlog::take_captured().into_iter().find(|l| l["text"] == "你好").unwrap();
    assert!(sel.get("fixed").is_none());
}

#[test]
fn 含纠错段的分段组词不学整句() {
    const M: i64 = 77;
    const E: i64 = 69;
    let mut s = typo_session();
    let mut keys = vec![W, O, M, E, N];
    keys.extend_from_slice(&TUOJIAN);
    let r = press(&mut s, &keys);
    let pos = r["candidateList"].as_array().unwrap().iter().position(|c| c == "我们").unwrap();
    let r = key(&mut s, "onKeyDown", 49 + pos as i64);
    assert_eq!(r["candidateList"][1], "推荐");
    key(&mut s, "onKeyDown", 50);
    let u = s.ime.user_export();
    assert!(u.contains("tuijian\t推荐\t1") && !u.contains("tuojian"), "{u}");
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-backend 纠错 2>&1 | tail -20`
Expected: `纠错候选按正确拼音学习…` 失败(学到的是 `tuojian`),`含纠错段的分段组词不学整句` 失败(出现 `womentuojian`)。

- [ ] **Step 3: `inputlog::select` 加 `fixed`**

```rust
/// 一次选词:`input` 为选词时的完整拼音串,`typed` 为本次消耗的部分,`index` 为选中项在全部候选中的下标;
/// `fixed` 为按键纠错候选纠正后的拼音(学习按它记),其余为 `None` 且不写该字段。
pub fn select(input: &str, context: &str, typed: &str, text: &str, index: usize, top: &[&str], fixed: Option<&str>) {
    let top: Vec<&str> = top.iter().take(TOP).copied().collect();
    let mut line = json!({ "input": input, "context": context, "typed": typed, "text": text, "index": index, "top": top });
    if let Some(f) = fixed {
        line["fixed"] = json!(f);
    }
    write(line);
}
```

- [ ] **Step 4: 会话改动**

`Session` 结构体在 `segments` 字段旁加:

```rust
    /// 本次组词已有一段选的是按键纠错候选:整串拼音含错拼,组词结束时不学整句。
    typo_seg: bool,
```

`Session::new` 的字面量加 `typo_seg: false`。

`select` 中:

```rust
        crate::inputlog::select(&input, &ctx, &typed, &c.text, index, &tops, c.fixed.as_deref());
        // 符号不学习;它吃满剩余输入,必是本次组词的最后一段
        let symbol = c.source == Source::Symbol;
        if !symbol {
            // 按键纠错候选按纠正后的拼音学,user.txt 不留错拼
            let learn_as = c.fixed.clone().unwrap_or_else(|| typed.clone());
            let lessons = c.lessons(&learn_as, &ctx);
            self.ime.learn(&learn_as, &c.text, c.pairs, &ctx, &lessons);
            self.learned.push((learn_as, c.text.clone(), c.pairs, ctx, lessons));
        }
        self.typo_seg |= c.fixed.is_some();
```

整句学习条件改为 `if self.segments > 1 && !symbol && !self.typo_seg {`;紧随 `self.segments = 0;`(组词结束处)加 `self.typo_seg = false;`;`flush_raw` 中 `self.segments = 0;` 之后同样加 `self.typo_seg = false;`。

- [ ] **Step 5: 回放按 `fixed` 学习**

`pyime-replay.rs` 在 `let ctx = v["context"]...` 之后加:

```rust
        // 按键纠错的选择,会话按纠正后的拼音学习,回放保持一致(retract 项里记的也是它)
        let learn_as = v["fixed"].as_str().unwrap_or(typed);
```

并把其后的 `engine.learn(typed, chosen);` 改为 `engine.learn(learn_as, chosen);`,`taught.insert((typed.to_string(), ...` 改为 `taught.insert((learn_as.to_string(), ...`。`episodes.select(...)` 与名次统计仍用 `typed`/`input`,不改。

- [ ] **Step 6: 运行确认通过**

Run: `cargo test -q -p pyime-backend 2>&1 | tail -5`
Expected: `test result: ok`,无失败。

- [ ] **Step 7: 提交**

```bash
git add crates/pyime-backend/src/inputlog.rs crates/pyime-backend/src/session.rs crates/pyime-backend/src/bin/pyime-replay.rs crates/pyime-backend/src/session/tests.rs
git commit -m "会话:按键纠错候选按纠正后拼音学习,含纠错段不学整句,输入日志记 fixed

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: 托盘"按键纠错"菜单项

**Files:**
- Modify: `crates/pyime-backend/src/tray.rs`(常量、`menu`)
- Modify: `crates/pyime-backend/src/ime.rs:343-358`(`toggle_fuzzy` 抽出写盘、新增 `toggle_typo`)
- Modify: `crates/pyime-backend/src/session.rs:296-310`(命令分发)
- Test: `crates/pyime-backend/src/session/tests.rs`

**Interfaces:**
- Consumes: `FuzzyConfig.typo`、`export_off`(Task 2)。
- Produces: `tray::CMD_TYPO: u64 = 15`;`Ime::toggle_typo(&self)`。

- [ ] **Step 1: 写失败的测试,并更新两处既有菜单断言**

`session/tests.rs` 末尾加:

```rust
/// 右键菜单"按键纠错"项的勾选态。
fn typo_checked(s: &mut Session) -> Value {
    let r = s.handle(&json!({"method": "onMenu", "seqNum": 5, "id": "windows-mode-icon"}));
    r["return"].as_array().unwrap().iter().find(|i| i["text"] == "按键纠错").unwrap()["checked"].clone()
}

#[test]
fn 按键纠错菜单项切换并写盘_重启后保持() {
    let p = crate::ime::temp_user_path("typo-menu");
    let f = p.with_file_name("fuzzy.txt");
    let mut s = Session::new(Ime::with_user_path(engine(), p.clone()));
    assert_eq!(typo_checked(&mut s), true);
    assert_eq!(menu_cmd(&mut s, tray::CMD_TYPO)["return"], true);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "纠错\n");
    assert_eq!(typo_checked(&mut s), false);
    let mut s = Session::new(Ime::with_user_path(engine(), p.clone()));
    assert_eq!(typo_checked(&mut s), false);
    assert_eq!(menu_cmd(&mut s, tray::CMD_TYPO)["return"], true);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "");
}
```

既有测试 `右键菜单含版本与三项命令` 中:

```rust
    assert_eq!(texts[1..], ["模糊音", "按键纠错", "本程序默认中文", "编辑用户词库…", "重新加载用户词库", "打开数据目录"]);
```

既有测试 `菜单命令分发到编辑_重载与打开目录` 中 `r["return"][6]["text"]` 改为 `r["return"][7]["text"]`(顶层多一项,重载项后移一位)。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-backend 菜单 2>&1 | tail -20`
Expected: 编译失败(`CMD_TYPO` 不存在)。

- [ ] **Step 3: 实现**

`tray.rs` 在 `CMD_CHINESE_APP` 之后加:

```rust
/// 菜单"按键纠错"(勾选即开)。
pub const CMD_TYPO: u64 = 15;
```

`menu` 返回的数组中 `{ "text": "模糊音", "submenu": rules },` 之后加:

```rust
        { "text": "按键纠错", "id": CMD_TYPO, "checked": fuzzy.typo },
```

并把 `menu` 文档注释中"`fuzzy` 决定模糊音子菜单的勾选"改为"`fuzzy` 决定模糊音子菜单与按键纠错的勾选"。

`ime.rs`:把 `toggle_fuzzy` 中 `set_fuzzy` 与写盘部分抽成私有方法,`toggle_fuzzy` 与新 `toggle_typo` 共用:

```rust
    /// 模糊音子菜单:切换第 `i` 对规则,即时生效并写 `fuzzy.txt`(与 `user.txt` 同目录)。
    pub fn toggle_fuzzy(&self, i: usize) {
        let mut cfg = self.fuzzy();
        let on = !cfg.is_on(i);
        cfg.set(i, on);
        crate::log::line(&format!("菜单:模糊音 {} {}", FuzzyConfig::rule_names()[i], if on { "开" } else { "关" }));
        self.apply_fuzzy(cfg);
    }

    /// 菜单"按键纠错":切换开关,即时生效并写 `fuzzy.txt`。
    pub fn toggle_typo(&self) {
        let mut cfg = self.fuzzy();
        cfg.typo = !cfg.typo;
        crate::log::line(&format!("菜单:按键纠错 {}", if cfg.typo { "开" } else { "关" }));
        self.apply_fuzzy(cfg);
    }

    /// 启用新的模糊音/纠错配置并写 `fuzzy.txt`。
    fn apply_fuzzy(&self, cfg: FuzzyConfig) {
        let text = cfg.export_off();
        self.engine.borrow_mut().set_fuzzy(cfg);
        let Some(path) = fuzzy_path(self.user_path.as_deref()) else { return };
        let _ = path.parent().map(std::fs::create_dir_all);
        if let Err(e) = std::fs::write(&path, text) {
            crate::log::line(&format!("写入 {path:?} 失败: {e}"));
        }
    }
```

`session.rs` 命令分发中,在 `CMD_FUZZY` 分支之前加:

```rust
            (tray::CMD_TYPO, tray::COMMAND_MENU) => {
                self.ime.toggle_typo();
                true
            }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -q -p pyime-backend 2>&1 | tail -5`
Expected: `test result: ok`,无失败。

- [ ] **Step 5: 提交**

```bash
git add crates/pyime-backend/src/tray.rs crates/pyime-backend/src/ime.rs crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "托盘菜单:新增 按键纠错 开关,即时生效并写 fuzzy.txt

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 回归调参、日志回放、耗时与文档

**Files:**
- Modify: `crates/pyime-engine/tests/regression.rs`(`CASES` 加用例)
- Modify: `crates/pyime-engine/src/engine.rs`(仅 `TYPO_PENALTY` 取值,如需)
- Modify: `test-kit/tsf.txt`(实机项)
- 临时脚本(不入库):`/tmp/typo_ana/`

**Interfaces:**
- Consumes: Task 1–5 全部产物;Task 1 Step 1 的 `/tmp/typo_base.tsv`、`/tmp/typo_base_perf.log`。

- [ ] **Step 1: 加回归用例**

`regression.rs` 的 `CASES` 末尾(`];` 之前)加:

```rust
    ("kailv", "考虑", 2),       // 按键纠错:i/o 相邻误按(用户日志 2026-11)
    ("xiachenxu", "小程序", 2), // 按键纠错:漏打 o,叠加 en=eng
    ("tuojian", "推荐", 2),     // 按键纠错:用户例子,模糊整段词 脱缰 不应挡住
```

- [ ] **Step 2: 跑回归并与基线比对**

```bash
cd /data/bak20250527/PYIME
PYIME_DICT=$PWD/installer/pyime.dict PYIME_EN=$PWD/installer/pyime.en PYIME_RANKS_BASE=/tmp/typo_base.tsv \
  cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture > /tmp/typo_reg.log 2>&1; grep -A30 '与基线比对' /tmp/typo_reg.log; grep '通过' /tmp/typo_reg.log | tail -12
```

验收:"与基线比对"中 跌出首位 0、其余变化 0(新增 3 例不在基线里,不参与比对);新增 3 例通过;"通过 x/纠错"桶出现。

- [ ] **Step 3: 若不满足,调 `TYPO_PENALTY`**

- 原有用例出现变化(被纠错词挤动)→ 调大 `TYPO_PENALTY`(每次 +1.0)重跑 Step 2。
- 新增用例未召回或名次 > 2 → 先用 `PYIME_INPUTS=kailv,xiachenxu,tuojian` 跑 `real_engine`(见 Task 1 Step 1 命令)看候选;若纠错词没出现是门槛所致,调小 `TYPO_PENALTY`(每次 −0.5,不低于 0)。
- 找到同时满足两边的取值即停;取区间中值。若不存在这样的取值(调大才护住原用例、调小才救回新用例):**停下,把两边的具体用例与得分报告给用户**,不要擅自放宽门槛或删用例。
- 改了取值则 `cargo test -q -p pyime-engine` 确认单测仍过(单测按 3.0 设计,余量较大;若单测因取值失败,报告而不是改测试)。

- [ ] **Step 4: 用 0.1.17 日志回放统计触发与误伤**

```bash
python3 -I - <<'PY'
import json
comps = json.load(open('/tmp/typo_ana/comps.json'))
out = open('/tmp/typo_ana/replay.tsv', 'w')
for ev in comps:
    last = None
    for k, d in ev:
        if k == 'in' and d:
            last = d
        elif k == 'commit':
            if last and last.isascii() and last.isalpha() and last.islower():
                out.write(f'{last}\t{d}\n')
            break
PY
wc -l /tmp/typo_ana/replay.tsv
```

把 `/tmp/typo_ana/w/src/main.rs` 换成:

```rust
use pyime_engine::dict::Dict;
use pyime_engine::engine::{Engine, Source};
use pyime_engine::fuzzy::FuzzyConfig;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dict = || Dict::from_bytes(std::fs::read(&a[1]).unwrap()).unwrap();
    let on = Engine::new(dict(), FuzzyConfig::default());
    let mut cfg = FuzzyConfig::default();
    cfg.typo = false;
    let off = Engine::new(dict(), cfg);
    let (mut n, mut fired, mut helped, mut moved) = (0, 0, 0, 0);
    for line in std::fs::read_to_string(&a[2]).unwrap().lines() {
        let Some((input, chosen)) = line.split_once('\t') else { continue };
        n += 1;
        let c1 = on.candidates(input);
        let c0 = off.candidates(input);
        let typo: Vec<&str> = c1.iter().filter(|c| c.source == Source::Typo).map(|c| c.text.as_str()).collect();
        if typo.is_empty() { continue }
        fired += 1;
        let r1 = c1.iter().position(|c| c.text == chosen);
        let r0 = c0.iter().position(|c| c.text == chosen);
        if typo.contains(&chosen) { helped += 1 } else if r1 != r0 { moved += 1 }
        println!("{input}\t选{chosen}\t纠错{typo:?}\t名次 {r0:?}→{r1:?}");
    }
    println!("上屏 {n},触发 {fired},选中纠错词 {helped},所选被挤动 {moved}");
}
```

```bash
cd /tmp/typo_ana/w && cargo run -q --release -- /data/bak20250527/PYIME/installer/pyime.dict /tmp/typo_ana/replay.tsv > /tmp/typo_ana/replay.out 2>&1; tail -40 /tmp/typo_ana/replay.out
```

记录最后一行统计与触发明细,写入最终汇报;"所选被挤动"逐条列出(这是误伤参考,不作为本任务的通过门槛)。

- [ ] **Step 5: 耗时**

```bash
cd /data/bak20250527/PYIME
PYIME_DICT=$PWD/installer/pyime.dict PYIME_INPUTS=tuojian,kailv,xiachenxu,zhegewentihao,woxiangquguan \
  cargo test -q -p pyime-engine --release --test real_engine -- --ignored --nocapture > /tmp/typo_perf.log 2>&1; grep -v '^$' /tmp/typo_perf.log | head -12
```

与 `/tmp/typo_base_perf.log` 逐输入对比。验收:每个输入多出 ≤ 5ms。超出则把 `typo.rs` 的 `INSERTS` 收窄为 `b"aeiou"` 再测;仍超出则停下报告。

- [ ] **Step 6: 实机清单**

在 `test-kit/tsf.txt` 末尾按现有"八、…"的格式追加一节"九、按键纠错(0.1.18 起)",内容:

```
九、按键纠错
1. 记事本(默认英文,先单击 Shift 切到中文)打 tuojian:第 2 位出现"推荐";按 2 上屏。再打 tuojian,"推荐"仍在第 2 位(学习记在 tuijian 下,不会变首选)。
2. 打 kailv:第 2 位出现"考虑"。
3. 打 tuojia 停住(末尾残缺):不出现纠错词。
4. 托盘右键:"按键纠错"默认勾选;取消勾选后再打 tuojian,不再出现"推荐";%LOCALAPPDATA%\PYIME\fuzzy.txt 中有一行"纠错"。重新勾选后恢复。
5. 中文状态打 these 后按空格:上屏的是英文(原样或英文候选),不被纠成中文。
```

- [ ] **Step 7: 全量测试、clippy、提交**

```bash
cd /data/bak20250527/PYIME
cargo test -q 2>&1 | grep -E 'test result|FAILED|panicked' | head -20
cargo clippy -q --all-targets 2>&1 | head -30
git add crates/pyime-engine/tests/regression.rs crates/pyime-engine/src/engine.rs test-kit/tsf.txt
git commit -m "按键纠错:回归用例、惩罚取值与实机清单

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: 所有 `test result: ok`;clippy 无新增告警。若 `engine.rs` 未改动,`git add` 对它无副作用。

- [ ] **Step 8: CLAUDE.md 说明(不提交)**

`CLAUDE.md` 有用户未提交的改动,只编辑、不 `git add`。改三处:
- Project Layout 的引擎模块列表在 `symbol` 之后加:"`typo` 按键纠错(输入无精确整段词时查改一处的整段词,第 2 位)"。
- `fuzzy.txt` 说明处补:"含一行 `纠错` 表示关闭按键纠错"。
- 托盘菜单说明处补:"按键纠错(勾选即开,写 `fuzzy.txt`;待实机验证)";待办补一条"按键纠错待按 `test-kit/tsf.txt` 第九节实机验证(设计 `docs/superpowers/specs/2026-11-03-typo-correction-design.md`)"。

最终汇报须写明:CLAUDE.md 已改未提交;回归比对结果;`TYPO_PENALTY` 最终值;回放统计;耗时对比。
