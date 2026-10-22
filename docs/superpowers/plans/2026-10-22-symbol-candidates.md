# 符号候选 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 精确整段词候选(`上`、`二`)后附加符号表里的符号(`↑`、`② Ⅱ`),选中符号不学习。

**Architecture:** 引擎新模块 `symbol.rs` 解析符号表;`Engine::candidates_after` 末尾 `place_symbols` 插入
`Source::Symbol` 候选。后端启动时读 `server.exe` 同目录 `symbols.txt`;`Session::select` 对符号跳过学习、清上文。

**Tech Stack:** Rust(workspace:`pyime-engine`、`pyime-backend`),NSIS。

**Spec:** `docs/superpowers/specs/2026-10-22-symbol-candidates-design.md`

## Global Constraints

- 注释、commit message 一律简体中文;commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 不 push、不打 tag、不发版(须先问用户)。
- 每个宿主候选最多附加 2 个符号(表内前 2 个),已在列表中的跳过。
- 宿主只限精确整段词:`consumed == input.len()` 且 `words` 中有 `start == 0 && end == len && fuzzy == 0 && !partial` 的同文字词。
- 符号候选:`consumed = input.len()`,`fuzzy = 0`,`pairs = 0`,`cuts` 复制宿主,`parts` 为空,`source = Source::Symbol`。
- `place_symbols` 必须在 ASCII 改标 `English` 的循环之后调用。
- 不改无关代码;`cargo clippy` 不得新增告警。

---

### Task 1: 符号表解析 `symbol.rs`

**Files:**
- Create: `crates/pyime-engine/src/symbol.rs`
- Modify: `crates/pyime-engine/src/lib.rs`(加 `pub mod symbol;`,按现有顺序放在 `pub mod rime;` 后)

**Interfaces:**
- Produces: `pub struct Symbols`(`#[derive(Debug, Default)]`);`Symbols::parse(text: &str) -> Symbols`;`Symbols::get(&self, word: &str) -> &[String]`(未命中返回空切片)。

- [ ] **Step 1: 写失败测试**(建 `symbol.rs`,先只放测试模块与空壳 `pub struct Symbols;` 以便编译出失败)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 解析注释空行坏行() {
        let s = Symbols::parse("# 注释\n\n上\t↑\n坏行没有制表符\n下\t\n二\t② Ⅱ ⑵\n");
        assert_eq!(s.get("上"), ["↑"]);
        assert_eq!(s.get("二"), ["②", "Ⅱ", "⑵"]);
        assert!(s.get("下").is_empty());
        assert!(s.get("坏行没有制表符").is_empty());
        assert!(s.get("左").is_empty());
    }

    #[test]
    fn 去_bom_与_crlf_重复词以首行为准() {
        let s = Symbols::parse("\u{feff}上\t↑\r\n上\t⇧\r\n");
        assert_eq!(s.get("上"), ["↑"]);
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-engine --lib symbol`
Expected: 编译失败(`parse`/`get` 未定义)。

- [ ] **Step 3: 最小实现**(替换空壳,放在测试模块之上)

```rust
//! 符号表:候选文字精确命中时,在其后附加的符号(`上` → `↑`、`二` → `② Ⅱ`)。
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Symbols {
    map: HashMap<String, Vec<String>>,
}

impl Symbols {
    /// 解析 `词⇥符号1 符号2` 文本:去文件头 BOM,`#` 注释与空行跳过,缺制表符或无符号的行跳过,
    /// 同一词重复出现时以首行为准。
    pub fn parse(text: &str) -> Symbols {
        let mut map = HashMap::new();
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((word, syms)) = line.split_once('\t') else { continue };
            let syms: Vec<String> = syms.split_whitespace().map(str::to_string).collect();
            let word = word.trim();
            if word.is_empty() || syms.is_empty() {
                continue;
            }
            map.entry(word.to_string()).or_insert(syms);
        }
        Symbols { map }
    }

    /// 该词对应的符号(按表内顺序),没有则为空。
    pub fn get(&self, word: &str) -> &[String] {
        self.map.get(word).map(Vec::as_slice).unwrap_or(&[])
    }
}
```

注:`下\t` 经 `trim` 后成 `下`,无制表符,按坏行跳过。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -q -p pyime-engine --lib symbol`
Expected: 2 passed。

- [ ] **Step 5: 用真实符号表冒烟**(确认 `installer/symbols.txt` 可解析、`上`/`二` 命中)

在测试模块加:

