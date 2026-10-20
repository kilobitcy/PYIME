# 英文候选(IT/科技词补全) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 拼音输入串可出英文候选(完全匹配 + 前缀补全、规范大小写),回车原样上屏满 2 次自动学成英文词。

**Architecture:** 引擎新增 `english.rs`(词表解析、前缀查询、回车计数);`Engine` 内按"拼音三态"(精确通/残缺通/不通)把英文候选插入中文候选列表,来源记 `Source::English`。后端载入 `pyime.en`/`en.txt`/`en_learned.txt`,回车时调 `learn_raw`。打包新增 `pyime-en-build` 生成 `pyime.en`。

**Tech Stack:** Rust 2021(无新依赖)、PIME 1.3.0 JSON 协议、makensis。

Spec:`docs/superpowers/specs/2026-10-20-english-candidates-design.md`

## Global Constraints

- 注释、commit message 一律简体中文;commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 键必须全是 ASCII 小写字母,否则该行跳过。
- 档位:0 = 自带 IT 词表/用户 `en.txt`/回车学会;1 = rime-ice `en_ext`;2 = rime-ice `en`。
- 补全要求输入 ≥ 4 字母;回车学习满 2 次才出现;回车学习不写 `user.txt`。
- 不加新 crate 依赖;单次写文件 ≤ 150 行。
- 每个任务结束跑 `cargo test` 与 `cargo clippy`,无新警告。
- 不 push、不打 tag、不发布(需先问用户)。

## File Structure

- Create `crates/pyime-engine/src/english.rs`:英文词表与回车计数(纯数据结构,不知道拼音)。
- Modify `crates/pyime-engine/src/lib.rs`:`pub mod english;`
- Modify `crates/pyime-engine/src/engine.rs`:`Source::English`、三态判定、英文落位、`set_english`/`learn_raw`/`english`、`forget` 连带。
- Create `crates/pyime-engine/src/bin/pyime-en-build.rs`:合并词表为 `pyime.en`。
- Create `installer/en_it.txt`:自带 IT 词表。
- Modify `scripts/package.sh`、`installer/pyime.nsi`、`installer/NOTICE.md`:打包。
- Modify `crates/pyime-backend/src/ime.rs`:载入英文词表、存 `en_learned.txt`、`learn_raw`。
- Modify `crates/pyime-backend/src/session.rs`、`session/tests.rs`:回车学习。
- Modify `crates/pyime-backend/src/inputlog.rs`:`enter_raw` 事件。
- Modify `crates/pyime-engine/tests/regression.rs`、`crates/pyime-backend/src/bin/pyime-replay.rs`、`CLAUDE.md`:评测链路。

---

### Task 1: `english.rs` 词表模块

**Files:**
- Create: `crates/pyime-engine/src/english.rs`
- Modify: `crates/pyime-engine/src/lib.rs`

**Interfaces:**
- Produces(后续任务依赖,名称不得改):
  - `pub const TIER_CUSTOM: u8 = 0; pub const TIER_EXT: u8 = 1; pub const TIER_BASE: u8 = 2;`
  - `pub const MIN_COMPLETE: usize = 4; pub const RAW_LEARN_MIN: u32 = 2;`
  - `pub struct Hit<'a> { pub word: &'a str, pub tier: u8 }`(`Debug, Clone, Copy, PartialEq, Eq`)
  - `pub fn parse_rime(text: &str) -> Vec<(String, String)>`(键, 词)
  - `pub fn parse_list(text: &str) -> Vec<(String, String)>`(键, 词)
  - `pub fn parse_compiled(text: &str) -> Vec<(String, String, u8)>`(键, 词, 档位)
  - `#[derive(Default)] pub struct English`,方法:`new(Vec<(String, String, u8)>) -> English`、`len(&self) -> usize`、
    `export(&self) -> String`、`lookup(&self, input: &str) -> (Vec<Hit<'_>>, Vec<Hit<'_>>)`、
    `learn_raw(&mut self, s: &str)`、`forget(&mut self, word: &str) -> bool`、`export_raw(&self) -> String`、`import_raw(&mut self, text: &str)`

