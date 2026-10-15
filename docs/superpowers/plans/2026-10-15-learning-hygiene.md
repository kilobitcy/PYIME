# 学习防污染 + lue/nue 召回修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 上屏后退格删光即撤销学习、Ctrl+Shift+数字删除个人学习、`lue/nue` 与 `lve/nve` 等价。

**Architecture:** 引擎(`pyime-engine`)加 `pinyin::canonical` 与 `UserModel/Engine::forget`;后端(`pyime-backend`)
在 `Session` 的放行按键处理里计退格、回退上文、标点上屏保留跟踪,并接管 Ctrl+Shift+数字;
`inputlog`/`pyime-replay` 记录与回放 `forget`。

**Tech Stack:** Rust(workspace),`cargo test`;真实词典 `installer/pyime.dict`(rime-ice 编译)。

**Spec:** `docs/superpowers/specs/2026-10-15-learning-hygiene-design.md`

## Global Constraints

- 注释、commit message 一律简体中文;commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 分支 `feat/learning-hygiene`;不 push、不打 tag、不发布(须先问用户)。
- 版本 0.1.8(`crates/pyime-engine/Cargo.toml`、`crates/pyime-backend/Cargo.toml`)。
- 不削弱既有测试;行为变更导致的既有测试改写须在 commit message 说明。
- 真实词典回归基线(改动前,无 gram):`regression` 29/32 通过,已知失败
  `zaizuijia`、`fengzhong`、`jintianxingqijia`,不属本计划;新增用例必须通过,且不得新增失败。
- 用户数据 `/tmp/user.txt`、`/tmp/input-log.jsonl` 仅本机使用,不得入库。
- 全量测试命令:`cargo test` 输出中每个 `test result:` 均为 `ok`。

---

### Task 1: lue/nue 规范化(pinyin/fuzzy/rime)

**Files:**
- Modify: `crates/pyime-engine/src/pinyin.rs`(加 `canonical` 与测试)
- Modify: `crates/pyime-engine/src/fuzzy.rs:57-92`(`variants`、`partial` 与测试)
- Modify: `crates/pyime-engine/src/rime.rs:8-23`(`parse_line` 与测试)
- Modify: `crates/pyime-engine/src/engine.rs`(`mod tests` 末尾加测试)
- Modify: `crates/pyime-engine/tests/regression.rs`(加用例)

**Interfaces:**
- Produces: `pub fn canonical(s: &str) -> &str`(`pyime_engine::pinyin`)。

- [ ] **Step 1: 写失败测试**

`pinyin.rs` 的 `mod tests` 末尾加:
```rust
    #[test]
    fn lue_nue_规范为_lve_nve() {
        assert_eq!(canonical("lue"), "lve");
        assert_eq!(canonical("nue"), "nve");
        assert_eq!(canonical("jue"), "jue");
    }
```
`fuzzy.rs` 的 `mod tests` 末尾加:
```rust
    #[test]
    fn lue_nue_按规范写法展开_残缺前缀也映射() {
        assert_eq!(variants("lue", &FuzzyConfig::none())[0], ("lve".to_string(), 0, 0));
        assert!(variants("nue", &FuzzyConfig::default()).contains(&("lve".to_string(), 1, 1 << 3)));
        let got = partial("lu", &FuzzyConfig::none());
        assert!(got.contains(&("lve", 0, 0)) && !got.iter().any(|(t, _, _)| *t == "lue"));
    }
```
`rime.rs` 的 `mod tests` 末尾加:
```rust
    #[test]
    fn 词库里的_lue_键规范为_lve() {
        assert_eq!(parse_line("略\tlue\t100"), Some(("lve".into(), "略", 100)));
    }
```
`engine.rs` 的 `mod tests` 末尾加:
```rust
    #[test]
    fn lue_与_lve_等价_残缺前缀也能召回() {
        let e = engine(&[("ce lve", "策略", 1000), ("ce", "测", 500), ("lu", "路", 500)]);
        assert_eq!(texts(&e, "celue")[0], "策略");
        assert!(texts(&e, "chelue").contains(&"策略".to_string()));
        assert!(texts(&e, "celu").contains(&"策略".to_string()));
    }
```