```rust
    #[test]
    fn 自带符号表可解析() {
        let s = Symbols::parse(include_str!("../../../installer/symbols.txt"));
        assert_eq!(s.get("上"), ["↑"]);
        assert_eq!(s.get("二"), ["②", "Ⅱ"]);
    }
```

Run: `cargo test -q -p pyime-engine --lib symbol` → 3 passed。

- [ ] **Step 6: 提交**

```bash
git add crates/pyime-engine/src/symbol.rs crates/pyime-engine/src/lib.rs
git commit -m "引擎:符号表解析(词⇥符号,去 BOM、注释、坏行,同词以首行为准)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 引擎插入符号候选

**Files:**
- Modify: `crates/pyime-engine/src/engine.rs`(`use` 段、`Source` 枚举 `:70-77`、`Engine` 结构与 `new` `:162-176`、`candidates_after` `:343-354`、在 `place_english` 之前新增 `place_symbols`;测试模块加用例)
- Modify: `crates/pyime-engine/tests/regression.rs:153-161`(`bucket()` 加分支)

**Interfaces:**
- Consumes: Task 1 的 `Symbols::get`。
- Produces: `Source::Symbol`;`Engine::set_symbols(&mut self, symbols: Symbols)`。

- [ ] **Step 1: 写失败测试**(engine.rs 测试模块,`with_english` 之后)

```rust
    fn with_symbols(entries: &[(&str, &str, u32)], table: &str) -> Engine {
        let mut e = engine(entries);
        e.set_symbols(Symbols::parse(table));
        e
    }

    fn has(t: &[String], s: &str) -> bool {
        t.iter().any(|x| x == s)
    }

    #[test]
    fn 符号紧跟精确整段词() {
        let e = with_symbols(&[("shang", "上", 1000), ("shang", "尚", 500)], "上\t↑\n");
        assert_eq!(texts(&e, "shang"), ["上", "↑", "尚"]);
        assert_eq!(cand(&e, "shang", "↑").source, Source::Symbol);
    }

    #[test]
    fn 每词最多两个符号_按表序() {
        let e = with_symbols(&[("er", "二", 1000), ("er", "而", 500)], "二\t② Ⅱ ⑵\n");
        assert_eq!(texts(&e, "er"), ["二", "②", "Ⅱ", "而"]);
    }

    #[test]
    fn 前缀词不挂符号() {
        let e = with_symbols(&[("shang", "上", 1000), ("ban", "班", 1000), ("shang ban", "上班", 800)], "上\t↑\n");
        let t = texts(&e, "shangban");
        assert!(has(&t, "上") && !has(&t, "↑"), "{t:?}");
    }

    #[test]
    fn 模糊与残缺音节补全的宿主不挂() {
        let e = with_symbols(&[("san", "三", 1000), ("shang", "上", 1000)], "三\t③\n上\t↑\n");
        let t = texts(&e, "san");
        assert!(has(&t, "上") && has(&t, "③") && !has(&t, "↑"), "{t:?}");
        let t = texts(&e, "sh");
        assert!(has(&t, "上") && !has(&t, "↑"), "{t:?}");
    }

    #[test]
    fn 已在列表中的符号不重复() {
        let e = with_symbols(&[("shang", "上", 1000), ("shang", "↑", 500)], "上\t↑\n");
        assert_eq!(texts(&e, "shang").iter().filter(|t| *t == "↑").count(), 1);
    }

    #[test]
    fn 符号沿用宿主切分_ascii_符号仍为符号来源() {
        let e = with_symbols(&[("er shi", "二十", 1000), ("bai fen hao", "百分号", 1000)], "二十\t⑳\n百分号\t%\n");
        assert_eq!(cand(&e, "ershi", "⑳").cuts, cand(&e, "ershi", "二十").cuts);
        assert_eq!(cand(&e, "baifenhao", "%").source, Source::Symbol);
    }
```

测试模块顶部 `use` 不必改(`use super::*;` 会带入 engine.rs 顶部导入的 `Symbols`)。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-engine --lib engine::tests::`
Expected: 编译失败(`set_symbols`、`Source::Symbol` 未定义)。

- [ ] **Step 3: 实现**

3a. 顶部 `use` 段加:`use crate::symbol::Symbols;`

3b. `Source` 枚举末尾加 `Symbol,`,并把枚举上方注释改为
`/// 候选来源:整段词(词典或用户短语吃满输入)、整句(Viterbi 拼装)、前缀词(只吃前一段)、英文(文字全为 ASCII)、符号(附在精确整段词后)。`