- [ ] **Step 1: 写失败的测试** —— 新建 `crates/pyime-engine/src/english.rs`,先只放测试模块(实现留到 Step 3),并在 `lib.rs` 加 `pub mod english;`(放在 `pub mod engine;` 之后)。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn s(k: &str, w: &str) -> (String, String) {
        (k.to_string(), w.to_string())
    }

    fn words<'a>(h: &[Hit<'a>]) -> Vec<&'a str> {
        h.iter().map(|h| h.word).collect()
    }

    fn table() -> English {
        let mut items: Vec<(String, String, u8)> = parse_list("subagent\nKubernetes\nClaude Code\n").into_iter().map(|(k, w)| (k, w, TIER_CUSTOM)).collect();
        let rime = "---\nname: en\n...\nhell\thell\t2\nhe'll\thell\t1\nkube\tkube\napple\tapple\napplication\tapplication\nApple\tApple\n";
        items.extend(parse_rime(rime).into_iter().map(|(k, w)| (k, w, TIER_BASE)));
        English::new(items)
    }

    #[test]
    fn rime_跳过yaml头_注释_空行与含非字母的编码_第三列忽略() {
        let v = parse_rime("---\nname: en\n...\n# 注\nhell\thell\t2\nhe'll\thell\t1\nKubernetes\tK8s\nREADME.md\treadme.md\n\nGitHub\tGitHub\n");
        assert_eq!(v, vec![s("hell", "hell"), s("hell", "he'll"), s("github", "GitHub")]);
    }

    #[test]
    fn 词表行_无键时去空格转小写_显式键_跳过注释与非法键() {
        let v = parse_list("# 注释\nClaude Code\nC++\tcpp\nC#\n\nsubagent\r\n");
        assert_eq!(v, vec![s("claudecode", "Claude Code"), s("cpp", "C++"), s("subagent", "subagent")]);
    }

    #[test]
    fn 编译格式往返一致() {
        let e = table();
        let back = English::new(parse_compiled(&e.export()));
        assert_eq!(back.len(), e.len());
        assert_eq!(back.export(), e.export());
    }

    #[test]
    fn 完全匹配与补全按档位_词长_先后排序() {
        let e = table();
        let (ex, more) = e.lookup("kube");
        assert_eq!(words(&ex), vec!["kube"]);
        assert_eq!(words(&more), vec!["Kubernetes"]);
        let (ex, more) = e.lookup("appl");
        assert!(ex.is_empty());
        assert_eq!(words(&more), vec!["apple", "Apple", "application"]);
        assert_eq!(words(&e.lookup("hell").0), vec!["hell", "he'll"]);
        assert_eq!(words(&e.lookup("claudecode").0), vec!["Claude Code"]);
    }

    #[test]
    fn 不足4字母不补全() {
        let (ex, more) = table().lookup("kub");
        assert!(ex.is_empty() && more.is_empty());
    }

    #[test]
    fn 同键同词留最小档位() {
        let e = English::new(vec![("kube".into(), "kube".into(), TIER_BASE), ("kube".into(), "kube".into(), TIER_CUSTOM)]);
        assert_eq!(e.len(), 1);
        assert_eq!(e.lookup("kube").0, vec![Hit { word: "kube", tier: TIER_CUSTOM }]);
    }

    #[test]
    fn 回车学满2次才出现_可存取_可删除() {
        let mut e = table();
        e.learn_raw("zhegnshi");
        assert!(e.lookup("zhegnshi").0.is_empty());
        e.learn_raw("zhegnshi");
        assert_eq!(e.lookup("zhegnshi").0, vec![Hit { word: "zhegnshi", tier: TIER_CUSTOM }]);
        assert_eq!(words(&e.lookup("zhegn").1), vec!["zhegnshi"]);
        let mut f = table();
        f.import_raw(&e.export_raw());
        assert_eq!(words(&f.lookup("zhegnshi").0), vec!["zhegnshi"]);
        assert!(e.forget("zhegnshi"));
        assert!(!e.forget("zhegnshi"));
        assert!(e.lookup("zhegnshi").0.is_empty());
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p pyime-engine --lib english`
Expected: 编译失败(`English`、`parse_rime` 等未定义)。

- [ ] **Step 3: 写实现** —— 在 `english.rs` 测试模块之前加入:

```rust
//! 英文词表:键(全小写字母)→ 词,带档位;完全匹配与前缀补全。
//! 另记"回车原样上屏"的次数,满 `RAW_LEARN_MIN` 次的串以档位 0 当英文词。不涉及拼音。
use std::collections::{HashMap, HashSet};

/// 自带 IT 词表、用户 `en.txt`、回车学会的词。
pub const TIER_CUSTOM: u8 = 0;
/// rime-ice `en_ext.dict.yaml`。
pub const TIER_EXT: u8 = 1;
/// rime-ice `en.dict.yaml`。
pub const TIER_BASE: u8 = 2;
/// 输入至少这么多字母才做前缀补全。
pub const MIN_COMPLETE: usize = 4;
/// 回车原样上屏满这么多次,才把该串当英文词。
pub const RAW_LEARN_MIN: u32 = 2;
const MAX_COUNT: u32 = 1000;

struct Entry {
    key: String,
    word: String,
    tier: u8,
    /// 载入先后,作同档同长时的次序。
    seq: usize,
}

/// 一条英文候选。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit<'a> {
    pub word: &'a str,
    pub tier: u8,
}

#[derive(Default)]
pub struct English {
    /// 按 (键, 词) 排序。
    entries: Vec<Entry>,
    /// 回车原样上屏的串 → 次数。
    raw: HashMap<String, u32>,
}

fn valid_key(k: &str) -> bool {
    !k.is_empty() && k.bytes().all(|b| b.is_ascii_lowercase())
}

/// rime-ice 英文词库:跳过 `...` 之前的 YAML 头、`#` 注释与空行;行 `词⇥编码[⇥权重]`,
/// 键取编码转小写,含非字母的编码(如 `K8s`)跳过,权重忽略。返回 (键, 词)。
pub fn parse_rime(text: &str) -> Vec<(String, String)> {
    let body = text.split_once("\n...\n").map_or(text, |(_, rest)| rest);
    body.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut c = l.trim_end_matches('\r').split('\t');
            let (w, k) = (c.next()?, c.next()?.to_ascii_lowercase());
            (!w.is_empty() && valid_key(&k)).then(|| (k, w.to_string()))
        })
        .collect()
}

/// 自带/用户词表:一行 `词` 或 `词⇥键`,`#` 注释;无键时键 = 词去空格转小写。返回 (键, 词)。
pub fn parse_list(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut c = l.split('\t');
            let w = c.next()?.trim();
            let k = match c.next() {
                Some(k) => k.trim().to_ascii_lowercase(),
                None => w.replace(' ', "").to_ascii_lowercase(),
            };
            (!w.is_empty() && valid_key(&k)).then(|| (k, w.to_string()))
        })
        .collect()
}

/// 编译好的 `pyime.en`:行 `键⇥词⇥档位`,坏行跳过。
pub fn parse_compiled(text: &str) -> Vec<(String, String, u8)> {
    text.lines()
        .filter_map(|l| {
            let mut c = l.split('\t');
            let (k, w, t) = (c.next()?, c.next()?, c.next()?.trim().parse::<u8>().ok()?);
            (valid_key(k) && !w.is_empty()).then(|| (k.to_string(), w.to_string(), t))
        })
        .collect()
}

/// 按 档位 → 词长 → 先后 排序,同词只留第一条。
fn rank<'a>(mut v: Vec<(u8, usize, usize, &'a str)>) -> Vec<Hit<'a>> {
    v.sort();
    let mut seen = HashSet::new();
    v.into_iter().filter(|h| seen.insert(h.3)).map(|(tier, _, _, word)| Hit { word, tier }).collect()
}