- [ ] **Step 2: 确认失败**

Run: `cargo test -p pyime-engine 2>&1 | grep -E "test result|error\[|FAILED"`
Expected: 编译失败 `cannot find function canonical`。

- [ ] **Step 3: 实现**

`pinyin.rs` 在 `is_syllable` 之后加:
```rust
/// 统一 ü 的写法:`lue`/`nue` 与 `lve`/`nve` 同音,词库(rime-ice)用后者,输入两种都接受。
pub fn canonical(s: &str) -> &str {
    match s {
        "lue" => "lve",
        "nue" => "nve",
        _ => s,
    }
}
```
`fuzzy.rs`:`use crate::pinyin::{all, canonical, is_syllable};`;`variants` 改为:
```rust
/// 返回 `(词典音节, 模糊次数, 用到的规则位掩码)`;第一项恒为规范化(`canonical`)后的精确写法 `(s, 0, 0)`,其余只含合法音节。
pub fn variants(s: &str, cfg: &FuzzyConfig) -> Vec<(String, u8, u16)> {
    let s = canonical(s);
    let (ini, fin) = split(s);
```
(函数其余不变。)`partial` 的循环改为:
```rust
    for t in all() {
        let t_ini = split(t).0;
        if let Some(&(_, _, c, m)) = starts.iter().find(|(pre, i, _, _)| t.starts_with(pre.as_str()) && t_ini == *i) {
            let t = canonical(t);
            match out.iter_mut().find(|(x, _, _)| *x == t) {
                Some(e) if e.1 > c => *e = (t, c, m),
                Some(_) => {}
                None => out.push((t, c, m)),
            }
        }
    }
```
`rime.rs`:`use crate::pinyin::canonical;`,`parse_line` 中:
```rust
    let syllables: Vec<&str> = pinyin.split_whitespace().map(canonical).collect();
```

- [ ] **Step 4: 加真实词典回归用例**

`tests/regression.rs` 的 `CASES` 末尾加:
```rust
    ("celue", "策略", 1),       // lue 写法:词库键为 lve
    ("chelue", "策略", 3),      // 用户真实例子:c=ch 模糊 + lue
```
(残缺前缀 `celu` 由 Step 1 的引擎单测覆盖,真实词典名次不设门槛。)

- [ ] **Step 5: 确认通过**

Run: `cargo test -p pyime-engine 2>&1 | grep -E "test result|FAILED"` → 全部 `ok`。
Run: `PYIME_DICT=$PWD/installer/pyime.dict cargo test -q -p pyime-engine --release --test regression -- --ignored 2>&1 | grep -E "通过|→"`
Expected: `通过 31/34`,失败行只有基线的三条。

- [ ] **Step 6: Commit**
```bash
git add crates/pyime-engine
git commit -m "引擎:lue/nue 规范为 lve/nve(切分展开、残缺前缀、词库导入三处共用)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 引擎 forget + 回放支持

**Files:**
- Modify: `crates/pyime-engine/src/user.rs`(`forget` 与测试)
- Modify: `crates/pyime-engine/src/engine.rs:114-136`(`Engine::forget` 与测试)
- Modify: `crates/pyime-backend/src/bin/pyime-replay.rs:33-42`

**Interfaces:**
- Produces: `UserModel::forget(&mut self, word: &str)`、`Engine::forget(&mut self, text: &str)`。
- 日志行格式(Task 4 写入):`{"ts":…, "forget": "<词>", "input": "<拼音>", "noop": <bool>}`。

- [ ] **Step 1: 写失败测试**

`user.rs` 的 `mod tests` 末尾加:
```rust
    #[test]
    fn forget_删除该词全部学习_不影响其他词() {
        let mut m = UserModel::default();
        m.learn("qinjixu", "亲继续");
        m.learn("qjx", "亲继续");
        m.learn("qin", "请");
        m.learn_next("是", "亲继续");
        m.learn_next("是", "请");
        m.forget("亲继续");
        assert_eq!(m.bonus("qinjixu", "亲继续"), 0.0);
        assert_eq!(m.bonus("qjx", "亲继续"), 0.0);
        assert!(!m.knows("亲继续"));
        assert_eq!(m.collocation("是", "亲继续"), 0.0);
        assert!(m.bonus("qin", "请") > 0.0 && m.collocation("是", "请") > 0.0);
    }
