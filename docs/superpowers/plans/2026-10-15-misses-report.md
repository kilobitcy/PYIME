# 日志增强 + 错例报告 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `input-log.jsonl` 每行带版本号并记录"放弃输入";新工具 `pyime-misses` 从日志直接生成回归用例与疑似清单。

**Architecture:** `inputlog.rs` 四个写函数合并为私有 `write`(补 `ts`/`v`,测试下捕获);`Session` 记录本次组字最长拼音 `peak`,Esc/退格删光时写 `cancel`。`pyime-misses` 是自包含的 bin,只读日志、不加载词典,纯函数 `analyze` + `render`。

**Tech Stack:** Rust 1.95,serde_json(已有依赖),无新依赖。

Spec:`docs/superpowers/specs/2026-10-15-misses-report-design.md`

## Global Constraints

- 回复、注释、commit message 一律简体中文;每个 commit 末尾 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 不加新依赖;只改任务列出的文件;不得削弱或删除已有测试断言。
- clippy 未安装:以 `cargo build --all-targets` 0 warning 代替 lint;`cargo test` 全工作区全绿。
- 版本字段 `"v": env!("CARGO_PKG_VERSION")`(backend crate 版本)。
- `cancel` 行格式 `{"ts", "v", "cancel": 拼音, "top": 前 5 候选}`。
- 纠错时间窗 60 秒(`RETYPE_MS = 60_000`),放弃配对时间窗 30 秒(`CANCEL_MS = 30_000`)。
- 用例输出格式 `("拼音", "期望", 1), // ×次数 注释`。
- 用户数据 `/tmp/user.txt`、`/tmp/input-log.jsonl`、`/tmp/log.txt` 只读,不得写入、移动或提交。

## 文件

- Modify: `crates/pyime-backend/src/inputlog.rs` — 合并写入、加 `v`、新增 `cancel`、测试捕获。
- Modify: `crates/pyime-backend/src/session.rs` — `peak` 字段与 `log_cancel`。
- Modify: `crates/pyime-backend/src/session/tests.rs` — cancel 会话测试。
- Create: `crates/pyime-backend/src/bin/pyime-misses.rs` — 错例报告工具与单测。
- Modify: `crates/pyime-backend/Cargo.toml`、`crates/pyime-engine/Cargo.toml`、`Cargo.lock` — 版本 0.1.9。

---

### Task 1: inputlog 合并写入、版本号、cancel 事件、测试捕获

**Files:**
- Modify: `crates/pyime-backend/src/inputlog.rs`(全文件,现 59 行)

**Interfaces:**
- Produces:`pub(crate) const TOP: usize = 5;`、`pub fn cancel(input: &str, top: &[String])`、
  `#[cfg(test)] pub fn take_captured() -> Vec<serde_json::Value>`。`select`/`retract`/`forget` 签名不变。

- [ ] **Step 1: 写失败的测试**

在 `inputlog.rs` 末尾追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 每行带版本与时间戳() {
        take_captured();
        select("nihao", "", "nihao", "你好", 0, &["你好", "你", "泥", "尼", "拟", "逆"]);
        let lines = take_captured();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["v"], env!("CARGO_PKG_VERSION"));
        assert!(lines[0]["ts"].as_u64().unwrap() > 0);
        assert_eq!(lines[0]["text"], "你好");
        assert_eq!(lines[0]["top"].as_array().unwrap().len(), TOP);
    }

    #[test]
    fn 放弃输入记拼音与候选() {
        take_captured();
        cancel("chelue", &["车".to_string(), "撤".to_string()]);
        let lines = take_captured();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["cancel"], "chelue");
        assert_eq!(lines[0]["top"], json!(["车", "撤"]));
        assert_eq!(lines[0]["v"], env!("CARGO_PKG_VERSION"));
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend inputlog`
Expected: 编译失败,`cannot find function take_captured` / `cancel`。

- [ ] **Step 3: 实现**

`use serde_json::json;` 改为 `use serde_json::{json, Value};`;`const TOP` 改为 `pub(crate) const TOP: usize = 5;`。
`init` 不变。把 `select`、`retract`、`forget` 三个函数整体替换为下面的代码(并新增 `write`、`cancel`、捕获):

```rust
/// 写一行:补上时间戳与后端版本号。测试下同时存入本线程的捕获列表,供断言。
fn write(mut line: Value) {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    line["ts"] = json!(ms);
    line["v"] = json!(env!("CARGO_PKG_VERSION"));
    #[cfg(test)]
    CAPTURED.with(|c| c.borrow_mut().push(line.clone()));
    let Some(file) = FILE.get() else { return };
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(f, "{line}");
    }
}