impl English {
    /// 由 (键, 词, 档位) 建表;同键同词留最小档位,输入顺序作同档同长时的次序。
    pub fn new(items: Vec<(String, String, u8)>) -> English {
        let mut entries: Vec<Entry> = items.into_iter().enumerate().map(|(seq, (key, word, tier))| Entry { key, word, tier, seq }).collect();
        entries.sort_by(|a, b| (&a.key, &a.word, a.tier, a.seq).cmp(&(&b.key, &b.word, b.tier, b.seq)));
        entries.dedup_by(|cur, kept| cur.key == kept.key && cur.word == kept.word);
        English { entries, raw: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 写成 `pyime.en`:每行 `键⇥词⇥档位`,按载入先后。
    pub fn export(&self) -> String {
        let mut v: Vec<&Entry> = self.entries.iter().collect();
        v.sort_by_key(|e| e.seq);
        v.iter().map(|e| format!("{}\t{}\t{}\n", e.key, e.word, e.tier)).collect()
    }

    /// `input` 的英文候选:(完全匹配, 补全);补全只在 `input` ≥ `MIN_COMPLETE` 字母时给出。
    /// 回车学会的串(次数 ≥ `RAW_LEARN_MIN`)按档位 0 参与。
    pub fn lookup(&self, input: &str) -> (Vec<Hit<'_>>, Vec<Hit<'_>>) {
        let complete = input.len() >= MIN_COMPLETE;
        let (mut exact, mut more) = (Vec::new(), Vec::new());
        let start = self.entries.partition_point(|e| e.key.as_str() < input);
        for e in self.entries[start..].iter().take_while(|e| e.key.starts_with(input)) {
            let h = (e.tier, e.word.len(), e.seq, e.word.as_str());
            if e.key == input {
                exact.push(h);
            } else if complete {
                more.push(h);
            }
        }
        for (w, &n) in self.raw.iter().filter(|&(w, &n)| n >= RAW_LEARN_MIN && w.starts_with(input)) {
            let h = (TIER_CUSTOM, w.len(), usize::MAX - n as usize, w.as_str());
            if w == input {
                exact.push(h);
            } else if complete {
                more.push(h);
            }
        }
        (rank(exact), rank(more))
    }

    /// 记一次回车原样上屏的串。
    pub fn learn_raw(&mut self, s: &str) {
        let n = self.raw.entry(s.to_string()).or_insert(0);
        *n = (*n + 1).min(MAX_COUNT);
    }

    /// 删除回车学到的 `word`;返回是否删了。词表本身不受影响。
    pub fn forget(&mut self, word: &str) -> bool {
        self.raw.remove(word).is_some()
    }

    /// 回车计数存盘文本:`词⇥次数` 行,存 `en_learned.txt`。
    pub fn export_raw(&self) -> String {
        let mut lines: Vec<String> = self.raw.iter().map(|(w, n)| format!("{w}\t{n}")).collect();
        lines.sort();
        lines.join("\n")
    }

    /// 读回 `export_raw` 的结果;坏行跳过。
    pub fn import_raw(&mut self, text: &str) {
        for (w, n) in text.lines().filter_map(|l| l.split_once('\t')) {
            if let (true, Ok(n)) = (valid_key(w), n.trim().parse::<u32>()) {
                self.raw.insert(w.to_string(), n.min(MAX_COUNT));
            }
        }
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p pyime-engine --lib english && cargo clippy -p pyime-engine`
Expected: 7 个 english 测试 PASS;clippy 无新警告。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-engine/src/english.rs crates/pyime-engine/src/lib.rs
git commit -m "引擎:新增英文词表模块(解析、前缀补全、回车计数)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 2: 引擎合并英文候选(三态落位、回车学习)

**Files:**
- Modify: `crates/pyime-engine/src/engine.rs`(`Source`:58-64;常量区:8-34;`Engine` 结构与 `new`:111-124;`forget`:175-178;`candidates_after`:263-293;测试模块末尾)
- Modify: `crates/pyime-engine/tests/regression.rs:101-106`(`bucket` 补 `Source::English` 分支,否则编译不过)

**Interfaces:**
- Consumes(Task 1):`English`、`Hit`、`TIER_CUSTOM`、`English::lookup/learn_raw/forget`。
- Produces:
  - `Source::English`
  - `Engine::set_english(&mut self, english: English)`
  - `Engine::english(&self) -> Option<&English>`
  - `Engine::learn_raw(&mut self, input: &str) -> bool`(记了返回 true)
  - `Engine::forget` 同时删回车学习

- [ ] **Step 1: 写失败的测试** —— 加到 `engine.rs` 的 `mod tests` 末尾(`use super::*;` 已有,Step 3 在 `engine.rs` 顶部导入 `English` 后即可用;Step 2 编译失败属预期):

```rust
    fn with_english(entries: &[(&str, &str, u32)], en: &[(&str, &str, u8)]) -> Engine {
        let mut e = engine(entries);
        e.set_english(English::new(en.iter().map(|&(k, w, t)| (k.to_string(), w.to_string(), t)).collect()));
        e
    }

    #[test]
    fn 拼音不通时英文排最前_合计至多3条() {
        let en = [("playwright", "Playwright", 0), ("playwright", "playwright", 2), ("playwrights", "playwrights", 2), ("playwriting", "playwriting", 2)];
        let e = with_english(&[("pa", "怕", 100)], &en);
        let c = e.candidates("playw");
        assert_eq!(c.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), vec!["Playwright", "playwright", "playwrights"]);
        assert!(c.iter().all(|c| c.source == Source::English && c.consumed == 5 && c.cuts.is_empty()));
    }

    #[test]
    fn 残缺通时完全匹配的英文排第一() {
        let e = with_english(&[("su", "苏", 100), ("ba", "把", 100), ("gen", "跟", 100), ("ta", "他", 100)], &[("subagent", "subagent", 0)]);
        let t = texts(&e, "subagent");
        assert_eq!(t[0], "subagent");
        assert!(t.len() > 1, "中文候选仍在");
    }

    #[test]
    fn 残缺通时只补全档位0_插在第3位() {
        let e = with_english(&[("ku", "苦", 100), ("bei", "被", 100)], &[("kubernetes", "Kubernetes", 0), ("kubectl", "kubectl", 2)]);
        let t = texts(&e, "kube");
        assert_eq!(t[2], "Kubernetes");
        assert!(!t[..2].iter().any(|s| s.is_ascii()));
        assert!(!t.contains(&"kubectl".to_string()));
    }

    #[test]
    fn 精确通时完全匹配的英文插第3位_不补全() {
        let dict = [("chang e", "嫦娥", 1000), ("chang ge", "唱歌", 900), ("chang", "长", 800), ("e", "鹅", 500)];
        let e = with_english(&dict, &[("change", "change", 2), ("changes", "changes", 2)]);
        let t = texts(&e, "change");
        assert_eq!(t[0], "嫦娥");
        assert_eq!(t[2], "change");
        assert!(!t.contains(&"changes".to_string()));
    }

    #[test]
    fn 精确通时不足4字母只出档位0() {
        let e = with_english(&[("a", "啊", 100), ("pi", "皮", 100), ("me", "么", 100)], &[("api", "API", 0), ("me", "me", 2)]);
        assert_eq!(texts(&e, "api").iter().position(|t| t == "API"), Some(2));
        assert!(!texts(&e, "me").contains(&"me".to_string()));
    }

    #[test]
    fn 学过的英文由用户短语排首位且不重复() {
        let mut e = with_english(&[("ku", "苦", 100), ("bei", "被", 100)], &[("kubernetes", "Kubernetes", 0)]);
        e.learn("kube", "Kubernetes");
        let c = e.candidates("kube");
        assert_eq!(c[0].text, "Kubernetes");
        assert_eq!(c[0].source, Source::English);
        assert_eq!(c.iter().filter(|c| c.text == "Kubernetes").count(), 1);
    }

    #[test]
    fn 未注入或空英文词表时候选不变() {
        let dict = [("ni", "你", 100), ("hao", "好", 90), ("ni hao", "你好", 80)];
        let plain = engine(&dict);
        let mut with = engine(&dict);
        with.set_english(English::default());
        for input in ["nihao", "ni", "nih"] {
            assert_eq!(plain.candidates(input), with.candidates(input));
        }
    }

    #[test]
    fn 回车学习_不通或残缺通且满4字母才记_满2次出现_forget后消失() {
        let mut e = with_english(&[("zhe", "这", 100), ("chang e", "嫦娥", 100)], &[]);
        assert!(!e.learn_raw("change"), "精确通不学");
        assert!(!e.learn_raw("zhe"), "不足 4 字母不学");
        assert!(e.learn_raw("zhegnshi"));
        assert!(!texts(&e, "zhegnshi").contains(&"zhegnshi".to_string()));
        assert!(e.learn_raw("zhegnshi"));
        assert_eq!(texts(&e, "zhegnshi")[0], "zhegnshi");
        e.forget("zhegnshi");
        assert!(!texts(&e, "zhegnshi").contains(&"zhegnshi".to_string()));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p pyime-engine --lib engine`
Expected: 编译失败(`set_english`、`learn_raw`、`Source::English` 未定义)。

- [ ] **Step 3: 实现** —— `engine.rs` 按下列片段修改。

3a. 顶部 `use` 区加:

```rust
use crate::english::{English, Hit, TIER_CUSTOM};
```

3b. 常量区(`MAX_CANDIDATES` 之后)加:

```rust
/// 英文候选落位:拼音不通时英文合计至多几条;残缺通时完全匹配、档位 0 补全各至多几条。
const EN_UNPARSED: usize = 3;
const EN_PARTIAL_EXACT: usize = 2;
const EN_PARTIAL_COMPLETE: usize = 2;
/// 英文插入位置的下标(第 3 位)。
const EN_SLOT: usize = 2;
/// 精确通时英文完全匹配至少几个字母才出(档位 0 不限)。
const EN_EXACT_MIN: usize = 4;
```

3c. `Source` 改为:

```rust
/// 候选来源:整段词(词典或用户短语吃满输入)、整句(Viterbi 拼装)、前缀词(只吃前一段)、英文(文字全为 ASCII)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Whole,
    Sentence,
    Prefix,
    English,
}
```

3d. `Acc` 定义之后加三态:

```rust
/// 中文候选吃满输入的情况,决定英文候选落位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cover {
    /// 没有吃满输入的中文。
    None,
    /// 吃满输入的路径都含残缺音节词。
    Partial,
    /// 有不含残缺音节词的路径吃满输入(模糊词允许)。
    Exact,
}

/// 沿词的起止做可达性:只走完整词能到末尾为精确通,走上残缺词才到为残缺通。
fn cover(words: &[Word], len: usize) -> Cover {
    let (mut exact, mut any) = (vec![false; len + 1], vec![false; len + 1]);
    exact[0] = true;
    any[0] = true;
    let mut ws: Vec<&Word> = words.iter().filter(|w| w.end <= len).collect();
    ws.sort_by_key(|w| w.start);
    for w in ws {
        if any[w.start] {
            any[w.end] = true;
        }
        if exact[w.start] && !w.partial {
            exact[w.end] = true;
        }
    }
    if exact[len] { Cover::Exact } else if any[len] { Cover::Partial } else { Cover::None }
}
```

3e. `Engine` 结构加字段 `english: Option<English>,`(放在 `gram_threshold` 之后),`new` 里加 `english: None`。

3f. `forget` 改为:

```rust
    /// 删除词 `text` 的全部个人学习(含回车学会的英文串;词库不受影响)。
    pub fn forget(&mut self, text: &str) {
        self.user.forget(text);
        if let Some(en) = &mut self.english {
            en.forget(text);
        }
    }
```

3g. `forget` 之后加:

```rust
    pub fn set_english(&mut self, english: English) {
        self.english = Some(english);
    }

    pub fn english(&self) -> Option<&English> {
        self.english.as_ref()
    }

    /// 回车原样上屏了 `input`:全小写字母、≥ 4 个且拼音非精确通时记一次;返回是否记了。
    pub fn learn_raw(&mut self, input: &str) -> bool {
        if input.len() < EN_EXACT_MIN || !input.bytes().all(|b| b.is_ascii_lowercase()) || cover(&self.words(input, ""), input.len()) == Cover::Exact {
            return false;
        }
        self.english.get_or_insert_with(English::default).learn_raw(input);
        true
    }
```

3h. `candidates_after` 拆成两段(原函数体除最后的 `self.assemble(&words)` 外原样移入 `words`,末尾改为返回 `words`):

```rust
    /// 同 `candidates`,`context` 是光标前已上屏的文字,用于首词的搭配加分。
    pub fn candidates_after(&self, input: &str, context: &str) -> Vec<Candidate> {
        let words = self.words(input, context);
        let mut out = self.assemble(&words);
        if let Some(en) = &self.english {
            place_english(en, input, cover(&words, input.len()), &mut out);
        }
        for c in out.iter_mut().filter(|c| c.text.is_ascii()) {
            c.source = Source::English;
        }
        out.truncate(MAX_CANDIDATES);
        out
    }

    /// 收集从各可达位置出发的词与匹配的用户短语;有上文时首词加搭配分。
    fn words(&self, input: &str, context: &str) -> Vec<Word> {
        let mut words = Vec::new();
        let mut reach = vec![false; input.len() + 1];
        reach[0] = true;
        for a in 0..input.len() {
            if !reach[a] {
                continue;
            }
            for s in syllables_at(input, a) {
                reach[a + s.len()] = true;
            }
            self.grow(input, a, a, "", Acc::default(), 0, &mut Vec::new(), &mut words);
        }
        for (typed, text, bonus) in self.user.matching(input) {
            if !words.iter().any(|w| w.start == 0 && w.end == typed.len() && w.text == text) {
                words.push(Word { start: 0, end: typed.len(), text: text.to_string(), score: USER_PHRASE_BASE + bonus, fuzzy: 0, pairs: 0, cuts: Vec::new(), partial: false, learned: false });
            }
        }
        if !context.is_empty() {
            for w in words.iter_mut().filter(|w| w.start == 0) {
                w.score += self.gram_bonus(context, &w.text);
            }
        }
        words
    }
```

3i. 文件中 `fn sentences` 之前加两个自由函数:

```rust
/// 按拼音三态把英文候选插进 `out`;`input` 须全为小写字母。
fn place_english(en: &English, input: &str, cover: Cover, out: &mut Vec<Candidate>) {
    if input.is_empty() || !input.bytes().all(|b| b.is_ascii_lowercase()) {
        return;
    }
    let (exact, more) = en.lookup(input);
    match cover {
        Cover::None => insert_english(out, 0, input, exact.iter().chain(&more).take(EN_UNPARSED)),
        Cover::Partial => {
            insert_english(out, 0, input, exact.iter().take(EN_PARTIAL_EXACT));
            insert_english(out, EN_SLOT, input, more.iter().filter(|h| h.tier == TIER_CUSTOM).take(EN_PARTIAL_COMPLETE));
        }
        Cover::Exact => insert_english(out, EN_SLOT, input, exact.iter().filter(|h| input.len() >= EN_EXACT_MIN || h.tier == TIER_CUSTOM).take(1)),
    }
}

/// 从下标 `at`(超出则接末尾)起依次插入英文候选,已有同文字的跳过。
fn insert_english<'a>(out: &mut Vec<Candidate>, at: usize, input: &str, hits: impl Iterator<Item = &'a Hit<'a>>) {
    let mut i = at.min(out.len());
    for h in hits {
        if out.iter().any(|c| c.text == h.word) {
            continue;
        }
        out.insert(i, Candidate { text: h.word.to_string(), consumed: input.len(), fuzzy: 0, pairs: 0, cuts: Vec::new(), parts: Vec::new(), source: Source::English });
        i += 1;
    }
}
```

3j. `crates/pyime-engine/tests/regression.rs` 的 `bucket` 中 `match` 加一行 `Source::English => "英文",`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p pyime-engine && cargo clippy -p pyime-engine --all-targets`
Expected: 新增 8 个测试与原有测试全部 PASS;clippy 无新警告。
若 `残缺通时只补全档位0_插在第3位` 失败,先打印 `texts(&e, "kube")` 核对中文部分(预期 `["苦被", "苦"]`),按实际中文条数分析,不得放松"Kubernetes 在下标 2"的断言。

- [ ] **Step 5: 回归对照不退步**(真实词典,未载英文词表,验证三态改造不影响中文)

Run:
```bash
PYIME_DICT=$PWD/installer/pyime.dict PYIME_GRAM=/tmp/gram/pyime.gram cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture
```
Expected: 与改动前一致——`通过 36/39`,失败的仍只有 `zaizuijia`、`fengzhong`、`jintianxingqijia` 三条(既有失败,非本次引入)。

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-engine/src/engine.rs crates/pyime-engine/tests/regression.rs
git commit -m "引擎:按拼音三态合并英文候选,新增回车原样上屏学习

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 3: 英文词表编译工具、自带 IT 词表与打包

**Files:**
- Create: `crates/pyime-engine/src/bin/pyime-en-build.rs`
- Create: `installer/en_it.txt`
- Modify: `scripts/package.sh`(编词典那行之后)、`installer/pyime.nsi:29-30`、`installer/NOTICE.md`、`.gitignore`

**Interfaces:**
- Consumes(Task 1):`parse_list`、`parse_rime`、`English::new/export/len`、`TIER_*`。
- Produces:`installer/pyime.en`(每行 `键⇥词⇥档位`),Task 4/5 载入。

- [ ] **Step 1: 写编译工具** `crates/pyime-engine/src/bin/pyime-en-build.rs`:

```rust
//! 英文词表编译器:`pyime-en-build <输出 pyime.en> <en_it.txt> <en_ext.dict.yaml> <en.dict.yaml>`
//! 自带 IT 词表为档位 0,rime-ice en_ext 为 1、en 为 2;同键同词留最小档位。
use pyime_engine::english::{parse_list, parse_rime, English, TIER_BASE, TIER_CUSTOM, TIER_EXT};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [out, it, ext, base] = args.as_slice() else {
        eprintln!("用法: pyime-en-build <输出 pyime.en> <en_it.txt> <en_ext.dict.yaml> <en.dict.yaml>");
        std::process::exit(2);
    };
    let read = |p: &str| {
        std::fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("读取 {p} 失败: {e}");
            std::process::exit(1);
        })
    };
    let mut items = Vec::new();
    for (list, tier) in [(parse_list(&read(it)), TIER_CUSTOM), (parse_rime(&read(ext)), TIER_EXT), (parse_rime(&read(base)), TIER_BASE)] {
        eprintln!("档位 {tier}: {} 条", list.len());
        items.extend(list.into_iter().map(|(k, w)| (k, w, tier)));
    }
    let en = English::new(items);
    std::fs::write(out, en.export()).expect("写出英文词表失败");
    eprintln!("已写出 {out}({} 条)", en.len());
}
```

- [ ] **Step 2: 写自带 IT 词表** `installer/en_it.txt`(内容如下,一行一词;刻意不收 `CI`、`bug`、`Mac`、`Bash` 等会与拼音/简拼冲突的短词:`ci` 是完整音节,`bug`/`mac`/`bash` 会被判残缺通而抢首位):

```text
# PYIME 自带 IT/科技英文词表:一行 `词` 或 `词⇥键`(键只含字母,省略时取词去空格转小写)。
# 档位 0,优先于 rime-ice 英文词库。避免收 ≤4 字母且像拼音开头的词(会抢中文首位)。
subagent
subagents
agentic
Claude
Claude Code
Anthropic
OpenAI
ChatGPT
Gemini
Copilot
Cursor
Codex
Ollama
Hugging Face
PyTorch
TensorFlow
LLM
LLMs
MCP
RAG
prompt
prompts
token
tokens
embedding
embeddings
API
APIs
SDK
CLI
GUI
IDE
JSON
YAML
TOML
HTML
CSS
URL
HTTP
HTTPS
SSH
SQL
NoSQL
OAuth
JWT
WebSocket
gRPC
GraphQL
GitHub
GitLab
Git
commit
rebase
worktree
DevOps
Docker
Dockerfile
Kubernetes
Linux
Ubuntu
Windows
macOS
WSL
Playwright
Puppeteer
Selenium
TypeScript
JavaScript
Python
Rust
Cargo
Golang
Java
Kotlin
Swift
React
Vue
Next.js	nextjs
Node.js	nodejs
npm
pnpm
Vite
webpack
Tailwind
PostgreSQL
MySQL
Redis
MongoDB
SQLite
Nginx
VS Code	vscode
Vim
Neovim
tmux
PowerShell
regex
debug
refactor
backend
frontend
fullstack
deploy
localhost
README
changelog
Markdown
Mermaid
AWS
Azure
GCP
Cloudflare
Vercel
Supabase
GPU
CPU
CUDA
NVIDIA
Android
iPhone
WeChat
PIME
Rime
```

- [ ] **Step 3: 跑编译工具验证**

Run:
```bash
cargo run --release -q -p pyime-engine --bin pyime-en-build -- /tmp/pyime.en installer/en_it.txt /tmp/rime-ice/en_dicts/en_ext.dict.yaml /tmp/rime-ice/en_dicts/en.dict.yaml
grep -P '^(subagent|kubernetes|github|playwright|nodejs)\t' /tmp/pyime.en
grep -cP '\t[^\t]*\t[^012]$' /tmp/pyime.en
```
Expected:stderr 打出三档条数(档位 0 约 125、1 约 2500、2 约 2.3 万)与总条数;第一个 grep 每个键都有档位 `0` 的行(`subagent⇥subagent⇥0`、`kubernetes⇥Kubernetes⇥0`、`github⇥GitHub⇥0`、`playwright⇥Playwright⇥0`、`nodejs⇥Node.js⇥0`);第二个 grep 输出 `0`。

- [ ] **Step 4: 接入打包**

`scripts/package.sh` 在 `cargo run ... pyime-dict-build ...` 两行之后加:

```bash
E="$RIME/en_dicts"
cargo run --release -q -p pyime-engine --bin pyime-en-build -- installer/pyime.en \
  installer/en_it.txt "$E/en_ext.dict.yaml" "$E/en.dict.yaml"
```

`installer/pyime.nsi` 在 `File "pyime.dict"` 之后加一行 `File "pyime.en"`(卸载段已 `RMDir /r "$INSTDIR\pyime"`,无需改)。

`.gitignore` 加一行 `installer/pyime.en`。

`installer/NOTICE.md` 在"词典 `pyime.dict`"那条之后加:

```markdown
- 英文词表 `pyime.en`:由雾凇拼音 rime-ice(GPL-3.0)的 `en_dicts/en`、`en_dicts/en_ext` 与本项目自带 IT 词表 `en_it.txt` 合并而成,https://github.com/iDvel/rime-ice
```

- [ ] **Step 5: 验证打包脚本语法与工具构建**

Run: `bash -n scripts/package.sh && cargo build -p pyime-engine --bins && cargo clippy -p pyime-engine --all-targets`
Expected: 无输出错误;clippy 无新警告。(完整 `package.sh` 含交叉编译与 makensis,留到 Task 5 末尾统一跑。)

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-engine/src/bin/pyime-en-build.rs installer/en_it.txt scripts/package.sh installer/pyime.nsi installer/NOTICE.md .gitignore
git commit -m "打包:新增 pyime-en-build 与自带 IT 英文词表,安装包附带 pyime.en

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 4: 后端载入英文词表与回车学习

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs`(`use` 区、路径函数区 50-62、`Ime` 结构 12-18、`load`/`with_engine` 77-95、`forget` 146-151、`save` 之后)
- Modify: `crates/pyime-backend/src/session.rs:455`(`VK_RETURN` 分支)
- Modify: `crates/pyime-backend/src/inputlog.rs`(`cancel` 之后)
- Test: `crates/pyime-backend/src/session/tests.rs`(末尾)

**Interfaces:**
- Consumes(Task 1/2):`parse_compiled`、`parse_list`、`English::new/import_raw/export_raw`、`TIER_CUSTOM`、`Engine::set_english/english/learn_raw/forget`。
- Produces:`Ime::learn_raw(&self, input: &str) -> bool`;`inputlog::enter_raw(input: &str, counted: bool)`(日志行 `{"enter_raw": 拼音, "counted": bool}`,Task 5 回放用)。

- [ ] **Step 1: 写失败的测试** —— 加到 `session/tests.rs` 末尾:

```rust
/// 小写字母串 → 对应的虚拟键码序列。
fn letters(s: &str) -> Vec<i64> {
    s.bytes().map(|b| i64::from(b.to_ascii_uppercase())).collect()
}

#[test]
fn 回车原样上屏的串满2次出现在候选首位_并记日志() {
    let mut s = session();
    crate::inputlog::take_captured();
    for _ in 0..2 {
        press(&mut s, &letters("zhegnshi"));
        assert_eq!(key(&mut s, "onKeyDown", VK_RETURN)["commitString"], "zhegnshi");
    }
    let logs = crate::inputlog::take_captured();
    assert_eq!(logs.iter().filter(|l| l["enter_raw"] == "zhegnshi" && l["counted"] == true).count(), 2);
    let r = press(&mut s, &letters("zhegnshi"));
    assert_eq!(r["candidateList"][0], "zhegnshi");
}

#[test]
fn 回车不学_精确通_不足4字母_分段选过词() {
    let mut s = session();
    crate::inputlog::take_captured();
    for _ in 0..2 {
        press(&mut s, &letters("nihao"));
        key(&mut s, "onKeyDown", VK_RETURN);
        press(&mut s, &letters("zhg"));
        key(&mut s, "onKeyDown", VK_RETURN);
        type_pick(&mut s, &letters("nizhegnshi"), "你");
        assert_eq!(key(&mut s, "onKeyDown", VK_RETURN)["commitString"], "你zhegnshi");
    }
    let logs = crate::inputlog::take_captured();
    let raws: Vec<&Value> = logs.iter().filter(|l| l.get("enter_raw").is_some()).collect();
    assert_eq!(raws.len(), 4, "分段选过词后回车不记 enter_raw");
    assert!(raws.iter().all(|l| l["counted"] == false));
    let r = press(&mut s, &letters("zhegnshi"));
    assert!(!r["candidateList"].as_array().is_some_and(|c| c.iter().any(|t| t == "zhegnshi")));
}

#[test]
fn 回车学会的英文串可用ctrl_shift_数字删除() {
    let mut s = session();
    for _ in 0..2 {
        press(&mut s, &letters("zhegnshi"));
        key(&mut s, "onKeyDown", VK_RETURN);
    }
    let r = press(&mut s, &letters("zhegnshi"));
    assert_eq!(r["candidateList"][0], "zhegnshi");
    let r = pkc(&mut s, "onKeyDown", 49, true, true);
    assert!(!r["candidateList"].as_array().is_some_and(|c| c.iter().any(|t| t == "zhegnshi")));
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p pyime-backend 回车`
Expected: 测试能编译(只用已有辅助函数),第一、二个测试因日志里没有 `enter_raw` 行而断言失败,第三个因候选首位不是 `zhegnshi` 而失败。

- [ ] **Step 3: 实现**

3a. `inputlog.rs` 在 `cancel` 之后加:

```rust
/// 回车原样上屏拼音串(本次组字未分段选过词):`counted` 表示是否计入英文回车学习。
pub fn enter_raw(input: &str, counted: bool) {
    write(json!({ "enter_raw": input, "counted": counted }));
}
```

3b. `session.rs` 的 `on_key_down` 中把 `VK_RETURN => commit = Some(self.flush_raw()),` 改为:

```rust
            VK_RETURN => {
                if self.pending.is_empty() && !self.input.is_empty() {
                    let counted = self.ime.learn_raw(&self.input);
                    crate::inputlog::enter_raw(&self.input, counted);
                }
                commit = Some(self.flush_raw());
            }
```

3c. `ime.rs`:

`use` 区加 `use pyime_engine::english::{parse_compiled, parse_list, English, TIER_CUSTOM};`

`user_path` 之后加:

```rust
/// 英文词表随安装包放在 `server.exe` 同目录。
fn en_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("pyime.en"))
}