```
`engine.rs` 的 `mod tests` 末尾加:
```rust
    #[test]
    fn forget_后恢复词库排序() {
        let mut e = engine(&[("ni", "你", 1000), ("ni", "泥", 10)]);
        e.learn("ni", "泥");
        assert_eq!(texts(&e, "ni")[0], "泥");
        e.forget("泥");
        assert_eq!(texts(&e, "ni"), ["你", "泥"]);
    }
```

- [ ] **Step 2: 确认失败**

Run: `cargo test -p pyime-engine 2>&1 | grep -E "error\[|test result"`
Expected: 编译失败 `no method named forget`。

- [ ] **Step 3: 实现**

`user.rs` 在 `unlearn_next` 之后加:
```rust
    /// 删除 `word` 的全部个人学习:各拼音下的精确层、词层,以及以它为后继的搭配。词库不受影响。
    pub fn forget(&mut self, word: &str) {
        self.pairs.retain(|(_, w), _| w != word);
        self.words.remove(word);
        self.next.retain(|(_, w), _| w != word);
    }
```
`engine.rs` 在 `unlearn` 之后加:
```rust
    /// 删除词 `text` 的全部个人学习(词库不受影响)。
    pub fn forget(&mut self, text: &str) {
        self.user.forget(text);
    }
```
`pyime-replay.rs`:在 `retract` 分支(`continue;` 后的 `}`)之后、`let (Some(input), Some(typed), Some(chosen))` 之前加:
```rust
        if let Some(w) = v["forget"].as_str() {
            engine.forget(w);
            continue;
        }
```

- [ ] **Step 4: 确认通过**

Run: `cargo test -p pyime-engine 2>&1 | grep -E "test result|FAILED"` → 全部 `ok`。

- [ ] **Step 5: 回放手工验证**

```bash
cat > /tmp/replay-forget.jsonl <<'J'
{"context":"","index":1,"input":"qinjixu","text":"亲继续","top":[],"ts":1,"typed":"qinjixu"}
{"ts":2,"forget":"亲继续","input":"qinjixu","noop":false}
{"context":"","index":0,"input":"qinjixu","text":"请继续","top":[],"ts":3,"typed":"qinjixu"}
J
cargo run -q --release -p pyime-backend --bin pyime-replay -- installer/pyime.dict /tmp/replay-forget.jsonl
```
Expected: `共 2 条;首选 1 (50.0%) …`(第 1 条"亲继续"非首选;forget 后"请继续"回到首选)。
对照:删掉第 2 行再跑,应为 `首选 0`。

- [ ] **Step 6: Commit**
```bash
git add crates/pyime-engine crates/pyime-backend/src/bin/pyime-replay.rs
git commit -m "引擎:删除某词的全部个人学习(forget);回放识别 forget 行

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 退格删光即撤销学习(后端 A)

**Files:**
- Modify: `crates/pyime-backend/src/session.rs`(`Last`:50-57;`handle`:134-161;`note_passthrough`:180-201;
  `finish_commit`:203-214;`punctuate`:339-365)
- Test: `crates/pyime-backend/src/session/tests.rs`

**Interfaces:**
- Produces: `fn note_passthrough(&mut self, key: i64, shift_down: bool, count_back: bool)`(Task 4 沿用 `handle` 中的调用)。

**行为变更说明(须写进 commit message):** 既有测试 `退格后重选同一个词_不撤回` 期望 `ni 你 2`;
新规则下单字"你"退格 1 次即删光并撤回,重选后为 `ni 你 1`,测试改名并改期望。
`退格后又按了别的键_不再视为撤销` 原用单字,退格 1 次即已撤回,改为两字词以保留原测试意图。

