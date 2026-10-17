# 整句逐词学习(仅模糊词) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 选中整句候选时,把句中经模糊匹配得到的完整词按普通选词学一次,并让学过的模糊词在整句分档中按精确算,使"请X""我希望"类输入的学习能推广到新句子。

**Architecture:** 引擎在 Viterbi 回溯时把模糊完整词带进 `Candidate.parts`,`Candidate::lessons` 生成 `(拼音片段, 词, 上文)`;`Engine::learn_lessons/unlearn_lessons` 统一学/撤。后端把 lessons 挂在 `Learned` 上随撤回一起撤,日志格式不变;回放复用同一逻辑。

**Tech Stack:** Rust(workspace:`pyime-engine`、`pyime-backend`),`cargo test` / `cargo clippy`。

**设计文档:** `docs/superpowers/specs/2026-10-17-sentence-word-learning-design.md`

## Global Constraints

- 注释、测试名、commit message 一律简体中文;测试函数沿用中文命名(如 `fn 整句候选带出模糊词()`)。
- 不加新依赖;不改 `user.txt` 格式;不改 `input-log.jsonl` 格式与 retract 语义(四元组、末项为候选自身/分段整句)。
- 只学 `fuzzy > 0 && !partial` 的词;精确词不学。
- 单次写文件 ≤ 约 150 行;改已有文件用精确 Edit。
- 每个 commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 分支:`feat/sentence-word-learning`(已存在,当前所在)。不 push、不打 tag。

---

### Task 1: 引擎——整句候选带出模糊词与 lessons

**Files:**
- Modify: `crates/pyime-engine/src/engine.rs`(`Candidate` 结构 34-46 行、`Engine` impl 内 `learn_fuzzy` 之后、`assemble` 的 `as_cand` 241 行、`sentences` 回溯 289-313 行、`tail` 之后、tests 模块末尾)

**Interfaces:**
- Produces:
  - `pub type Lesson = (String, String, String);` —— `(拼音片段, 词, 上文)`
  - `Candidate.parts: Vec<(usize, usize, usize, String)>` —— `(起, 止, 句中字位置, 词)`
  - `impl Candidate { pub fn lessons(&self, typed: &str, context: &str) -> Vec<Lesson> }`
  - `impl Engine { pub fn learn_lessons(&mut self, lessons: &[Lesson]); pub fn unlearn_lessons(&mut self, lessons: &[Lesson]) }`

- [ ] **Step 1: 写失败测试**(追加到 `engine.rs` tests 模块末尾,`forget_后恢复词库排序` 之后)

```rust
    fn cand(e: &Engine, input: &str, text: &str) -> Candidate {
        e.candidates(input).into_iter().find(|c| c.text == text).unwrap()
    }

    #[test]
    fn 整句候选只带出模糊完整词() {
        let e = engine(&[("qin", "亲", 50_000), ("qing", "请", 20_000), ("si kao", "思考", 10_000)]);
        assert_eq!(cand(&e, "qinsikao", "请思考").parts, [(0, 3, 0, "请".to_string())]);
        assert!(cand(&e, "qinsikao", "亲思考").parts.is_empty());
        assert!(cand(&e, "qinsikao", "请").parts.is_empty());
    }

    #[test]
    fn 句末残缺词不进_parts() {
        let e = engine(&[("qing", "请", 20_000), ("zhi", "之", 10_000)]);
        assert_eq!(cand(&e, "qinz", "请之").lessons("qinz", ""), [("qin".to_string(), "请".to_string(), String::new())]);
    }

    #[test]
    fn lessons_上文含前文与句中前面的词() {
        let e = engine(&[("wo", "我", 5000), ("xi wang", "希望", 5000)]);
        let c = cand(&e, "woxiwan", "我希望");
        assert_eq!(c.parts, [(2, 7, 1, "希望".to_string())]);
        assert_eq!(c.lessons("woxiwan", "说"), [("xiwan".to_string(), "希望".to_string(), "说我".to_string())]);
    }

    #[test]
    fn 全精确整句没有_lessons() {
        let e = engine(&[("wo", "我", 5000), ("kan dao", "看到", 5000), ("ta", "他", 9000), ("ta", "她", 5000), ("le", "了", 5000)]);
        assert!(cand(&e, "wokandaotale", "我看到她了").lessons("wokandaotale", "").is_empty());
    }

    #[test]
    fn learn_lessons_与_unlearn_lessons_对称() {
        let mut e = engine(&[("qin", "亲", 50_000)]);
        let ls = vec![("qin".to_string(), "请".to_string(), "说".to_string())];
        e.learn_lessons(&ls);
        let x = e.user().export();
        assert!(x.contains("qin\t请\t1") && x.contains("@ng\t说\t请\t1"), "{x}");
        e.unlearn_lessons(&ls);
        assert_eq!(e.user().export(), "");
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-engine --lib engine::tests 2>&1 | tail -20`
Expected: 编译失败,报 `no field parts`、`no method named lessons` / `learn_lessons`。