/// 一次选词:`input` 为选词时的完整拼音串,`typed` 为本次消耗的部分,`index` 为选中项在全部候选中的下标。
pub fn select(input: &str, context: &str, typed: &str, text: &str, index: usize, top: &[&str]) {
    let top: Vec<&str> = top.iter().take(TOP).copied().collect();
    write(json!({ "input": input, "context": context, "typed": typed, "text": text, "index": index, "top": top }));
}

/// 撤销一次组词的学习:`items` 为 (typed, text, pairs, prev)。
pub fn retract(items: &[(String, String, u16, String)]) {
    write(json!({ "retract": items }));
}

/// 用户删除了某候选的个人学习:`input` 为当时的拼音串,`noop` 表示删除后候选顺序没有变化。
pub fn forget(input: &str, text: &str, noop: bool) {
    write(json!({ "forget": text, "input": input, "noop": noop }));
}

/// 放弃一次组字(Esc,或退格删光且无上屏):`input` 为本次组字中最长的拼音,`top` 为其当时的候选前几名。
pub fn cancel(input: &str, top: &[String]) {
    write(json!({ "cancel": input, "top": top }));
}

#[cfg(test)]
thread_local! {
    static CAPTURED: std::cell::RefCell<Vec<Value>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// 取出并清空本线程已写的日志行(仅测试)。
#[cfg(test)]
pub fn take_captured() -> Vec<Value> {
    CAPTURED.with(|c| std::mem::take(&mut *c.borrow_mut()))
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-backend` → 全部通过(原 55 个 + 新 2 个);`cargo build --all-targets` → 0 warning。
(`cancel` 暂未被调用,若出现 `function never used` 警告,Task 2 会消除;本任务允许这一条 dead_code 警告并在报告里注明。)

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/inputlog.rs
git commit -m "后端:输入日志每行带版本号,新增放弃输入事件,测试可捕获日志行

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 2: 会话记录放弃输入

**Files:**
- Modify: `crates/pyime-backend/src/session.rs`(`Session` 结构体与 `new`、`refresh`、`flush_raw`、`select`、`on_key_down` 中 `VK_ESCAPE`/`VK_BACK` 分支)
- Test: `crates/pyime-backend/src/session/tests.rs`

**Interfaces:**
- Consumes:`crate::inputlog::cancel(input: &str, top: &[String])`、`crate::inputlog::TOP`、
  `#[cfg(test)] crate::inputlog::take_captured() -> Vec<Value>`(Task 1)。
- 测试辅助(已有):`session()`、`press`、`type_pick`、`shift_click`、常量 `N I H A O W`、`VK_ESCAPE`、`VK_BACK`、`VK_RETURN`。

- [ ] **Step 1: 写失败的测试**

在 `session/tests.rs` 末尾追加:

```rust
/// 取出本测试线程至今写出的 cancel 行的拼音(同时清空捕获列表)。
fn cancels() -> Vec<String> {
    crate::inputlog::take_captured().into_iter().filter_map(|l| l["cancel"].as_str().map(String::from)).collect()
}

#[test]
fn esc放弃组字记一次cancel() {
    let mut s = session();
    cancels();
    press(&mut s, &[N, I, H, A, O]);
    press(&mut s, &[VK_ESCAPE]);
    let lines: Vec<Value> = crate::inputlog::take_captured().into_iter().filter(|l| l["cancel"].is_string()).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["cancel"], "nihao");
    assert_eq!(lines[0]["top"][0], "你好");
}

#[test]
fn 退格删光拼音记最长的串() {
    let mut s = session();
    cancels();
    press(&mut s, &[N, I, H, A, O]);
    press(&mut s, &[VK_BACK, VK_BACK, VK_BACK, VK_BACK, VK_BACK]);
    assert_eq!(cancels(), ["nihao"]);
}

#[test]
fn 部分选词后esc也记cancel() {
    let mut s = session();
    cancels();
    type_pick(&mut s, &[N, I, H, A, O], "你");
    press(&mut s, &[VK_ESCAPE]);
    assert_eq!(cancels(), ["nihao"]);
}

#[test]
fn 回车与shift切英文不记cancel() {
    let mut s = session();
    cancels();
    press(&mut s, &[N, I]);
    press(&mut s, &[VK_RETURN]);
    press(&mut s, &[W, O]);
    shift_click(&mut s);
    assert!(cancels().is_empty());
}

#[test]
fn 上屏后新组字的cancel记新串() {
    let mut s = session();
    cancels();
    type_pick(&mut s, &[N, I], "你");
    press(&mut s, &[W, O]);
    press(&mut s, &[VK_ESCAPE]);
    assert_eq!(cancels(), ["wo"]);
}
```

注:`部分选词后esc也记cancel` 依赖测试词典中 `nihao` 的首页候选含"你"(`type_pick` 找不到会 panic)。
若不含,报告 NEEDS_CONTEXT,不要改测试词典。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend cancel`
Expected: `esc放弃组字记一次cancel`、`退格删光拼音记最长的串`、`部分选词后esc也记cancel`、`上屏后新组字的cancel记新串` 失败(没有 cancel 行);`回车与shift切英文不记cancel` 通过。

- [ ] **Step 3: 实现**

1) `Session` 结构体在 `back_counted` 字段之后加:

```rust
    /// 本次组字中最长的拼音及其当时的候选前几名;放弃组字时写入日志。
    peak: Option<(String, Vec<String>)>,
```

`Session::new` 的结构体字面量末尾 `back_counted: None` 后加 `, peak: None`。

2) `refresh` 改为:

```rust
    fn refresh(&mut self) {
        self.cands = if self.input.is_empty() { Vec::new() } else { self.ime.candidates(&self.input, &self.prev()) };
        self.page = 0;
        self.cursor = 0;
        if self.peak.as_ref().map_or(0, |p| p.0.len()) < self.input.len() {
            let top = self.cands.iter().take(crate::inputlog::TOP).map(|c| c.text.clone()).collect();
            self.peak = Some((self.input.clone(), top));
        }
    }
```

3) `flush_raw` 中 `self.context.clear();` 之后加 `self.peak = None;`(该函数被 Esc、回车、空格无候选、Shift 切英文、退格删光共用)。

4) `select` 中 `if self.input.is_empty() {` 分支内,`self.cands.clear();` 之后加 `self.peak = None;`。

5) 在 `forget` 方法之前新增:

```rust
    /// 放弃组字(Esc,或退格删光且无上屏):把本次最长的拼音记为一次放弃。
    fn log_cancel(&mut self) {
        if let Some((input, top)) = self.peak.take() {
            crate::inputlog::cancel(&input, &top);
        }
    }
```

6) `on_key_down` 的 match 中:

```rust
            VK_ESCAPE => {
                self.log_cancel();
                self.flush_raw();
            }
```

`VK_BACK` 分支改为:

```rust
            VK_BACK => {
                self.input.pop();
                if self.input.is_empty() {
                    if self.pending.is_empty() {
                        self.log_cancel();
                    }
                    commit = Some(self.flush_raw()).filter(|t| !t.is_empty());
                } else {
                    self.refresh();
                }
            }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-backend` → 全部通过;`cargo build --all-targets` → 0 warning(Task 1 的 dead_code 警告此时应消失)。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "后端:Esc 或退格删光放弃组字时记录本次最长拼音与候选

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 3: `pyime-misses` 错例报告工具

**Files:**
- Create: `crates/pyime-backend/src/bin/pyime-misses.rs`(实现 + `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes:日志行格式(Task 1/2):select `{"ts","v","input","context","typed","text","index","top"}`、
  retract `{"ts","v","retract":[[typed,text,pairs,prev],…]}`(分段上屏时末项为整句)、
  forget `{"ts","v","forget","input","noop"}`、cancel `{"ts","v","cancel","top"}`;旧版行无 `v`。
- Produces:命令 `pyime-misses <input-log.jsonl>`,打印报告。crate 无 lib,本文件自包含,不引用后端模块。

规则说明(与 spec 一致,另有一处细化):
- 分组:select 行 = 同时有 `typed` 与 `text` 的行;`forget` 行跳过不打断分组;`retract`/`cancel` 行结束正在等待的组。
- 完整组:组拼音(各段 typed 拼接)== 首行 `input`。不完整组不计入 ② ③。
- **细化**:作为 ① 纠错期望而被累加的组,也不再计入 ② ③(否则同一件事在 ① 和 ③ 各出一条)。

- [ ] **Step 1: 写失败的测试**

先建文件,只放测试模块与空的 `fn main() {}`,测试引用的 `analyze`/`render`/`Report`/`Case` 尚不存在:

```rust
//! 错例报告(开发用)。
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn sel(input: &str, typed: &str, text: &str, index: u64, top: &[&str], ts: u64) -> Value {
        json!({"v": "0.1.9", "ts": ts, "input": input, "context": "", "typed": typed, "text": text, "index": index, "top": top})
    }

    /// retract 行:items 依次为各段,分段上屏时末项为整句。
    fn ret(items: &[(&str, &str)], ts: u64) -> Value {
        let items: Vec<Value> = items.iter().map(|(t, w)| json!([t, w, 0, ""])).collect();
        json!({"v": "0.1.9", "ts": ts, "retract": items})
    }

    fn can(input: &str, top: &[&str], ts: u64) -> Value {
        json!({"v": "0.1.9", "ts": ts, "cancel": input, "top": top})
    }

    fn cases(r: &Report) -> Vec<(u8, &str, &str, usize)> {
        r.cases.iter().map(|c| (c.kind, c.input.as_str(), c.expect.as_str(), c.count)).collect()
    }

    #[test]
    fn 删光后分两次重打_得到纠错用例() {
        let r = analyze(&[
            sel("qinjixu", "qinjixu", "亲继续", 0, &["亲继续", "请继续"], 1000),
            ret(&[("qinjixu", "亲继续")], 2000),
            sel("qin", "qin", "请", 0, &["请", "亲"], 3000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 4000),
        ]);
        assert_eq!(cases(&r), [(1, "qinjixu", "请继续", 1)]);
        assert_eq!(r.cases[0].note, "误选 亲继续");
        assert!(r.suspects.is_empty());
    }

    #[test]
    fn 分段上屏后删光_以整句项为准() {
        let r = analyze(&[
            sel("qinjixu", "qin", "亲", 0, &["亲继续"], 1000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
            ret(&[("qin", "亲"), ("jixu", "继续"), ("qinjixu", "亲继续")], 2000),
            sel("qinjixu", "qinjixu", "请继续", 1, &["亲继续", "请继续"], 3000),
        ]);
        assert_eq!(cases(&r), [(1, "qinjixu", "请继续", 1)]);
    }

    #[test]
    fn 整串重选改词_撤回写在新选词之后() {
        let r = analyze(&[
            sel("nihao", "nihao", "你号", 0, &["你号", "你好"], 1000),
            sel("nihao", "nihao", "你好", 1, &["你号", "你好"], 3000),
            ret(&[("nihao", "你号")], 3000),
        ]);
        assert_eq!(cases(&r), [(1, "nihao", "你好", 1)]);
    }

    #[test]
    fn 删光后改打模糊拼音_进疑似清单() {
        let r = analyze(&[
            sel("qinjixu", "qinjixu", "亲继续", 0, &["亲继续"], 1000),
            ret(&[("qinjixu", "亲继续")], 2000),
            sel("qingjixu", "qingjixu", "请继续", 0, &["请继续"], 3000),
        ]);
        assert!(r.cases.is_empty());
        assert_eq!(r.suspects, ["qinjixu 误选 亲继续 → 改打 qingjixu 得 请继续"]);
    }

    #[test]
    fn 撤回后超过60秒才重打_不出用例() {
        let r = analyze(&[
            sel("qinjixu", "qinjixu", "亲继续", 0, &["亲继续"], 1000),
            ret(&[("qinjixu", "亲继续")], 2000),
            sel("qinjixu", "qinjixu", "请继续", 0, &["请继续"], 2000 + 60_001),
        ]);
        assert!(r.cases.is_empty());
        assert_eq!(r.suspects, ["qinjixu 误选 亲继续 → 之后 60 秒内没有重打"]);
    }

    #[test]
    fn 拆开选且首选不是整句() {
        let r = analyze(&[
            sel("qinjixu", "qin", "请", 1, &["亲继续", "请"], 1000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
        ]);
        assert_eq!(cases(&r), [(2, "qinjixu", "请继续", 1)]);
        assert_eq!(r.cases[0].note, "2 段");
    }

    #[test]
    fn 拆开选但首选就是整句_不出() {
        let r = analyze(&[
            sel("qinjixu", "qin", "请", 1, &["请继续", "请"], 1000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
        ]);
        assert!(r.cases.is_empty());
    }

    #[test]
    fn 末尾多余字母_不出拆开选() {
        let r = analyze(&[
            sel("nihaou", "ni", "你", 1, &["你好", "你"], 1000),
            sel("haou", "hao", "好", 0, &["好"], 1100),
        ]);
        assert!(r.cases.is_empty());
    }

    #[test]
    fn 链条断裂的首行_不出非首位() {
        let r = analyze(&[
            sel("qinji", "qin", "请", 2, &["亲吉", "亲", "请"], 1000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
        ]);
        assert!(r.cases.is_empty());
    }

    #[test]
    fn 两段之间夹forget仍归一组() {
        let r = analyze(&[
            sel("qinjixu", "qin", "请", 1, &["亲继续", "请"], 1000),
            json!({"v": "0.1.9", "ts": 1050, "forget": "亲继续", "input": "jixu", "noop": false}),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
        ]);
        assert_eq!(cases(&r), [(2, "qinjixu", "请继续", 1)]);
    }

    #[test]
    fn 单段非首位() {
        let r = analyze(&[sel("zai", "zai", "在", 1, &["再", "在"], 1000)]);
        assert_eq!(cases(&r), [(3, "zai", "在", 1)]);
        assert_eq!(r.cases[0].note, "上文: ");
    }

    #[test]
    fn 被撤回的上屏不出非首位() {
        let r = analyze(&[
            sel("zai", "zai", "在", 1, &["再", "在"], 1000),
            ret(&[("zai", "在")], 2000),
        ]);
        assert!(r.cases.is_empty());
    }

    #[test]
    fn 相同用例合并计数() {
        let r = analyze(&[
            sel("zai", "zai", "在", 1, &["再", "在"], 1000),
            sel("zai", "zai", "在", 1, &["再", "在"], 2000),
        ]);
        assert_eq!(cases(&r), [(3, "zai", "在", 2)]);
    }

    #[test]
    fn 放弃后30秒内上屏_列入疑似() {
        let r = analyze(&[
            can("chelue", &["车路饿", "车"], 1000),
            sel("celve", "celve", "策略", 0, &["策略"], 5000),
        ]);
        assert_eq!(r.suspects, ["chelue(候选: 车路饿 车)→ 改打 celve 得 策略"]);
    }

    #[test]
    fn 放弃超过30秒_不配对() {
        let r = analyze(&[
            can("chelue", &["车路饿"], 1000),
            sel("celve", "celve", "策略", 0, &["策略"], 1000 + 30_001),
        ]);
        assert!(r.suspects.is_empty());
    }

    #[test]
    fn 单字母放弃视为误敲() {
        let r = analyze(&[
            can("chelue", &["车路饿"], 1000),
            can("c", &["才"], 2000),
            sel("celve", "celve", "策略", 0, &["策略"], 3000),
        ]);
        assert_eq!(r.suspects, ["chelue(候选: 车路饿)→ 改打 celve 得 策略"]);
    }

    #[test]
    fn 报告列出版本与用例() {
        let r = analyze(&[
            json!({"ts": 1, "input": "ni", "context": "", "typed": "ni", "text": "你", "index": 0, "top": ["你"]}),
            sel("zai", "zai", "在", 1, &["再", "在"], 1000),
        ]);
        assert_eq!(r.lines, 2);
        assert_eq!(r.versions, ["旧版", "0.1.9"]);
        let out = render(&r);
        assert!(out.starts_with("共 2 条日志;版本: 旧版, 0.1.9\n"), "{out}");
        assert!(out.contains("== ① 纠错 ==\n    (无)\n"), "{out}");
        assert!(out.contains("    (\"zai\", \"在\", 1), // ×1 上文: \n"), "{out}");
        assert!(out.contains("== 疑似找不到 ==\n    (无)\n"), "{out}");
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend --bin pyime-misses`
Expected: 编译失败,`cannot find function analyze` 等。

- [ ] **Step 3: 实现**

把文件开头的 `//! 错例报告(开发用)。` 与 `fn main() {}` 替换为下面的代码(测试模块保留在末尾):

```rust
//! 错例报告:读 `input-log.jsonl`,只用日志里当时显示的候选(不加载词典),找出首选不对的上屏,
//! 输出可贴进 `regression.rs` 的用例与"疑似找不到"清单。仅本机开发用,不进安装包。
//! 用法:pyime-misses <input-log.jsonl>
use serde_json::Value;

/// 纠错配对:撤回后多久内的重打算数(毫秒)。
const RETYPE_MS: u64 = 60_000;
/// 放弃输入后多久内的上屏算作改打(毫秒)。
const CANCEL_MS: u64 = 30_000;

/// 一次上屏:连续分段选词合成的组。
struct Group {
    /// 各段 typed 拼接。
    typed: String,
    text: String,
    first_input: String,
    first_top: Option<String>,
    first_index: u64,
    segs: usize,
    context: String,
    /// 首段时刻(毫秒)。
    ts: u64,
    /// 误选组或已作为纠错期望,不再计入 ② ③。
    skip: bool,
}

impl Group {
    /// 输入被选完:没有被退格删掉的尾巴,分段链条也没有断。
    fn complete(&self) -> bool {
        self.typed == self.first_input
    }
}

enum Event {
    Group(usize),
    /// 撤回:取最后一项(分段上屏时为整句)。
    Retract { typed: String, text: String, ts: u64 },
    Cancel { input: String, top: Vec<String>, ts: u64 },
}

struct Case {
    /// 1 纠错、2 拆开选、3 选了非首位。
    kind: u8,
    input: String,
    expect: String,
    note: String,
    count: usize,
}

#[derive(Default)]
struct Report {
    lines: usize,
    versions: Vec<String>,
    cases: Vec<Case>,
    suspects: Vec<String>,
}

impl Report {
    /// 相同 (类型, 拼音, 期望) 合并计数,注释保留首次的。
    fn add(&mut self, kind: u8, input: &str, expect: &str, note: String) {
        if let Some(c) = self.cases.iter_mut().find(|c| c.kind == kind && c.input == input && c.expect == expect) {
            c.count += 1;
        } else {
            self.cases.push(Case { kind, input: input.to_string(), expect: expect.to_string(), note, count: 1 });
        }
    }
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else { return Err("用法: pyime-misses <input-log.jsonl>".into()) };
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取 {path} 失败: {e}"))?;
    let lines: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    print!("{}", render(&analyze(&lines)));
    Ok(())
}

/// 把日志行按顺序拆成上屏组与撤回/放弃事件,顺带统计条数与版本。
fn parse(lines: &[Value], report: &mut Report) -> (Vec<Group>, Vec<Event>) {
    let (mut groups, mut events) = (Vec::<Group>::new(), Vec::new());
    // 正在等待下一段的组及其剩余拼音
    let mut open: Option<(usize, String)> = None;
    for v in lines {
        report.lines += 1;
        let ver = v["v"].as_str().unwrap_or("旧版").to_string();
        if !report.versions.contains(&ver) {
            report.versions.push(ver);
        }
        let ts = v["ts"].as_u64().unwrap_or(0);
        if let Some(items) = v["retract"].as_array() {
            open = None;
            if let Some(last) = items.last() {
                events.push(Event::Retract { typed: str_of(&last[0]), text: str_of(&last[1]), ts });
            }
            continue;
        }
        if let Some(input) = v["cancel"].as_str() {
            open = None;
            let top = v["top"].as_array().map(|a| a.iter().map(str_of).collect()).unwrap_or_default();
            events.push(Event::Cancel { input: input.to_string(), top, ts });
            continue;
        }
        let (Some(input), Some(typed), Some(text)) = (v["input"].as_str(), v["typed"].as_str(), v["text"].as_str()) else { continue };
        let idx = match &open {
            Some((g, rest)) if rest == input => {
                let g = *g;
                groups[g].typed.push_str(typed);
                groups[g].text.push_str(text);
                groups[g].segs += 1;
                g
            }
            _ => {
                groups.push(Group {
                    typed: typed.to_string(),
                    text: text.to_string(),
                    first_input: input.to_string(),
                    first_top: v["top"][0].as_str().map(String::from),
                    first_index: v["index"].as_u64().unwrap_or(0),
                    segs: 1,
                    context: str_of(&v["context"]),
                    ts,
                    skip: false,
                });
                events.push(Event::Group(groups.len() - 1));
                groups.len() - 1
            }
        };
        open = input.strip_prefix(typed).filter(|r| !r.is_empty()).map(|r| (idx, r.to_string()));
    }
    (groups, events)
}

/// 撤回 (r, w):向前找误选组,再把其后的上屏(撤回后的只算 60 秒内)按序累加,
/// 拼音恰好凑成 r 时累加文字即期望;凑不上则列入疑似清单。
fn retract(report: &mut Report, groups: &mut [Group], events: &[Event], i: usize, r: &str, w: &str, ts: u64) {
    let found = (0..i).rev().find_map(|j| match events[j] {
        Event::Group(g) if !groups[g].skip && groups[g].typed == r && groups[g].text == w => Some((j, g)),
        _ => None,
    });
    let Some((pos, wrong)) = found else { return };
    groups[wrong].skip = true;
    let (mut typed, mut text, mut used) = (String::new(), String::new(), Vec::new());
    let mut first: Option<usize> = None;
    for (j, e) in events.iter().enumerate().skip(pos + 1) {
        let Event::Group(g) = *e else { continue };
        if j > i && groups[g].ts.saturating_sub(ts) > RETYPE_MS {
            break;
        }
        first.get_or_insert(g);
        typed.push_str(&groups[g].typed);
        text.push_str(&groups[g].text);
        used.push(g);
        if !r.starts_with(typed.as_str()) {
            break;
        }
        if typed == r {
            if text != w {
                for g in used {
                    groups[g].skip = true;
                }
                report.add(1, r, &text, format!("误选 {w}"));
            }
            return;
        }
    }
    let after = match first {
        Some(g) => format!("改打 {} 得 {}", groups[g].typed, groups[g].text),
        None => "之后 60 秒内没有重打".to_string(),
    };
    report.suspects.push(format!("{r} 误选 {w} → {after}"));
}

/// 放弃输入后 30 秒内的第一次上屏列为疑似找不到;其间又有有效放弃则以后者为准。
/// 拼音不足 2 个字母或没有候选的放弃视为误敲,忽略。
fn cancel(report: &mut Report, groups: &[Group], events: &[Event], i: usize, input: &str, top: &[String], ts: u64) {
    let valid = |s: &str, t: &[String]| s.len() >= 2 && !t.is_empty();
    if !valid(input, top) {
        return;
    }
    for e in &events[i + 1..] {
        match e {
            Event::Cancel { input: s, top: t, .. } if valid(s.as_str(), t.as_slice()) => return,
            Event::Group(g) => {
                let g = &groups[*g];
                if g.ts.saturating_sub(ts) <= CANCEL_MS {
                    report.suspects.push(format!("{input}(候选: {})→ 改打 {} 得 {}", top.join(" "), g.typed, g.text));
                }
                return;
            }
            _ => {}
        }
    }
}

fn analyze(lines: &[Value]) -> Report {
    let mut report = Report::default();
    let (mut groups, events) = parse(lines, &mut report);
    for (i, e) in events.iter().enumerate() {
        match e {
            Event::Retract { typed, text, ts } => retract(&mut report, &mut groups, &events, i, typed, text, *ts),
            Event::Cancel { input, top, ts } => cancel(&mut report, &groups, &events, i, input, top, *ts),
            Event::Group(_) => {}
        }
    }
    for g in groups.iter().filter(|g| !g.skip && g.complete()) {
        if g.segs > 1 && g.first_top.as_deref() != Some(g.text.as_str()) {
            report.add(2, &g.typed, &g.text, format!("{} 段", g.segs));
        } else if g.segs == 1 && g.first_index > 0 {
            report.add(3, &g.typed, &g.text, format!("上文: {}", g.context));
        }
    }
    // 稳定排序:同类内按次数降序,同次数保持首次出现顺序
    report.cases.sort_by_key(|c| (c.kind, std::cmp::Reverse(c.count)));
    report
}

/// 报告文本:统计行、三类用例(可直接贴进 regression.rs)、疑似清单。
fn render(r: &Report) -> String {
    let mut out = format!("共 {} 条日志;版本: {}\n", r.lines, r.versions.join(", "));
    for (kind, title) in [(1, "① 纠错"), (2, "② 拆开选"), (3, "③ 选了非首位")] {
        out.push_str(&format!("== {title} ==\n"));
        let cases: Vec<&Case> = r.cases.iter().filter(|c| c.kind == kind).collect();
        if cases.is_empty() {
            out.push_str("    (无)\n");
        }
        for c in cases {
            out.push_str(&format!("    ({:?}, {:?}, 1), // ×{} {}\n", c.input, c.expect, c.count, c.note));
        }
    }
    out.push_str("== 疑似找不到 ==\n");
    if r.suspects.is_empty() {
        out.push_str("    (无)\n");
    }
    for s in &r.suspects {
        out.push_str(&format!("    {s}\n"));
    }
    out
}
```

测试模块里的 `use serde_json::{json, Value};` 与上面的 `use serde_json::Value;` 不冲突(`use super::*` 引入的 `Value` 与显式导入同名同物,如编译器报重复导入,把测试模块里的改为 `use serde_json::json;`)。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-backend --bin pyime-misses` → 17 passed。
Run: `cargo build --all-targets` → 0 warning;`cargo test` → 全部通过。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/bin/pyime-misses.rs
git commit -m "工具:新增 pyime-misses,从输入日志生成纠错/拆开选/非首位回归用例与疑似清单

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 版本 0.1.9 与真实日志试跑

**Files:**
- Modify: `crates/pyime-backend/Cargo.toml`、`crates/pyime-engine/Cargo.toml`(`version = "0.1.8"` → `"0.1.9"`)、`Cargo.lock`(由 cargo 自动更新)

- [ ] **Step 1: 改版本号**

两个 `Cargo.toml` 的 `version = "0.1.8"` 改为 `version = "0.1.9"`,运行 `cargo build` 让 `Cargo.lock` 同步。

- [ ] **Step 2: 全量验证**

Run: `cargo test` → 全部通过;`cargo build --all-targets` → 0 warning。

- [ ] **Step 3: 真实日志试跑(只读)**

Run: `cargo run -q -p pyime-backend --bin pyime-misses -- /tmp/input-log.jsonl > /tmp/misses-report.txt 2>&1`
然后 Read `/tmp/misses-report.txt`。Expected:首行 `共 57 条日志;版本: 旧版`(条数以能解析的行为准;该日志来自 0.1.7,无 `v` 字段,也没有 cancel);
不 panic。把报告内容原样附在任务报告里。**不得**修改 `/tmp/input-log.jsonl`。

Run: `cargo run -q -p pyime-backend --bin pyime-replay -- installer/pyime.dict /tmp/input-log.jsonl > /tmp/replay-0.1.9.txt 2>&1`
Read 结果,确认 `pyime-replay` 仍正常输出统计行(cancel 行无 `typed`,会被跳过)。

- [ ] **Step 4: Commit**

```bash
git add crates/pyime-backend/Cargo.toml crates/pyime-engine/Cargo.toml Cargo.lock
git commit -m "版本 0.1.9

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