- [ ] **Step 1: 改写受影响的既有测试并加新测试**

`tests.rs` 在 `press` 之后加辅助函数:
```rust
/// 键入拼音后按数字键选中文字为 `word` 的候选。
fn type_pick(s: &mut Session, codes: &[i64], word: &str) -> Value {
    let r = press(s, codes);
    let pos = r["candidateList"].as_array().unwrap().iter().position(|c| c == word).unwrap();
    key(s, "onKeyDown", 49 + pos as i64)
}

fn backs(s: &mut Session, n: usize) {
    for _ in 0..n {
        key(s, "filterKeyDown", VK_BACK);
    }
}
```
把 `退格后重选同一个词_不撤回` 整个替换为:
```rust
#[test]
fn 退格删光后重选同一个词_计数不累加() {
    let mut s = session();
    press(&mut s, &[N, I]);
    press(&mut s, &[32]);
    backs(&mut s, 1); // 单字"你"已删光,学习撤回
    press(&mut s, &[N, I]);
    press(&mut s, &[32]);
    assert!(s.ime.user_export().contains("ni\t你\t1"));
}
```
把 `退格后又按了别的键_不再视为撤销` 整个替换为:
```rust
#[test]
fn 退格中途按了别的键_不再视为撤销() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    backs(&mut s, 1);
    key(&mut s, "filterKeyDown", 37); // 左方向键
    backs(&mut s, 1);
    assert!(s.ime.user_export().contains("nihao\t你好\t1"));
}
```
在其后加新测试:
```rust
#[test]
fn 退格删光两字词即撤回_只删一字不撤回() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    backs(&mut s, 1);
    assert!(s.ime.user_export().contains("nihao\t你好\t1"));
    backs(&mut s, 1);
    assert!(!s.ime.user_export().contains("nihao\t你好"), "{}", s.ime.user_export());
}

#[test]
fn 同一次退格的_onkeydown_不重复计数() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    key(&mut s, "filterKeyDown", VK_BACK);
    key(&mut s, "onKeyDown", VK_BACK);
    assert!(s.ime.user_export().contains("nihao\t你好\t1"));
}

#[test]
fn 带标点上屏后连标点一起删光才撤回() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(commit(&pk(&mut s, 188, false)), "你\u{FF0C}");
    backs(&mut s, 1); // 只删了逗号
    assert!(s.ime.user_export().contains("ni\t你\t1"));
    backs(&mut s, 1);
    assert!(!s.ime.user_export().contains("ni\t你"));
}

#[test]
fn 成对符号上屏后不跟踪退格() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(commit(&pk(&mut s, 188, true)), "你\u{300A}\u{300B}");
    backs(&mut s, 3);
    assert!(s.ime.user_export().contains("ni\t你\t1"));
}

#[test]
fn 删光后重打不学到被删内容的搭配() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    backs(&mut s, 2);
    type_pick(&mut s, &[N, I], "你");
    assert!(!s.ime.user_export().contains("@ng"), "{}", s.ime.user_export());
}
```

- [ ] **Step 2: 确认失败**

Run: `cargo test -p pyime-backend 2>&1 | grep -E "^test .*FAILED|test result"`
Expected: `退格删光后重选同一个词_计数不累加`、`退格删光两字词即撤回_只删一字不撤回`、
`带标点上屏后连标点一起删光才撤回`、`删光后重打不学到被删内容的搭配` FAILED;其余通过。

- [ ] **Step 3: 实现**