/// 用户数据目录 `%LOCALAPPDATA%\PYIME` 下的文件。
fn local_file(name: &str) -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("PYIME").join(name))
}

/// 英文词表:安装目录 `pyime.en` + 用户 `en.txt`(档位 0),再读回车学习计数;缺失的来源跳过。
fn load_english(learned: Option<&PathBuf>) -> English {
    let mut items = match en_path().map(|p| (std::fs::read_to_string(&p), p)) {
        Some((Ok(t), _)) => parse_compiled(&t),
        Some((Err(e), p)) => {
            log::line(&format!("读取英文词表 {p:?} 失败: {e}"));
            Vec::new()
        }
        None => Vec::new(),
    };
    if let Some(t) = local_file("en.txt").and_then(|p| std::fs::read_to_string(p).ok()) {
        items.extend(parse_list(&t).into_iter().map(|(k, w)| (k, w, TIER_CUSTOM)));
    }
    let mut en = English::new(items);
    if let Some(t) = learned.and_then(|p| std::fs::read_to_string(p).ok()) {
        en.import_raw(&t);
    }
    en
}
```

`Ime` 结构在 `user_path` 字段后加 `en_learned_path: Option<PathBuf>,`。

`load` 中 `let user_path = user_path();` 之前加:

```rust
        let en_learned_path = local_file("en_learned.txt");
        engine.set_english(load_english(en_learned_path.as_ref()));