- [ ] **Step 3: 实现**

3a. `Candidate` 结构:在 `pub cuts: Vec<usize>,` 之后加字段;并在结构体之前加类型别名:

```rust
/// 选中整句时顺带学习的一项:(拼音片段, 词, 上文)。
pub type Lesson = (String, String, String);
```

```rust
    /// 整句中经模糊匹配得到的完整词:(起, 止, 句中字位置, 词),起止为输入中的字节位置,
    /// 句中字位置为该词之前的字数。只有整句候选填写,供选中后逐词学习。
    pub parts: Vec<(usize, usize, usize, String)>,
```

3b. 在 `pub struct Candidate {…}` 之后加 impl:

```rust
impl Candidate {
    /// 选中本候选时顺带学习的模糊词。`typed` 为本候选消耗的拼音(`input[..consumed]`),
    /// 上文取 `context` 加句中该词之前文字的末尾 3 个字。
    pub fn lessons(&self, typed: &str, context: &str) -> Vec<Lesson> {
        self.parts
            .iter()
            .map(|(s, e, at, w)| {
                let before: String = self.text.chars().take(*at).collect();
                (typed[*s..*e].to_string(), w.clone(), tail(&format!("{context}{before}")))
            })
            .collect()
    }
}
```

3c. `Engine` impl 中 `learn_fuzzy` 之后加:

```rust
    /// 学一组整句拆出的模糊词(各记一次选词与搭配)。
    pub fn learn_lessons(&mut self, lessons: &[Lesson]) {
        for (t, w, p) in lessons {
            self.user.learn(t, w);
            self.user.learn_next(p, w);
        }
    }

    /// 撤销 `learn_lessons`。
    pub fn unlearn_lessons(&mut self, lessons: &[Lesson]) {
        for (t, w, p) in lessons {
            self.unlearn(t, w, 0, p);
        }
    }
```

3d. `assemble` 中 `as_cand` 补 `parts: Vec::new()`:

```rust
        let as_cand = |w: &&Word| Candidate { text: w.text.clone(), consumed: w.end, fuzzy: w.fuzzy, pairs: w.pairs, cuts: w.cuts.clone(), parts: Vec::new() };
```

3e. `sentences` 回溯:把收集词文字的 `parts` 改为收集 `&Word` 的 `path`,并填 `parts`。将原 289-313 行的循环体改为:

```rust
    for r in 0..best[far].len() {
        let (mut pos, mut rank) = (far, r);
        let (mut path, mut fuzzy, mut pairs, mut partial, mut cuts) = (Vec::new(), 0u8, 0u16, false, Vec::new());
        while pos > 0 {
            let (_, wi, prev_rank, _, _) = &best[pos][rank];
            let w = &words[*wi];
            path.push(w);
            fuzzy = fuzzy.saturating_add(w.fuzzy);
            pairs |= w.pairs;
            cuts.extend(w.cuts.iter().copied());
            if w.start > 0 {
                cuts.push(w.start);
            }
            partial |= w.partial;
            (pos, rank) = (w.start, *prev_rank);
        }
        path.reverse();
        let text: String = path.iter().map(|w| w.text.as_str()).collect();
        if path.len() >= 2 && out.len() < BEAM && !out.iter().any(|c| c.0.text == text) {
            out.push(({
                cuts.sort_unstable();
                Candidate { text, consumed: far, fuzzy, pairs, cuts, parts: fuzzy_parts(&path) }
            }, best[far][r].0, fuzzy == 0 && !partial));
        }
    }
```

3f. 在 `fn tail` 之后加:

```rust
/// 整句路径(按出现顺序)中模糊且完整的词:(起, 止, 句中字位置, 词)。
fn fuzzy_parts(path: &[&Word]) -> Vec<(usize, usize, usize, String)> {
    let mut at = 0;
    let mut out = Vec::new();
    for w in path {
        if w.fuzzy > 0 && !w.partial {
            out.push((w.start, w.end, at, w.text.clone()));
        }
        at += w.text.chars().count();
    }
    out
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-engine 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: 所有 `test result: ok`,无 FAILED(新 5 条 + 原有用例全过)。

- [ ] **Step 5: 全仓编译检查**(后端尚未使用新字段,应能编译)

Run: `cargo build 2>&1 | tail -3 && cargo clippy -p pyime-engine --all-targets 2>&1 | grep -E "^(warning|error)" | head`
Expected: 编译成功;clippy 无新增 warning/error。

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-engine/src/engine.rs
git commit -m "引擎:整句候选带出模糊词,新增 lessons 与 learn_lessons/unlearn_lessons

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 引擎——学过的模糊词在整句分档中按精确算

**Files:**
- Modify: `crates/pyime-engine/src/user.rs`(`knows` 之后加 `chose`)
- Modify: `crates/pyime-engine/src/engine.rs`(`Word` 结构 48-60 行、`grow_partial` 170 行、`grow` 186-189 行、`candidates_after` 221 行、`sentences` 273/277 行与回溯、tests 末尾)

**Interfaces:**
- Consumes: Task 1 的 `Candidate::lessons`、`Engine::learn_lessons`。
- Produces: `impl UserModel { pub fn chose(&self, typed: &str, word: &str) -> bool }`;`Word.learned: bool`(私有)。

- [ ] **Step 1: 写失败测试**(追加到 `engine.rs` tests 末尾)

```rust
    #[test]
    fn 整句学过的模糊词推广到新句子() {
        let mut e = engine(&[("qin", "亲", 50_000), ("qing", "请", 20_000), ("si kao", "思考", 10_000), ("jian cha", "检查", 10_000)]);
        assert_ne!(texts(&e, "qinjiancha")[0], "请检查");
        let ls = cand(&e, "qinsikao", "请思考").lessons("qinsikao", "");
        e.learn_lessons(&ls);
        assert_eq!(texts(&e, "qinjiancha")[0], "请检查");
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-engine --lib 整句学过的模糊词推广到新句子 2>&1 | tail -15`
Expected: FAIL,`assertion left == right` 失败,left 为 `"亲检查"`(请检查仍在模糊档,排在精确档之后)。

- [ ] **Step 3: 实现**

3a. `user.rs`,在 `knows` 之后加:

```rust
    /// 用户是否曾为拼音 `typed` 直接选过 `word`(精确层有记录)。
    pub fn chose(&self, typed: &str, word: &str) -> bool {
        self.pairs.contains_key(&(typed.to_string(), word.to_string()))
    }
```

3b. `engine.rs` 的 `Word` 结构,在 `partial: bool,` 之后加字段,并在结构体后加方法:

```rust
    /// 用户曾为这段拼音直接选过该词:模糊得来也按精确参与整句分档。
    learned: bool,
```

```rust
impl Word {
    /// 整句分档时是否算"不精确":未学过的模糊词,或残缺音节凑出的词。
    fn loose(&self) -> bool {
        (self.fuzzy > 0 && !self.learned) || self.partial
    }
}
```

3c. 三处构造 `Word` 补 `learned`:
- `grow_partial`:`out.push(Word { …, partial: true, learned: false });`
- `grow`:先算 `let learned = self.user.chose(&input[start..end], &e.word);`(放在 `let bonus = …` 旁),再 `out.push(Word { …, partial: false, learned });`
- `candidates_after` 用户短语:`words.push(Word { …, partial: false, learned: false });`

3d. `sentences` 中分束标记改用 `loose()`:

```rust
        if w.start == 0 {
            next.push((w.score - WORD_COST, i, 0, tail(&w.text), w.loose()));
        } else {
            for (r, p) in best[w.start].iter().enumerate() {
                let total = p.0 + w.score - WORD_COST + bigram(&p.3, &w.text);
                next.push((total, i, r, tail(&format!("{}{}", p.3, w.text)), p.4 || w.loose()));
            }
        }
```

3e. 回溯处精确标记改用路径的分束标记,并删去因此不再使用的 `partial` 累积:

```rust
        let (mut path, mut fuzzy, mut pairs, mut cuts) = (Vec::new(), 0u8, 0u16, Vec::new());
```
删除 `partial |= w.partial;` 一行;`out.push` 的第三项由 `fuzzy == 0 && !partial` 改为 `!best[far][r].4`。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-engine 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: 全部 ok;尤其 `整句学过的模糊词推广到新句子`、`选过的模糊词下次排第一`、`整句选过后成为短语并排第一` 均通过。若原有用例失败,先分析是否为本改动的预期影响,不得改弱断言;无法判断时停下汇报。

- [ ] **Step 5: clippy**

Run: `cargo clippy -p pyime-engine --all-targets 2>&1 | grep -E "^(warning|error)" | head`
Expected: 无输出。

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-engine/src/engine.rs crates/pyime-engine/src/user.rs
git commit -m "引擎:学过的模糊词在整句分档中按精确算,使请X类整句能排到精确整句之前

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 后端——选中时学 lessons、撤回连带、单次存盘

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs:127-141`(`learn` / `unlearn`)
- Modify: `crates/pyime-backend/src/session.rs`(51 行 `Learned`、`back_after_commit` 231-235、`finish_commit` 247-252、`select` 311-312 与 321-322)
- Test: `crates/pyime-backend/src/session/tests.rs`(末尾追加)

**Interfaces:**
- Consumes: `pyime_engine::engine::Lesson`、`Candidate::lessons`、`Engine::learn_lessons`、`Engine::unlearn_lessons`(Task 1)。
- Produces:
  - `Ime::learn(&self, typed: &str, text: &str, pairs: u16, prev: &str, lessons: &[Lesson])`
  - `Ime::unlearn(&self, typed: &str, text: &str, pairs: u16, prev: &str, lessons: &[Lesson])`
  - `session.rs`:`type Learned = (String, String, u16, String, Vec<Lesson>);`,`Session::retract(&self, learned: &[Learned])`

- [ ] **Step 1: 写失败测试**(追加到 `session/tests.rs` 末尾)

```rust
const L: i64 = 76;

#[test]
fn 整句里的模糊词随选中被学习_退格删光一并撤回() {
    let mut s = session();
    type_pick(&mut s, &[L, I, W, O], "你我"); // li 经 n=l 模糊得"你"
    let x = s.ime.user_export();
    assert!(x.contains("liwo\t你我\t1") && x.contains("li\t你\t1"), "{x}");
    backs(&mut s, 2);
    let x = s.ime.user_export();
    assert!(!x.contains("liwo\t你我") && !x.contains("li\t你"), "{x}");
}

#[test]
fn 撤回日志仍只记候选自身四元组() {
    let mut s = session();
    crate::inputlog::take_captured();
    type_pick(&mut s, &[L, I, W, O], "你我");
    backs(&mut s, 2);
    let lines = crate::inputlog::take_captured();
    let items = lines.iter().find_map(|l| l["retract"].as_array().cloned()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_array().unwrap().len(), 4);
    assert_eq!(items[0][1], "你我");
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend --lib 整句里的模糊词 2>&1 | tail -15`
Expected: FAIL,断言消息中的导出内容含 `liwo\t你我\t1` 但不含 `li\t你\t1`。(第二条测试此时可能已通过,属正常:它守护日志格式不被本改动破坏。)

- [ ] **Step 3: 实现**

3a. `ime.rs`:顶部 `use` 处加 `use pyime_engine::engine::Lesson;`(若已 `use pyime_engine::engine::{…}` 则并入),把 `learn` / `unlearn` 改为:

```rust
    /// 学习一次选择(连同整句拆出的模糊词)并只存盘一次;存盘失败只记日志。
    /// `pairs` 是该候选用到的模糊规则,`prev` 是其上文,`lessons` 见 `Candidate::lessons`。
    pub fn learn(&self, typed: &str, text: &str, pairs: u16, prev: &str, lessons: &[Lesson]) {
        let mut engine = self.engine.borrow_mut();
        engine.learn(typed, text);
        engine.learn_fuzzy(pairs);
        engine.learn_next(prev, text);
        engine.learn_lessons(lessons);
        self.save(&engine);
    }

    /// 撤销一次 `learn`(含 lessons)并存盘。
    pub fn unlearn(&self, typed: &str, text: &str, pairs: u16, prev: &str, lessons: &[Lesson]) {
        let mut engine = self.engine.borrow_mut();
        engine.unlearn(typed, text, pairs, prev);
        engine.unlearn_lessons(lessons);
        self.save(&engine);
    }
```

3b. `session.rs`:`Learned` 改为(并在文件 `use` 处引入 `pyime_engine::engine::Lesson`):

```rust
/// 一次学习:(键入拼音, 上屏文字, 模糊规则位, 上文, 整句拆出的模糊词)。
type Learned = (String, String, u16, String, Vec<Lesson>);
```

3c. `session.rs` 加方法(放在 `back_after_commit` 之前),替换两处重复的"写日志 + 逐项 unlearn":

```rust
    /// 撤回一次上屏的全部学习。日志只记候选自身的四元组(格式不变),lessons 随之撤销。
    fn retract(&self, learned: &[Learned]) {
        let items: Vec<(String, String, u16, String)> = learned.iter().map(|(t, w, p, c, _)| (t.clone(), w.clone(), *p, c.clone())).collect();
        crate::inputlog::retract(&items);
        for (t, w, pairs, ctx, lessons) in learned {
            self.ime.unlearn(t, w, *pairs, ctx, lessons);
        }
    }
```

`back_after_commit` 中:

```rust
        let Some(l) = self.last.take() else { return };
        self.retract(&l.learned);
        self.context.clear();
```

`finish_commit` 中:

```rust
            if prev.backspaces > 0 && prev.typed == typed && prev_text != text {
                self.retract(&prev.learned);
            }
```

3d. `select` 中学习处:

```rust
        let lessons = c.lessons(&typed, &ctx);
        self.ime.learn(&typed, &c.text, c.pairs, &ctx, &lessons);
        self.learned.push((typed.clone(), c.text.clone(), c.pairs, ctx, lessons));
```

分段整句处:

```rust
                self.ime.learn(&self.typed_all, &text, 0, "", &[]); // 分段选完的整句也学
                self.learned.push((self.typed_all.clone(), text.clone(), 0, String::new(), Vec::new()));
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: 全部 `test result: ok`,无 FAILED(含 `pyime-misses` 等 bin 测试)。

- [ ] **Step 5: clippy**

Run: `cargo clippy --all-targets 2>&1 | grep -E "^(warning|error)" | head`
Expected: 无输出。

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-backend/src/ime.rs crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "后端:选中整句时学其中的模糊词,撤回连带撤销,学习只存盘一次

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 回放同步 + 真实日志验收

**Files:**
- Modify: `crates/pyime-backend/src/bin/pyime-replay.rs`(31-72 行)

**Interfaces:**
- Consumes: `Candidate::lessons`、`Engine::learn_lessons`、`Engine::unlearn_lessons`、`pyime_engine::engine::Lesson`。

回放是命令行工具,无单测;以前后对比真实日志为验证。基线已保存在 `/tmp/replay-base.log`(改动前 `-v` 输出,末行:`共 630 条;首选 506 (80.3%) 前3 576 (91.4%) 前5 607 (96.3%) 未出现 7`;首选在 505–506 间浮动)。

- [ ] **Step 1: 实现**

在 `use` 处加 `use pyime_engine::engine::Lesson;` 与 `use std::collections::HashMap;`。在统计变量那一行之后加:

```rust
    // 每次选词顺带学的 lessons,按 (typed, text, 上文) 记最近一次,供 retract 连带撤销(日志里不含 lessons)
    let mut taught: HashMap<(String, String, String), Vec<Lesson>> = HashMap::new();
```

retract 分支中 `engine.unlearn(t, w, p as u16, c);` 之后加:

```rust
                    if let Some(ls) = taught.remove(&(t.to_string(), w.to_string(), c.to_string())) {
                        engine.unlearn_lessons(&ls);
                    }
```

末尾学习处改为:

```rust
        engine.learn(typed, chosen);
        engine.learn_next(ctx, chosen);
        let lessons = match rank {
            Some(r) => {
                engine.learn_fuzzy(cands[r].pairs);
                cands[r].lessons(&input[..cands[r].consumed], ctx)
            }
            None => Vec::new(),
        };
        engine.learn_lessons(&lessons);
        taught.insert((typed.to_string(), chosen.to_string(), ctx.to_string()), lessons);
```

- [ ] **Step 2: 编译 + clippy**

Run: `cargo build --release -q -p pyime-backend --bin pyime-replay && cargo clippy --all-targets 2>&1 | grep -E "^(warning|error)" | head`
Expected: 编译成功,clippy 无输出。

- [ ] **Step 3: 跑真实日志(3 次,观察浮动)**

Run:
```bash
for i in 1 2 3; do ./target/release/pyime-replay installer/pyime.dict /tmp/input-log.jsonl --gram /tmp/gram/pyime.gram -v > /tmp/replay-new-$i.log 2>&1; tail -1 /tmp/replay-new-$i.log; done
```
Expected: 每次首选 ≥ 505 条(80.2%)、前 3 ≥ 576 条(91.4%)。

- [ ] **Step 4: 逐条对比**

Run: `diff <(sort /tmp/replay-base.log) <(sort /tmp/replay-new-1.log)`
Expected:`<` 侧应多出 `qin… → 请…` 类条目(这些从"名次 ≥4"中消失);`>` 侧为新增或名次变差的项。
对 `>` 侧每一条:说明原因(属本改动影响 / 同分浮动),写进汇报。若出现首选或前 3 低于基线,**停下汇报**,不调参、不改测试。

- [ ] **Step 5: 全量测试**

Run: `cargo test 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: 全部 ok。

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-backend/src/bin/pyime-replay.rs
git commit -m "工具:pyime-replay 同步整句模糊词学习,retract 连带撤销

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

汇报内容:三次回放的末行、diff 中消失与新增的条目及分析。发布(版本号 +1、打包、tag)不在本计划内,需另行征得用户同意。