`session.rs` 的 `Last` 改为:
```rust
/// 刚上屏的一次组词:记下它做过的学习。随后放行的退格删光它(含随之上屏的标点),
/// 或删过之后对同一串拼音改选了别的词,就把这些学习撤回。
struct Last {
    learned: Vec<Learned>,
    typed: String,
    /// 上屏的全部文字(组词结果 + 随后一起上屏的标点)。
    text: String,
    /// `text` 末尾的标点字符数;标点不在上文 `context` 里,删它时不回退上文。
    punct: usize,
    /// 上屏后放行的退格次数。
    backspaces: usize,
}
```
`handle` 中把
```rust
                if !wants {
                    self.note_passthrough(key, shift_down);
                }
```
改为
```rust
                if !wants {
                    // 同一次按键可能先后以 filterKeyDown、onKeyDown 到达,退格只在前者计数
                    self.note_passthrough(key, shift_down, m == "filterKeyDown");
                }
```
`note_passthrough` 签名改为 `fn note_passthrough(&mut self, key: i64, shift_down: bool, count_back: bool)`,
文档注释末尾补"`count_back` 为假时退格不计数",其中
```rust
        if key == VK_BACK {
            if let Some(l) = &mut self.last {
                l.backspaced = true;
            }
        } else if
```
改为
```rust
        if key == VK_BACK {
            if count_back {
                self.back_after_commit();
            }
        } else if
```
在 `note_passthrough` 之后加:
```rust
    /// 放行的一次退格:删到组词文字时同步回退上文;删光整次上屏则撤回其全部学习并清空上文。
    fn back_after_commit(&mut self) {
        let Some(l) = &mut self.last else { return };
        l.backspaces += 1;
        if l.backspaces > l.punct {
            self.context.pop();
        }
        if l.backspaces < l.text.chars().count() {
            return;
        }
        let Some(l) = self.last.take() else { return };
        crate::inputlog::retract(&l.learned);
        for (t, w, pairs, ctx) in &l.learned {
            self.ime.unlearn(t, w, *pairs, ctx);
        }
        self.context.clear();
    }
```
`finish_commit` 中 `prev.backspaced &&` 改为 `prev.backspaces > 0 &&`;末行构造改为:
```rust
        self.last = Some(Last { learned: std::mem::take(&mut self.learned), typed, text: text.to_string(), punct: 0, backspaces: 0 });
```
`punctuate` 改为(`match m { … }` 分支体保持原样,只改首尾):
```rust
    /// 输出标点(组字时先上屏高亮候选);成对符号注入方向键让光标回到中间或跳过右符号。
    /// 同时上屏了组词时继续跟踪退格(标点计入删光阈值);成对符号会移动光标,不再跟踪。
    fn punctuate(&mut self, m: Mapped, was_digit: bool) -> Option<String> {
        let mut out = self.commit_all();
        let committed = out.chars().count();
        let keep = committed > 0 && !matches!(m, Mapped::Open { .. });
        match m {
            // …原有各分支不变…
        }
        if !keep {
            self.last = None;
        } else if let Some(l) = &mut self.last {
            l.punct = out.chars().count() - committed;
            l.text = out.clone();
        }
        Some(out)
    }
```

- [ ] **Step 4: 确认通过**

Run: `cargo test -p pyime-backend 2>&1 | grep -E "FAILED|test result"` → 全部 `ok`。
Run: `cargo clippy -p pyime-backend 2>&1 | grep -E "^warning|^error" | head` → 无新增告警。

- [ ] **Step 5: Commit**
```bash
git add crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "后端:上屏后退格删光即撤回学习,标点上屏也跟踪,上文同步回退

退格只在 filterKeyDown 计数。行为变更:单字删光即撤回,
既有测试「退格后重选同一个词」期望由 2 改为 1 并改名;
「退格后又按了别的键」改用两字词以保留原意图。

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Ctrl+Shift+数字 删除学习(后端 D)

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs:136-141`(加 `forget`)
- Modify: `crates/pyime-backend/src/inputlog.rs`(加 `forget`)
- Modify: `crates/pyime-backend/src/session.rs`(`wants_key`:110-124;`handle`;`on_key_down`:367-395;新增 `forget`)
- Test: `crates/pyime-backend/src/session/tests.rs`

**Interfaces:**
- Consumes: `Engine::forget(&mut self, text: &str)`(Task 2);`note_passthrough(key, shift_down, count_back)`(Task 3)。
- Produces: `Ime::forget(&self, text: &str)`、`inputlog::forget(input: &str, text: &str, noop: bool)`。

- [ ] **Step 1: 写失败测试**