```

并把 `load` 末尾构造改为 `Ime { engine: RefCell::new(engine), user_path, en_learned_path, #[cfg(test)] pressed: RefCell::new(Vec::new()) }`;`with_engine` 中构造加 `en_learned_path: None`。

`forget` 中 `self.save(&engine);` 之后加 `self.save_english(&engine);`。

`save` 之后加:

```rust
    /// 回车原样上屏了 `input`:引擎判定可学则记一次并存盘;返回是否记了。
    pub fn learn_raw(&self, input: &str) -> bool {
        let mut engine = self.engine.borrow_mut();
        let counted = engine.learn_raw(input);
        if counted {
            self.save_english(&engine);
        }
        counted
    }

    fn save_english(&self, engine: &Engine) {
        let (Some(path), Some(en)) = (&self.en_learned_path, engine.english()) else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path, en.export_raw()) {
            log::line(&format!("保存英文学习失败: {e}"));
        }
    }
```

(`learn_raw` 放在 `impl Ime` 的公开方法区、`forget` 之后亦可;`save_english` 为私有。)

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p pyime-backend && cargo clippy -p pyime-backend --all-targets`
Expected: 新增 3 个测试与原有测试(含 `pyime-misses` 的单测,其日志解析遇 `enter_raw` 行应跳过)全部 PASS;clippy 无新警告。

- [ ] **Step 5: 交叉编译检查**

Run: `cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend`
Expected: `Finished`,无错误。

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-backend/src/ime.rs crates/pyime-backend/src/session.rs crates/pyime-backend/src/inputlog.rs crates/pyime-backend/src/session/tests.rs
git commit -m "后端:载入英文词表,回车原样上屏计入英文学习并单独存盘

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 5: 评测链路(回归英文桶、回放 `--en`)、文档与整包验证

**Files:**
- Modify: `crates/pyime-engine/tests/regression.rs`(`use` 区、`CASES` 之后、`回归用例` 函数体)
- Modify: `crates/pyime-backend/src/bin/pyime-replay.rs`
- Modify: `CLAUDE.md`(Project Layout 两条、回归对照命令)

**Interfaces:**
- Consumes:`English::new`、`parse_compiled`、`Engine::set_english/learn_raw`、日志行 `{"enter_raw": …}`、`installer/pyime.en`(Task 3 生成)。
- Produces:`PYIME_EN` 环境变量(回归)、`--en <pyime.en>` 参数(回放)。

- [ ] **Step 1: 生成真实英文词表并存基线名次**(改回归前,未载英文)

Run:
```bash
cargo run --release -q -p pyime-engine --bin pyime-en-build -- installer/pyime.en installer/en_it.txt /tmp/rime-ice/en_dicts/en_ext.dict.yaml /tmp/rime-ice/en_dicts/en.dict.yaml
PYIME_DICT=$PWD/installer/pyime.dict PYIME_GRAM=/tmp/gram/pyime.gram PYIME_RANKS_OUT=/tmp/base.tsv cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture
```
Expected: `通过 36/39`(三条既有失败),`/tmp/base.tsv` 有 39 行。

- [ ] **Step 2: 回归加英文用例与 `PYIME_EN`** —— `regression.rs`:

`use` 区加 `use pyime_engine::english::{parse_compiled, English};`;文件头注释第 2 行之后加一行
`//! 设 `PYIME_EN=installer/pyime.en` 时载入英文词表并追加英文用例。`

`CASES` 之后加:

```rust
/// 载入英文词表(`PYIME_EN`)时追加的用例。
const EN_CASES: &[(&str, &str, usize)] = &[
    ("subagent", "subagent", 1),     // 残缺通:完全匹配的英文排首位
    ("playwright", "Playwright", 1), // 不通:规范大小写
    ("github", "GitHub", 1),
    ("kube", "Kubernetes", 3),       // 残缺通:档位 0 补全插第 3 位
    ("api", "API", 3),               // 精确通:档位 0 短词插第 3 位
    ("change", "嫦娥", 1),           // 精确通:英文不抢中文首位
];
```

函数体中,`PYIME_GRAM_T` 的 `if let` 之后加:

```rust
    let mut cases: Vec<(&str, &str, usize)> = CASES.to_vec();
    if let Ok(p) = std::env::var("PYIME_EN") {
        engine.set_english(English::new(parse_compiled(&std::fs::read_to_string(p).unwrap())));
        cases.extend_from_slice(EN_CASES);
    }
```

并把 `for &(input, want, max) in CASES {` 改为 `for &(input, want, max) in &cases {`,把 `println!("通过 {}/{}", CASES.len() - bad.len(), CASES.len());` 改为 `println!("通过 {}/{}", cases.len() - bad.len(), cases.len());`。

- [ ] **Step 3: 跑回归(载英文)并与基线逐例比对**

Run:
```bash
PYIME_DICT=$PWD/installer/pyime.dict PYIME_GRAM=/tmp/gram/pyime.gram PYIME_EN=$PWD/installer/pyime.en PYIME_RANKS_BASE=/tmp/base.tsv cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture
```
Expected: 出现 `通过 5/5 英文` 桶(`change→嫦娥` 计入"整段词"桶);总数 `通过 42/45`;失败仍只有既有三条;逐例比对中原 39 例无"跌出首位"、无其余名次变化(新增 6 例在基线中不存在,报告为新增或忽略均可)。
若英文用例失败,打印该输入的前 8 个候选分析原因(三态判定/词表档位),不得调低期望名次。

- [ ] **Step 4: 回放支持 `--en` 与 `enter_raw`** —— `pyime-replay.rs`:

文件头用法行改为 `//! 用法:pyime-replay <pyime.dict> <input-log.jsonl> [--gram <pyime.gram>] [--en <pyime.en>] [--fuzzy-penalty <x>] [-v]`;
`use` 区加 `use pyime_engine::english::{parse_compiled, English};`;
`let (mut gram, mut verbose, mut penalty) = (None, false, None);` 改为 `let (mut gram, mut en, mut verbose, mut penalty) = (None, None, false, None);`;
`match` 中加 `"--en" => en = args.next(),`;`Err("用法: …")` 里的用法字符串同步加 `[--en <pyime.en>]`;
`if let Some(p) = penalty { … }` 之后加:

```rust
    if let Some(p) = en {
        let text = String::from_utf8(read(&p)?).map_err(|e| e.to_string())?;
        engine.set_english(English::new(parse_compiled(&text)));
    }
```

循环中 `if let Some(w) = v["forget"].as_str() { … }` 之后加:

```rust
        // 回车原样上屏:按同样条件重建英文回车学习
        if let Some(s) = v["enter_raw"].as_str() {
            engine.learn_raw(s);
            continue;
        }
```

- [ ] **Step 5: 用合成日志验证回放**

Run:
```bash
printf '%s\n' '{"enter_raw":"zhegnshi","counted":true}' '{"enter_raw":"zhegnshi","counted":true}' \
  '{"input":"zhegnshi","context":"","typed":"zhegnshi","text":"zhegnshi","index":0,"top":["zhegnshi"]}' \
  '{"input":"subagent","context":"","typed":"subagent","text":"subagent","index":0,"top":["subagent"]}' > /tmp/en-log.jsonl
cargo run --release -q -p pyime-backend --bin pyime-replay -- installer/pyime.dict /tmp/en-log.jsonl --en installer/pyime.en
cargo run --release -q -p pyime-backend --bin pyime-replay -- installer/pyime.dict /tmp/en-log.jsonl
```
Expected: 带 `--en`:`共 2 条;首选 2 (100.0%) …未出现 0`;不带 `--en`:`subagent` 未出现(`未出现 1`),`zhegnshi` 仍首选(`learn_raw` 在无词表时自建空表)。

- [ ] **Step 6: 真实日志对照**(若 `/tmp/input-log.jsonl` 存在;不存在则跳过并在汇报中注明)

Run:
```bash
cargo run --release -q -p pyime-backend --bin pyime-replay -- installer/pyime.dict /tmp/input-log.jsonl --gram /tmp/gram/pyime.gram
cargo run --release -q -p pyime-backend --bin pyime-replay -- installer/pyime.dict /tmp/input-log.jsonl --gram /tmp/gram/pyime.gram --en installer/pyime.en -v
```
Expected: 带 `--en` 的首选/前 3 不低于不带的(回放有 ±1 抖动);若下降超过抖动,把 `-v` 中涉及英文候选的条目列给用户判断,不擅自调参。

- [ ] **Step 7: 更新 `CLAUDE.md`**

Project Layout 中 `crates/pyime-engine` 一条,把 `` `user` 用户学习)`` 改为 `` `user` 用户学习、`english` 英文词表与回车学习)``,并在 `` `pyime-dict-build` 编译词典`` 后加 ``、`pyime-en-build` 编译英文词表 `pyime.en`(自带 IT 词表 `installer/en_it.txt`)``。
`crates/pyime-backend` 一条末尾改为:`` 词典 `pyime.dict`、英文词表 `pyime.en` 放在 `server.exe` 同目录,用户数据 `%LOCALAPPDATA%\PYIME\user.txt`,用户英文词表 `en.txt`、回车学习计数 `en_learned.txt` 同目录。``
回归对照命令改为:`PYIME_DICT=$PWD/installer/pyime.dict PYIME_GRAM=/tmp/gram/pyime.gram PYIME_EN=$PWD/installer/pyime.en cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture`,并在该条说明末尾加"不设 `PYIME_EN` 则不载英文、不跑英文用例"。

- [ ] **Step 8: 整包验证(仅本机构建,不发布)**

Run: `scripts/package.sh 0.1.10-en-test`
Expected: `cargo test --workspace` 全绿(看到各 crate `test result: ok` 计数行);objdump 检查无 libgcc/libwinpthread;生成 `installer/PYIME-0.1.10-en-test-setup.exe`,makensis 无错误。不 push、不发布;版本号与发布由用户另行决定。

- [ ] **Step 9: Commit**

```bash
git add crates/pyime-engine/tests/regression.rs crates/pyime-backend/src/bin/pyime-replay.rs CLAUDE.md
git commit -m "评测:回归载入英文词表并新增英文用例,回放支持 --en 与回车学习重建

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