3c. `Engine` 结构加字段 `symbols: Symbols,`;`Engine::new` 字面量末尾加 `symbols: Symbols::default()`。在 `set_english` 之后加:

```rust
    /// 载入符号表:精确整段词候选后附加对应符号。
    pub fn set_symbols(&mut self, symbols: Symbols) {
        self.symbols = symbols;
    }
```

3d. `candidates_after` 里,ASCII 改标循环之后、`truncate` 之前加一行:

```rust
        place_symbols(&self.symbols, &words, input.len(), &mut out);
```

3e. 常量区(`EN_EXACT_MIN` 之后)加:

```rust
/// 每个宿主候选后至多附加几个符号。
const SYMBOLS_PER_WORD: usize = 2;
```

3f. `place_english` 之前新增:

```rust
/// 在精确整段词候选后插入符号表里的符号(取表内前几个,已在列表中的跳过);
/// 符号沿用宿主切分。模糊、残缺音节补全、前缀词、只靠整句拼出的候选不挂。
fn place_symbols(symbols: &Symbols, words: &[Word], len: usize, out: &mut Vec<Candidate>) {
    let exact: Vec<&str> = words.iter().filter(|w| w.start == 0 && w.end == len && w.fuzzy == 0 && !w.partial).map(|w| w.text.as_str()).collect();
    let mut i = 0;
    while i < out.len() {
        let c = &out[i];
        i += 1;
        if c.consumed != len || c.source == Source::Symbol || !exact.contains(&c.text.as_str()) {
            continue;
        }
        let cuts = c.cuts.clone();
        let add: Vec<String> = symbols.get(&c.text).iter().take(SYMBOLS_PER_WORD).filter(|s| !out.iter().any(|o| o.text == **s)).cloned().collect();
        for text in add {
            out.insert(i, Candidate { text, consumed: len, fuzzy: 0, pairs: 0, cuts: cuts.clone(), parts: Vec::new(), source: Source::Symbol });
            i += 1;
        }
    }
}
```

3g. `tests/regression.rs` 的 `bucket()` 在 `Source::English => "英文",` 后加 `Source::Symbol => "符号",`。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -q -p pyime-engine`
Expected: 全部通过(含新增 6 例)。若 `模糊与残缺音节补全的宿主不挂` 因 `san`/`sh` 未出"上"而失败,先用
`PYIME_INPUTS` 式打印确认引擎实际候选,只调整测试词频使"上"出现,不得删断言。

- [ ] **Step 5: clippy**

Run: `cargo clippy -q -p pyime-engine --all-targets`
Expected: 无新增告警。

- [ ] **Step 6: 提交**

```bash
git add crates/pyime-engine/src/engine.rs crates/pyime-engine/tests/regression.rs
git commit -m "引擎:精确整段词候选后附加符号(Source::Symbol,每词至多 2 个)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 后端加载符号表、选中符号不学习

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs`(`use` 段;`en_path` 之后加 `symbols_path`;`Ime::load` 里 `set_english` 之后加载)
- Modify: `crates/pyime-backend/src/session.rs:3`(`use`)、`:349-378`(`select`)
- Test: `crates/pyime-backend/src/session/tests.rs`

**Interfaces:**
- Consumes: `Symbols::parse`、`Engine::set_symbols`、`Source::Symbol`(Task 1、2)。

- [ ] **Step 1: 写失败测试**(tests.rs 顶部 `use` 加 `use pyime_engine::symbol::Symbols;`,文件末尾加)

```rust
const S: i64 = 83;
const G: i64 = 71;
const SHANG: [i64; 5] = [S, H, A, N, G];

fn session_sym() -> Session {
    let mut b = DictBuilder::default();
    b.add("shang", "上", 8000);
    b.add("wo", "我", 8000);
    let mut engine = Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default());
    engine.set_symbols(Symbols::parse("上\t↑\n"));
    Session::new(Ime::with_engine(engine))
}

#[test]
fn 选中符号上屏且不学习() {
    let mut s = session_sym();
    let r = type_pick(&mut s, &SHANG, "↑");
    assert_eq!(r["commitString"], "↑");
    assert_eq!(s.ime.user_export(), "");
}