`tests.rs` 在 `pk` 之后加:
```rust
fn pkc(s: &mut Session, method: &str, code: i64, shift: bool, ctrl: bool) -> Value {
    let mut ks = vec![0; 256];
    ks[16] = if shift { 0x80 } else { 0 };
    ks[17] = if ctrl { 0x80 } else { 0 };
    s.handle(&json!({"method": method, "seqNum": 1, "keyCode": code, "keyStates": ks}))
}

#[test]
fn 组字中ctrl_shift_数字删除该候选的学习() {
    let mut s = session();
    type_pick(&mut s, &[N, I], "泥");
    let r = press(&mut s, &[N, I]);
    assert_eq!(r["candidateList"][0], "泥");
    let r = pkc(&mut s, "onKeyDown", 49, true, true);
    assert_eq!(r["return"], true);
    assert_eq!(commit(&r), "");
    assert!(s.composing());
    assert_eq!(r["candidateList"][0], "你");
    assert!(!s.ime.user_export().contains("泥"), "{}", s.ime.user_export());
}

#[test]
fn 非组字时ctrl_shift_数字放行() {
    let mut s = session();
    assert_eq!(pkc(&mut s, "filterKeyDown", 49, true, true)["return"], false);
}

#[test]
fn 组字中ctrl_数字不选词() {
    let mut s = session();
    press(&mut s, &[N, I]);
    let r = pkc(&mut s, "onKeyDown", 50, false, true);
    assert_eq!(r["return"], false);
    assert!(s.composing());
    assert!(!s.ime.user_export().contains("ni\t"));
}
```

- [ ] **Step 2: 确认失败**

Run: `cargo test -p pyime-backend 2>&1 | grep -E "^test .*FAILED|test result"`
Expected: `组字中ctrl_shift_数字删除该候选的学习`、`组字中ctrl_数字不选词` FAILED(现状会当作选词)。

- [ ] **Step 3: 实现**

`ime.rs` 在 `unlearn` 之后加:
```rust
    /// 删除词 `text` 的全部个人学习并存盘。
    pub fn forget(&self, text: &str) {
        let mut engine = self.engine.borrow_mut();
        engine.forget(text);
        self.save(&engine);
    }
```
`inputlog.rs` 末尾加:
```rust
/// 用户删除了某候选的个人学习:`input` 为当时的拼音串,`noop` 表示删除后候选顺序没有变化。
pub fn forget(input: &str, text: &str, noop: bool) {
    let Some(file) = FILE.get() else { return };
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let line = json!({ "ts": ms, "forget": text, "input": input, "noop": noop });
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(f, "{line}");
    }
}
```
`session.rs`:
`wants_key` 签名改为 `fn wants_key(&self, key: i64, shift_down: bool, ctrl_down: bool) -> bool`,
文档注释补"Ctrl+数字不选词,Ctrl+Shift+数字删除学习",其中
```rust
        ) || digit_index(key).is_some_and(|i| i < self.page_items().len());
```
改为
```rust
        ) || digit_index(key).is_some_and(|i| i < self.page_items().len() && (!ctrl_down || shift_down));
```
`handle` 中 `let shift_down = …;` 之后加
```rust
        let ctrl_down = req["keyStates"][17].as_u64().unwrap_or(0) & 0x80 != 0;
```
并改两处调用:`self.wants_key(key, shift_down, ctrl_down)`、`self.on_key_down(key, mapped, ctrl_down, &mut resp)`。
`on_key_down` 签名改为 `fn on_key_down(&mut self, key: i64, mapped: Option<Mapped>, ctrl_down: bool, resp: &mut Value)`,
在 `_ if digit_index(key).is_some() => commit = self.select(…)` 分支**之前**加:
```rust
            _ if ctrl_down && digit_index(key).is_some() => self.forget(digit_index(key).unwrap()),
```
在 `select` 之前加:
```rust
    /// 删除页内第 `idx` 个候选的个人学习,保持输入并刷新候选(页码仍有效则留在当前页)。
    fn forget(&mut self, idx: usize) {
        let Some(text) = self.page_items().get(idx).map(|c| c.text.clone()) else { return };
        let before: Vec<String> = self.cands.iter().map(|c| c.text.clone()).collect();
        self.ime.forget(&text);
        let page = self.page;
        self.refresh();
        self.page = page.min(self.page_count().saturating_sub(1));
        let noop = self.cands.iter().map(|c| &c.text).eq(before.iter());
        crate::inputlog::forget(&self.input, &text, noop);
    }
```

- [ ] **Step 4: 确认通过**

Run: `cargo test 2>&1 | grep -E "FAILED|test result"` → 全部 `ok`。
Run: `cargo clippy 2>&1 | grep -E "^warning|^error" | head` → 无新增告警。

- [ ] **Step 5: Commit**
```bash
git add crates/pyime-backend
git commit -m "后端:组字中 Ctrl+Shift+数字删除候选的个人学习,Ctrl+数字不再选词

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: 版本 0.1.8 + 用户真实数据离线验证

**Files:**
- Modify: `crates/pyime-engine/Cargo.toml`、`crates/pyime-backend/Cargo.toml`(`version = "0.1.7"` → `"0.1.8"`)
- 仓库外:`/tmp/pyrepro`(本机复现工具,不入库)

- [ ] **Step 1: 改版本号**

两个 `Cargo.toml` 的 `version = "0.1.7"` 改为 `version = "0.1.8"`,然后 `cargo build -q` 更新 `Cargo.lock`。

- [ ] **Step 2: 全量测试 + 回归**

Run: `cargo test 2>&1 | grep -E "FAILED|test result"` → 全部 `ok`。
Run: `PYIME_DICT=$PWD/installer/pyime.dict cargo test -q -p pyime-engine --release --test regression -- --ignored 2>&1 | grep -E "通过|→"`
Expected: `通过 31/34`,失败仅基线三条(`zaizuijia`、`fengzhong`、`jintianxingqijia`)。

- [ ] **Step 3: 用用户 user.txt 验证 forget 效果**

把 `/tmp/pyrepro/src/main.rs` 整体替换为(第 4 个参数可选,逗号分隔要 forget 的词):
```rust
use pyime_engine::{dict::Dict, engine::Engine, fuzzy::FuzzyConfig, user::UserModel};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut e = Engine::new(Dict::from_bytes(std::fs::read(&a[1]).unwrap()).unwrap(), FuzzyConfig::default());
    if a[2] != "-" {
        e.set_user(UserModel::import(&std::fs::read_to_string(&a[2]).unwrap()));
    }
    for w in a.get(4).map(|s| s.split(',').collect::<Vec<_>>()).unwrap_or_default() {
        e.forget(w);
    }
    for inp in a[3].split(',') {
        let c = e.candidates(inp);
        let s: Vec<String> = c.iter().take(6).map(|c| format!("{}{}", c.text, if c.fuzzy > 0 { "~" } else { "" })).collect();
        println!("{inp:12} {}", s.join(" "));
    }
}
```
Run:
```bash
cd /tmp/pyrepro && cargo build -q --release && D=/data/bak20250527/PYIME/installer/pyime.dict
./target/release/pyrepro $D /tmp/user.txt qinjixu,gengsui,chelue
./target/release/pyrepro $D /tmp/user.txt qinjixu,gengsui,chelue 亲继续,更随
```
Expected:第一行输出 `qinjixu` 首位"亲继续"(复现污染)、`chelue` 前 3 含"策略"(Task 1 生效);
第二次 `qinjixu` 首位"请继续"、`gengsui` 首位"跟随~"。

- [ ] **Step 4: Commit**
```bash
git add crates/pyime-engine/Cargo.toml crates/pyime-backend/Cargo.toml Cargo.lock
git commit -m "版本 0.1.8

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: 交还用户(不自行发布)**

汇报:各任务测试计数、回归结果、离线验证输出;release 说明草稿须含
"退格删光撤销学习 / Ctrl+Shift+数字删除学习词 / lue·nue 修复",并写明
**已学进去的错词需用 Ctrl+Shift+数字 手动删除**。打包(`scripts/package.sh 0.1.8`)、push、tag、发布等待用户确认。