#[test]
fn 分段末段选符号_整句不学_上文不含符号() {
    let mut s = session_sym();
    let r = type_pick(&mut s, &[W, O, S, H, A, N, G], "我");
    let pos = r["candidateList"].as_array().unwrap().iter().position(|c| c == "↑").unwrap();
    assert_eq!(key(&mut s, "onKeyDown", 49 + pos as i64)["commitString"], "我↑");
    type_pick(&mut s, &[W, O], "我"); // 上文若留着"我↑",会记下以 ↑ 为上文的搭配
    let exported = s.ime.user_export();
    assert!(exported.contains("wo\t我\t2"), "{exported}");
    assert!(!exported.contains("woshang") && !exported.contains('↑'), "{exported}");
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-backend 符号`
Expected: 两例 FAIL(用户词表里出现 `shang\t↑`、`woshang\t我↑` 等)。

- [ ] **Step 3: 实现 `select`**

`session.rs:3` 改为 `use pyime_engine::engine::{Candidate, Lesson, Source};`

`select` 中把

```rust
        let lessons = c.lessons(&typed, &ctx);
        self.ime.learn(&typed, &c.text, c.pairs, &ctx, &lessons);
        self.learned.push((typed.clone(), c.text.clone(), c.pairs, ctx, lessons));
```

改为

```rust
        // 符号不学习;它吃满剩余输入,必是本次组词的最后一段
        let symbol = c.source == Source::Symbol;
        if !symbol {
            let lessons = c.lessons(&typed, &ctx);
            self.ime.learn(&typed, &c.text, c.pairs, &ctx, &lessons);
            self.learned.push((typed.clone(), c.text.clone(), c.pairs, ctx, lessons));
        }
```

`if self.segments > 1 {` 改为 `if self.segments > 1 && !symbol {`;
`self.context = tail3(&format!("{}{}", self.context, text));` 改为

```rust
            // 以符号结尾时不留上文,免得记下以符号为上文的搭配
            self.context = if symbol { String::new() } else { tail3(&format!("{}{}", self.context, text)) };
```

- [ ] **Step 4: 实现加载(`ime.rs`)**

`use` 段加 `use pyime_engine::symbol::Symbols;`;`en_path` 之后加:

```rust
/// 符号表随安装包放在 `server.exe` 同目录;缺失则不出符号。
fn symbols_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("symbols.txt"))
}
```

`Ime::load` 里 `engine.set_english(...)` 之后加:

```rust
        if let Some(t) = symbols_path().and_then(|p| read_optional(&p)) {
            engine.set_symbols(Symbols::parse(&t));
        }
```

- [ ] **Step 5: 运行确认通过**

Run: `cargo test -q --workspace`
Expected: 全部通过。

Run: `cargo clippy -q --workspace --all-targets`
Expected: 无新增告警。

Run: `cargo build -q --release --target x86_64-pc-windows-gnu -p pyime-backend`
Expected: 交叉编译成功。

- [ ] **Step 6: 提交**

```bash
git add crates/pyime-backend/src/ime.rs crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "后端:加载 symbols.txt;选中符号不学习、不留上文

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 打包与文档

**Files:**
- Modify: `installer/pyime.nsi:56`(`File "pyime.en"` 之后加 `File "symbols.txt"`)
- Modify: `CLAUDE.md` Project Layout 中 `pyime-backend` 一条:把"词典 `pyime.dict`、英文词表 `pyime.en` 放在 `server.exe` 同目录"改为"词典 `pyime.dict`、英文词表 `pyime.en`、符号表 `symbols.txt`(自带 `installer/symbols.txt`)放在 `server.exe` 同目录";`pyime-engine` 一条的模块列表在 `english` 后加 "`symbol` 符号表(精确整段词后附加符号)"。

- [ ] **Step 1: 改 `pyime.nsi`、`CLAUDE.md`**(如上)

- [ ] **Step 2: 验证安装脚本能编译**

Run: `cp target/x86_64-pc-windows-gnu/release/pyime-backend.exe installer/server.exe && makensis -V2 -DVERSION=0.0.0-symcheck installer/pyime.nsi`
Expected: 生成 `installer/PYIME-0.0.0-symcheck-setup.exe`,无错误(`*.exe` 已被 git 忽略)。

- [ ] **Step 3: 提交**

```bash
git add installer/pyime.nsi CLAUDE.md
git commit -m "打包:安装 symbols.txt;文档补符号表

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 4: 实机验证(交用户)**

在 Windows 11 上装测试包:`shang` 出 `上 ↑`、`er` 出 `二 ② Ⅱ`(视"二"名次);候选窗里 `↑ ② Ⅱ ⑳ ℃ ✓ ★ ♥` 等是否显示为方框;选 `↑` 后 `%LOCALAPPDATA%\PYIME\user.txt` 无新增。
