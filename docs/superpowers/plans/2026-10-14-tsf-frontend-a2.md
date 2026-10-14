# 子项目 A2:pyime-server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 做出可独立运行的 `pyime-server.exe`:命名管道服务多个宿主、按会话号驱动 `Hub`、在光标处显示最小候选窗;并提供 `pyime-probe` 探针,在 TSF 客户端(A3)完成前就能在 Windows 上验证 server。

**Architecture:** 跨平台的纯逻辑(`ui` 模型、`Dispatcher`、`run_pending`)放在 crate 根,Linux 可测;Windows 专有部分(管道 I/O 线程、消息窗口唤醒、GDI 候选窗、单实例)放在 `#[cfg(windows)] mod win`。主线程独占 `Hub`/候选窗并跑消息循环;每条管道连接一个 I/O 线程,经 `mpsc::channel` 把 `Job` 送主线程、`PostMessageW` 唤醒、等响应后写回。

**Tech Stack:** Rust 2021,`windows` 0.62.2,`pyime-backend`(库)、`pyime-protocol`,mingw 交叉编译 `x86_64-pc-windows-gnu`。

**Spec:** `docs/superpowers/specs/2026-10-14-tsf-frontend-a-design.md`(server 线程模型、单实例、DACL、候选窗样式各节)

## Global Constraints

- 分支 `spike/tsf-gnu`,不 push;回复、commit、注释一律简体中文;commit 末尾加 `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`。
- `Session` 及其测试一行不改;`pyime-backend` 的 PIME 版 bin 行为不变,现有测试数不减(当前:backend lib 40 + bin 7,engine 42,protocol 8,tsf 2)。
- 修订 A1 的约束:允许 `pyime-protocol` 增加 `#[cfg(windows)]` 的 `current_user_sid()`(仅此一个 Windows 依赖用途);`pyime-backend` 仍不得依赖 Windows 专有 crate。
- 管道名 `\\.\pipe\pyime-<用户 SID>`,帧为 4 字节小端长度前缀 + JSON,上限 1 MiB(由 `pyime-protocol` 提供)。
- 候选窗样式 `WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST`,不抢焦点;进程启动即声明 per-monitor v2 DPI 感知。
- 单实例:命名互斥体 `Local\PYIME-Server` + 首个管道实例标志 `FILE_FLAG_FIRST_PIPE_INSTANCE`。
- 管道 DACL:当前用户(OW)与 `ALL APPLICATION PACKAGES`(AC)可读写,低完整性标签。
- 构建只用 `scripts/build-*.sh` 的单次 `RUSTFLAGS`(`-C panic=abort -C target-feature=+crt-static`),不改 workspace `[profile]`。
- `Cargo.lock` 变化不由子代理提交(其无权读取);在报告中说明,由控制器处理。本机无 clippy,不跑。不得为过测试削弱既有测试。

## File Structure

- `Cargo.toml`(根):members 加 `crates/pyime-server`。
- `crates/pyime-backend/Cargo.toml`、`src/ime.rs`:加 `test-util` feature(Task 1)。
- `crates/pyime-protocol/Cargo.toml`、`src/lib.rs`:加 `current_user_sid()`(Task 4)。
- `crates/pyime-server/Cargo.toml`
- `crates/pyime-server/src/lib.rs`:`pub mod ui; pub mod core;` + `#[cfg(windows)] pub mod win;`
- `crates/pyime-server/src/ui.rs`:`Rect`、`UiCmd`、`CandView`、`ui_from_resp`(Task 2)。
- `crates/pyime-server/src/core.rs`:`Dispatcher`、`Outcome`、`Job`、`run_pending`(Task 3)。
- `crates/pyime-server/src/win/{mod.rs,pipe.rs,cand.rs,single.rs}`:Windows 部分(Task 4–5)。
- `crates/pyime-server/src/main.rs`、`src/bin/probe.rs`:入口与探针(Task 4、6)。
- `scripts/build-server.sh`(Task 6)。

---

### Task 1: pyime-backend 加 test-util feature

**Files:**
- Modify: `crates/pyime-backend/Cargo.toml`, `crates/pyime-backend/src/ime.rs`

**Interfaces:**
- Produces:feature `test-util`:启用后 `Ime::with_engine`、`Ime::user_export`、`Ime::take_pressed` 及"记录模拟按键"行为对下游 crate 可用(原先只在本 crate 的 `#[cfg(test)]` 下)。

- [ ] **Step 1: 看现状**

Run: `grep -n "cfg" crates/pyime-backend/src/ime.rs`
Expected: 列出所有 `cfg(test)` / `cfg(windows)` / `cfg(not(any(windows, test)))` 位置,记录行号。

- [ ] **Step 2: Cargo.toml 加 feature**

在 `crates/pyime-backend/Cargo.toml` 末尾追加:

```toml

[features]
# 给下游 crate 的测试用:暴露 Ime::with_engine 等测试辅助,并让 press 只记录不真发按键。
test-util = []
```

- [ ] **Step 3: ime.rs 把测试条件统一换成 test-util**

把 `ime.rs` 中所有 `test` 条件换成 `any(test, feature = "test-util")`:
- `#[cfg(test)]` → `#[cfg(any(test, feature = "test-util"))]`
- `#[cfg(not(test))]` → `#[cfg(not(any(test, feature = "test-util")))]`
- `#[cfg(not(any(windows, test)))]` → `#[cfg(not(any(windows, test, feature = "test-util")))]`
- 结构体字面量里行内的 `#[cfg(test)] pressed: ...` 同样替换。

逐处核对 `press()` 三个分支仍互斥且恰有一个生效:test/test-util → 只记录;否则 windows → `send_key`;否则空操作。

- [ ] **Step 4: 回归 + 带 feature 编译**

Run: `cargo test -p pyime-backend > /tmp/a2t1.log 2>&1; grep -E "^test result|error|warning: unused" /tmp/a2t1.log; cargo check -p pyime-backend --features test-util > /tmp/a2t1b.log 2>&1; tail -3 /tmp/a2t1b.log; cargo check --target x86_64-pc-windows-gnu -p pyime-backend > /tmp/a2t1c.log 2>&1; tail -2 /tmp/a2t1c.log`
Expected: backend lib 40 + bin 7 全过;带 feature 的 check 与 windows 目标的 check 都 `Finished`。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/Cargo.toml crates/pyime-backend/src/ime.rs
git commit -m "pyime-backend:加 test-util feature,下游 crate 可用测试引擎

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 2: pyime-server crate 骨架 + ui 模型(TDD)

**Files:**
- Modify: `Cargo.toml`(根 members 加 `"crates/pyime-server"`)
- Create: `crates/pyime-server/Cargo.toml`, `crates/pyime-server/src/lib.rs`, `crates/pyime-server/src/ui.rs`, `crates/pyime-server/src/main.rs`

**Interfaces:**
- Produces(`pyime_server::ui`):`Rect{left,top,right,bottom: i32}`(`Debug, Clone, Copy, PartialEq, Eq, Default`);`CandView{labels: Vec<String>, cursor: usize, font: String, font_size: i32, rect: Rect}`;`UiCmd{Hide, Show(CandView), Move(Rect)}`;`DEFAULT_FONT: &str`、`DEFAULT_FONT_SIZE: i32`;`ui_from_resp(resp: &serde_json::Value, rect: Option<Rect>) -> Option<UiCmd>`。语义:`showCandidates=false` → `Hide`;`true` 且 `candidateList` 非空 → `Show`;`true` 但无列表 → `None`;`true` 且列表为空 → `Hide`;无 `showCandidates` 字段 → `None`(保持不变)。

- [ ] **Step 1: Cargo.toml**

`crates/pyime-server/Cargo.toml`:

```toml
[package]
name = "pyime-server"
version = "0.0.1"
edition = "2021"
license = "GPL-3.0-only"
publish = false

[dependencies]
pyime-backend = { path = "../pyime-backend" }
pyime-protocol = { path = "../pyime-protocol" }
serde_json = "1"

[dev-dependencies]
pyime-backend = { path = "../pyime-backend", features = ["test-util"] }
```

- [ ] **Step 2: lib.rs 与 main.rs(非 Windows 下可编译)**

`src/lib.rs`:

```rust
//! pyime-server:独立输入法服务进程。纯逻辑跨平台,Windows 专有部分在 `win`。

pub mod ui;
```

`src/main.rs`:

```rust
fn main() {
    eprintln!("pyime-server 目前只支持 Windows");
}
```

- [ ] **Step 3: 先写 ui.rs 的测试与桩(使其失败)**

```rust
//! 候选窗的显示模型:把 Session 的响应 JSON 翻成"显示/隐藏/移动"指令。纯逻辑,不依赖 Windows。

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandView {
    pub labels: Vec<String>,
    pub cursor: usize,
    pub font: String,
    pub font_size: i32,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiCmd {
    Hide,
    Show(CandView),
    Move(Rect),
}

pub const DEFAULT_FONT: &str = "Microsoft YaHei UI";
pub const DEFAULT_FONT_SIZE: i32 = 16;

pub fn ui_from_resp(_resp: &Value, _rect: Option<Rect>) -> Option<UiCmd> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rect() -> Rect {
        Rect { left: 100, top: 200, right: 110, bottom: 220 }
    }

    #[test]
    fn 隐藏() {
        assert_eq!(ui_from_resp(&json!({"showCandidates": false}), None), Some(UiCmd::Hide));
    }

    #[test]
    fn 显示带序号标签与默认字体() {
        let r = json!({"showCandidates": true, "candidateList": ["你", "泥"], "candidateCursor": 1});
        let Some(UiCmd::Show(v)) = ui_from_resp(&r, Some(rect())) else { panic!("应为 Show") };
        assert_eq!(v.labels, vec!["1 你", "2 泥"]);
        assert_eq!(v.cursor, 1);
        assert_eq!(v.font, DEFAULT_FONT);
        assert_eq!(v.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(v.rect, rect());
    }

    #[test]
    fn 字体取自customize_ui() {
        let r = json!({"showCandidates": true, "candidateList": ["你"],
            "customizeUI": {"candFontName": "SimSun", "candFontSize": 20}});
        let Some(UiCmd::Show(v)) = ui_from_resp(&r, None) else { panic!("应为 Show") };
        assert_eq!(v.font, "SimSun");
        assert_eq!(v.font_size, 20);
        assert_eq!(v.rect, Rect::default());
    }

    #[test]
    fn 高亮下标越界时收敛到最后一项() {
        let r = json!({"showCandidates": true, "candidateList": ["你"], "candidateCursor": 5});
        let Some(UiCmd::Show(v)) = ui_from_resp(&r, None) else { panic!("应为 Show") };
        assert_eq!(v.cursor, 0);
    }

    #[test]
    fn 无字段保持不变() {
        assert_eq!(ui_from_resp(&json!({"return": true}), None), None);
    }

    #[test]
    fn 显示但无列表保持不变_空列表则隐藏() {
        assert_eq!(ui_from_resp(&json!({"showCandidates": true}), None), None);
        assert_eq!(
            ui_from_resp(&json!({"showCandidates": true, "candidateList": []}), None),
            Some(UiCmd::Hide)
        );
    }
}
```

- [ ] **Step 4: 运行测试确认失败**

Run: `cargo test -p pyime-server > /tmp/a2t2.log 2>&1; grep -E "test result|FAILED|panicked" /tmp/a2t2.log | head`
Expected: 除 `无字段保持不变`、`显示但无列表...`的前半外多数 FAILED。

- [ ] **Step 5: 实现 ui_from_resp**

```rust
pub fn ui_from_resp(resp: &Value, rect: Option<Rect>) -> Option<UiCmd> {
    if !resp["showCandidates"].as_bool()? {
        return Some(UiCmd::Hide);
    }
    let list = resp["candidateList"].as_array()?;
    if list.is_empty() {
        return Some(UiCmd::Hide);
    }
    let labels: Vec<String> = list
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{} {}", i + 1, t.as_str().unwrap_or("")))
        .collect();
    let cursor = (resp["candidateCursor"].as_u64().unwrap_or(0) as usize).min(labels.len() - 1);
    let ui = &resp["customizeUI"];
    Some(UiCmd::Show(CandView {
        labels,
        cursor,
        font: ui["candFontName"].as_str().unwrap_or(DEFAULT_FONT).to_string(),
        font_size: ui["candFontSize"].as_i64().map_or(DEFAULT_FONT_SIZE, |n| n as i32),
        rect: rect.unwrap_or_default(),
    }))
}
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p pyime-server > /tmp/a2t2.log 2>&1; grep -E "test result|FAILED|panicked" /tmp/a2t2.log`
Expected: `6 passed`。

- [ ] **Step 7: Commit**(`Cargo.lock` 不提交,报告里说明)

```bash
git add Cargo.toml crates/pyime-server
git commit -m "pyime-server:crate 骨架与候选窗显示模型

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 3: Dispatcher 与 run_pending(TDD)

**Files:**
- Create: `crates/pyime-server/src/core.rs`
- Modify: `crates/pyime-server/src/lib.rs`(加 `pub mod core;`)

**Interfaces:**
- Consumes:`pyime_backend::hub::Hub`(`open/close/handle`)、`pyime_protocol::{ClientMessage, ServerMessage}`、Task 2 的 `Rect/UiCmd/ui_from_resp`。
- Produces(`pyime_server::core`):`Dispatcher::new(Hub) -> Dispatcher`、`Dispatcher::handle(&mut self, ClientMessage) -> Outcome`;`Outcome{reply: Option<ServerMessage>, ui: Option<UiCmd>}`;`Job{msg: ClientMessage, reply: mpsc::Sender<ServerMessage>}`;`run_pending(rx: &mpsc::Receiver<Job>, d: &mut Dispatcher, on_ui: impl FnMut(UiCmd)) -> usize`(排空队列,返回处理条数)。
- 候选窗归属规则:只有"当前显示候选窗的会话"(`shown_by`)发出的 `Hide`/`Move`/`Close` 才产生 UI 指令;非所有者的被丢弃(前台切换时旧会话的收尾不能把新会话的候选窗关掉)。`Show` 总是生效并把所有者设为该会话。

- [ ] **Step 1: lib.rs 加 `pub mod core;`**(放在 `pub mod ui;` 之前)。

- [ ] **Step 2: 先写测试与桩(使其失败)**

`crates/pyime-server/src/core.rs`:

```rust
//! 分发核心:管道消息 → Hub → 响应与候选窗指令。纯逻辑,不依赖 Windows,Linux 可测。

use crate::ui::{ui_from_resp, Rect, UiCmd};
use pyime_backend::hub::Hub;
use pyime_protocol::{ClientMessage, ServerMessage};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};

pub struct Dispatcher {
    hub: Hub,
    rects: HashMap<u64, Rect>,
    /// 当前正显示候选窗的会话。
    shown_by: Option<u64>,
}

pub struct Outcome {
    pub reply: Option<ServerMessage>,
    pub ui: Option<UiCmd>,
}

pub struct Job {
    pub msg: ClientMessage,
    pub reply: Sender<ServerMessage>,
}

impl Dispatcher {
    pub fn new(hub: Hub) -> Dispatcher {
        Dispatcher { hub, rects: HashMap::new(), shown_by: None }
    }

    pub fn handle(&mut self, _msg: ClientMessage) -> Outcome {
        Outcome { reply: None, ui: None }
    }
}

pub fn run_pending(_rx: &Receiver<Job>, _d: &mut Dispatcher, _on_ui: impl FnMut(UiCmd)) -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyime_backend::ime::Ime;
    use pyime_engine::dict::{Dict, DictBuilder};
    use pyime_engine::engine::Engine;
    use pyime_engine::fuzzy::FuzzyConfig;
    use serde_json::{json, Value};
    use std::sync::mpsc::channel;

    const N: i64 = 78;
    const I: i64 = 73;
    const Q: i64 = 81;
    const VK_ESCAPE: i64 = 27;

    fn dispatcher() -> Dispatcher {
        let mut b = DictBuilder::default();
        for (i, w) in "你泥尼拟逆腻匿溺".chars().enumerate() {
            b.add("ni", &w.to_string(), 9000 - i as u32 * 100);
        }
        let engine = Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default());
        Dispatcher::new(Hub::new(Ime::with_engine(engine)))
    }

    fn req(sid: u64, code: i64) -> ClientMessage {
        ClientMessage::Req { sid, json: json!({"method": "onKeyDown", "seqNum": 1, "keyCode": code}) }
    }

    fn rect_msg(sid: u64, left: i32) -> ClientMessage {
        ClientMessage::Rect { sid, left, top: 2, right: left + 10, bottom: 22 }
    }

    fn resp_json(o: &Outcome) -> &Value {
        let Some(ServerMessage::Resp { json, .. }) = &o.reply else { panic!("应有响应") };
        json
    }

    #[test]
    fn open无回复无ui() {
        let mut d = dispatcher();
        let o = d.handle(ClientMessage::Open { sid: 1, host: "notepad.exe".into() });
        assert!(o.reply.is_none() && o.ui.is_none());
    }

    #[test]
    fn 按键产生响应与显示指令且用已知光标矩形() {
        let mut d = dispatcher();
        d.handle(rect_msg(1, 100));
        d.handle(req(1, N));
        let o = d.handle(req(1, I));
        assert_eq!(resp_json(&o)["compositionString"], "ni");
        let Some(UiCmd::Show(v)) = o.ui else { panic!("应为 Show") };
        assert_eq!(v.labels[0], "1 你");
        assert_eq!(v.rect.left, 100);
    }

    #[test]
    fn 只有所有者的rect才移动窗口() {
        let mut d = dispatcher();
        d.handle(req(1, N));
        d.handle(req(1, I)); // 会话 1 显示候选窗,成为所有者
        assert_eq!(d.handle(rect_msg(1, 50)).ui, Some(UiCmd::Move(Rect { left: 50, top: 2, right: 60, bottom: 22 })));
        assert_eq!(d.handle(rect_msg(2, 70)).ui, None);
    }

    #[test]
    fn 只有所有者的隐藏生效() {
        let mut d = dispatcher();
        d.handle(req(1, N));
        d.handle(req(1, I)); // 会话 1 显示候选窗
        // 会话 2 输入词典里没有的 q(无候选 → showCandidates=false),它不是所有者,指令被丢弃
        assert_eq!(d.handle(req(2, Q)).ui, None);
        assert_eq!(d.handle(req(2, VK_ESCAPE)).ui, None);
        assert_eq!(d.handle(req(1, VK_ESCAPE)).ui, Some(UiCmd::Hide));
        // 所有者已清空,再来一次隐藏不再产生指令
        assert_eq!(d.handle(req(1, VK_ESCAPE)).ui, None);
    }

    #[test]
    fn 另一会话显示后所有权转移() {
        let mut d = dispatcher();
        d.handle(req(1, N));
        d.handle(req(1, I));
        d.handle(req(2, N));
        let o = d.handle(req(2, I));
        assert!(matches!(o.ui, Some(UiCmd::Show(_))));
        assert_eq!(d.handle(req(1, VK_ESCAPE)).ui, None); // 旧会话收尾不关新窗口
        assert_eq!(d.handle(req(2, VK_ESCAPE)).ui, Some(UiCmd::Hide));
    }

    #[test]
    fn 关闭所有者会话隐藏_关闭其他会话不动() {
        let mut d = dispatcher();
        d.handle(req(1, N));
        d.handle(req(1, I));
        assert_eq!(d.handle(ClientMessage::Close { sid: 2 }).ui, None);
        assert_eq!(d.handle(ClientMessage::Close { sid: 1 }).ui, Some(UiCmd::Hide));
    }

    #[test]
    fn run_pending按序处理并回复() {
        let mut d = dispatcher();
        let (tx, rx) = channel::<Job>();
        let (rtx, rrx) = channel::<ServerMessage>();
        tx.send(Job { msg: req(1, N), reply: rtx.clone() }).unwrap();
        tx.send(Job { msg: req(1, I), reply: rtx.clone() }).unwrap();
        tx.send(Job { msg: ClientMessage::Close { sid: 1 }, reply: rtx }).unwrap();
        let mut cmds = Vec::new();
        assert_eq!(run_pending(&rx, &mut d, |c| cmds.push(c)), 3);
        assert_eq!(run_pending(&rx, &mut d, |_| panic!("队列已空")), 0);
        let replies: Vec<_> = rrx.try_iter().collect();
        assert_eq!(replies.len(), 2); // Close 无回复
        assert!(matches!(cmds.as_slice(), [UiCmd::Show(_), UiCmd::Hide]));
    }
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -p pyime-server core > /tmp/a2t3.log 2>&1; grep -E "test result|FAILED|panicked|error" /tmp/a2t3.log | head`
Expected: 编译通过(`pyime-engine` 经 `pyime-backend` 传递可见?若报找不到 `pyime_engine` crate,在 `crates/pyime-server/Cargo.toml` 的 `[dev-dependencies]` 加 `pyime-engine = { path = "../pyime-engine" }` 并在报告说明);7 个用例中除 `open无回复无ui` 外 FAILED。

- [ ] **Step 4: 实现**

把 `handle` 与 `run_pending` 的桩替换为:

```rust
    pub fn handle(&mut self, msg: ClientMessage) -> Outcome {
        match msg {
            ClientMessage::Open { sid, .. } => {
                self.hub.open(sid);
                Outcome { reply: None, ui: None }
            }
            ClientMessage::Req { sid, json } => {
                let resp = self.hub.handle(sid, &json);
                let ui = ui_from_resp(&resp, self.rects.get(&sid).copied());
                let ui = self.gate(sid, ui);
                Outcome { reply: Some(ServerMessage::Resp { sid, json: resp }), ui }
            }
            ClientMessage::Rect { sid, left, top, right, bottom } => {
                let rect = Rect { left, top, right, bottom };
                self.rects.insert(sid, rect);
                Outcome { reply: None, ui: self.gate(sid, Some(UiCmd::Move(rect))) }
            }
            ClientMessage::Close { sid } => {
                self.hub.close(sid);
                self.rects.remove(&sid);
                Outcome { reply: None, ui: self.gate(sid, Some(UiCmd::Hide)) }
            }
        }
    }

    /// 按候选窗归属过滤指令:Show 夺取所有权;Hide 仅所有者生效并释放所有权;Move 仅所有者生效。
    fn gate(&mut self, sid: u64, ui: Option<UiCmd>) -> Option<UiCmd> {
        match ui? {
            show @ UiCmd::Show(_) => {
                self.shown_by = Some(sid);
                Some(show)
            }
            UiCmd::Hide if self.shown_by == Some(sid) => {
                self.shown_by = None;
                Some(UiCmd::Hide)
            }
            mv @ UiCmd::Move(_) if self.shown_by == Some(sid) => Some(mv),
            _ => None,
        }
    }
```

`run_pending`:

```rust
pub fn run_pending(rx: &Receiver<Job>, d: &mut Dispatcher, mut on_ui: impl FnMut(UiCmd)) -> usize {
    let mut n = 0;
    while let Ok(job) = rx.try_recv() {
        n += 1;
        let out = d.handle(job.msg);
        if let Some(reply) = out.reply {
            let _ = job.reply.send(reply); // I/O 线程已退出时发送失败,忽略
        }
        if let Some(ui) = out.ui {
            on_ui(ui);
        }
    }
    n
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p pyime-server > /tmp/a2t3.log 2>&1; grep -E "test result|FAILED|panicked" /tmp/a2t3.log`
Expected: `13 passed`(ui 6 + core 7)。若某用例因 `Session` 真实行为(如 Esc 响应是否带 `showCandidates:false`)与期望不符,先读 `crates/pyime-backend/src/session/tests.rs` 与 `session.rs` 确认,只改测试期望并说明依据,不得改 `Session`/`Hub`、不得削弱断言。

- [ ] **Step 6: 全量回归**

Run: `cargo test --workspace > /tmp/a2t3all.log 2>&1; grep -E "^test result|FAILED|error" /tmp/a2t3all.log`
Expected: 全部 ok;backend lib 40 + bin 7、engine 42、protocol 8、tsf 2 数量不变。

- [ ] **Step 7: Commit**(`Cargo.lock` 若有变化不提交,报告说明)

```bash
git add crates/pyime-server
git commit -m "pyime-server:Dispatcher 与 run_pending,候选窗归属规则

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 4: Windows 底座 —— 用户 SID、单实例、管道服务

**Files:**
- Modify: `crates/pyime-protocol/Cargo.toml`, `crates/pyime-protocol/src/lib.rs`(加 `current_user_sid`)
- Modify: `crates/pyime-server/Cargo.toml`(加 windows 依赖)、`crates/pyime-server/src/lib.rs`(加 `#[cfg(windows)] pub mod win;`)
- Create: `crates/pyime-server/src/win/mod.rs`(本任务只放 `mod single; mod pipe;`)、`win/single.rs`、`win/pipe.rs`

**Interfaces:**
- Consumes:Task 3 的 `core::Job`;`pyime_protocol::{ClientMessage, ServerMessage, read_frame, write_frame}`。
- Produces:`pyime_protocol::current_user_sid() -> Option<String>`(仅 Windows);`win::single::acquire() -> Option<SingleInstance>`(已有实例返回 `None`;`SingleInstance` 析构时关闭句柄);`win::pipe::WakeFn = Arc<dyn Fn() + Send + Sync>`;`win::pipe::serve(name: &str, tx: Sender<Job>, wake: WakeFn) -> io::Result<()>`(同步创建首个管道实例,失败即返回 Err;成功后在后台线程接受连接,每条连接一个 I/O 线程)。
- I/O 线程语义:读到 `ClientMessage` 就 `Job` 入队并 `wake()`;只有 `Req` 等待响应并写回;读失败/连接断开时,为该连接上出现过的每个会话号补发 `Close`。

- [ ] **Step 1: protocol 加 current_user_sid**

`crates/pyime-protocol/Cargo.toml` 追加:

```toml

[target.'cfg(windows)'.dependencies]
windows = { version = "0.62.2", features = [
    "Win32_Foundation",
    "Win32_Security",
    "Win32_Security_Authorization",
    "Win32_System_Threading",
] }
```

`src/lib.rs` 末尾(`#[cfg(test)]` 之前)追加:

```rust
/// 当前进程用户的 SID 字符串(如 `S-1-5-21-...`),用于拼管道名。仅 Windows。
#[cfg(windows)]
pub fn current_user_sid() -> Option<String> {
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::core::PWSTR;
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        let got = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), len, &mut len);
        let _ = CloseHandle(token);
        got.ok()?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut wide = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut wide).ok()?;
        let out = wide.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(wide.0.cast())));
        out
    }
}
```

- [ ] **Step 2: server 的 Cargo.toml 与 lib.rs**

`crates/pyime-server/Cargo.toml` 追加:

```toml

[target.'cfg(windows)'.dependencies]
windows = { version = "0.62.2", features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_Security",
    "Win32_Security_Authorization",
    "Win32_Storage_FileSystem",
    "Win32_System_LibraryLoader",
    "Win32_System_Pipes",
    "Win32_System_Threading",
    "Win32_UI_HiDpi",
    "Win32_UI_WindowsAndMessaging",
] }
```

`src/lib.rs` 追加:

```rust

#[cfg(windows)]
pub mod win;
```

`src/win/mod.rs`:

```rust
//! Windows 专有部分:单实例、管道服务、候选窗、消息循环。

pub mod pipe;
pub mod single;
```

- [ ] **Step 3: 写 win/single.rs**

```rust
//! 单实例:命名互斥体。已有实例在运行时 `acquire` 返回 `None`。

use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

pub struct SingleInstance(HANDLE);

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub fn acquire() -> Option<SingleInstance> {
    let handle = unsafe { CreateMutexW(None, true, w!("Local\\PYIME-Server")) }.ok()?;
    // 必须紧接着取 LastError:互斥体已存在时 CreateMutexW 成功但置此错误码。
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        let _ = unsafe { CloseHandle(handle) };
        return None;
    }
    Some(SingleInstance(handle))
}
```

- [ ] **Step 4: 写 win/pipe.rs**

```rust
//! 命名管道服务:首个实例同步创建(失败说明已有 server),之后后台接受连接,每条连接一个 I/O 线程。

use crate::core::Job;
use pyime_protocol::{read_frame, write_frame, ClientMessage};
use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use windows::Win32::Foundation::{
    GetLastError, LocalFree, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, INVALID_HANDLE_VALUE,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::core::{w, PCWSTR};

/// 唤醒主线程消息循环(由 win::run 提供,内部是 PostMessageW)。
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// 当前用户与 ALL APPLICATION PACKAGES 可读写,低完整性标签(AppContainer 宿主也能连)。
const SDDL: PCWSTR = w!("D:(A;;GA;;;OW)(A;;GRGW;;;AC)S:(ML;;NW;;;LW)");

fn create_instance(name: &str, first: bool) -> io::Result<HANDLE> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(SDDL, SDDL_REVISION_1, &mut sd, None) }
        .map_err(io::Error::other)?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    };
    let mut open = PIPE_ACCESS_DUPLEX;
    if first {
        open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(wide.as_ptr()),
            open,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            64 * 1024,
            64 * 1024,
            0,
            Some(&sa),
        )
    };
    let _ = unsafe { LocalFree(Some(HLOCAL(sd.0))) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(handle)
}

pub fn serve(name: &str, tx: Sender<Job>, wake: WakeFn) -> io::Result<()> {
    let first = create_instance(name, true)?.0 as isize;
    let name = name.to_string();
    std::thread::spawn(move || {
        let mut next = Some(first);
        loop {
            let raw = match next.take() {
                Some(h) => h,
                None => match create_instance(&name, false) {
                    Ok(h) => h.0 as isize,
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                },
            };
            let handle = HANDLE(raw as *mut _);
            let connected = unsafe { ConnectNamedPipe(handle, None) }.is_ok()
                || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            if !connected {
                let _ = unsafe { windows::Win32::Foundation::CloseHandle(handle) };
                continue;
            }
            let file = unsafe { File::from_raw_handle(handle.0 as RawHandle) };
            let (tx, wake) = (tx.clone(), wake.clone());
            std::thread::spawn(move || serve_conn(file, tx, wake));
        }
    });
    Ok(())
}

fn session_of(msg: &ClientMessage) -> u64 {
    match msg {
        ClientMessage::Open { sid, .. }
        | ClientMessage::Req { sid, .. }
        | ClientMessage::Rect { sid, .. }
        | ClientMessage::Close { sid } => *sid,
    }
}

fn serve_conn(mut file: File, tx: Sender<Job>, wake: WakeFn) {
    let mut sids: HashSet<u64> = HashSet::new();
    while let Ok(msg) = read_frame::<_, ClientMessage>(&mut file) {
        let sid = session_of(&msg);
        match &msg {
            ClientMessage::Close { .. } => {
                sids.remove(&sid);
            }
            _ => {
                sids.insert(sid);
            }
        }
        let wants_reply = matches!(msg, ClientMessage::Req { .. });
        let (reply, rx) = channel();
        if tx.send(Job { msg, reply }).is_err() {
            break;
        }
        wake();
        if wants_reply {
            match rx.recv() {
                Ok(resp) if write_frame(&mut file, &resp).is_ok() => {}
                _ => break,
            }
        }
    }
    // 连接断开:为遗留会话补发 Close,避免 Hub 里残留状态。
    for sid in sids {
        let (reply, _rx) = channel();
        let _ = tx.send(Job { msg: ClientMessage::Close { sid }, reply });
        wake();
    }
}
```

- [ ] **Step 5: 交叉编译检查**

Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-protocol -p pyime-server > /tmp/a2t4.log 2>&1; tail -40 /tmp/a2t4.log`
Expected: `Finished`。windows 0.62.2 的签名(`PSECURITY_DESCRIPTOR`/`HLOCAL` 的构造、`CreateNamedPipeW` 参数类型、`FILE_FLAG_FIRST_PIPE_INSTANCE` 与 `PIPE_ACCESS_DUPLEX` 的类型是否能 `|`、`bInheritHandle` 类型、`Option<&SECURITY_ATTRIBUTES>` 等)若不符,按编译器提示做最小修正并在报告列出,保持行为不变:首实例带 `FILE_FLAG_FIRST_PIPE_INSTANCE`、拒绝远程客户端、SDDL 不变、读失败时补发 `Close`。

- [ ] **Step 6: Linux 回归**

Run: `cargo test --workspace > /tmp/a2t4all.log 2>&1; grep -E "^test result|FAILED|error" /tmp/a2t4all.log`
Expected: 全部 ok,数量:backend lib 40 + bin 7、engine 42、protocol 8、server 13、tsf 2。

- [ ] **Step 7: Commit**(`Cargo.lock` 不提交,报告说明)

```bash
git add crates/pyime-protocol crates/pyime-server
git commit -m "pyime-server:用户 SID、单实例与命名管道服务

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 5: 主线程消息循环与入口(候选窗先用日志桩)

**Files:**
- Modify: `crates/pyime-server/src/win/mod.rs`(整体替换)
- Create: `crates/pyime-server/src/win/cand.rs`(日志桩,Task 6 替换)
- Modify: `crates/pyime-server/src/main.rs`(整体替换)

**Interfaces:**
- Consumes:Task 3 的 `Dispatcher/Job/run_pending`;Task 4 的 `single::acquire`、`pipe::serve/WakeFn`、`pyime_protocol::{current_user_sid, pipe_name}`;`pyime_backend::{hub::Hub, ime::Ime, log}`。
- Produces:`win::run()`(进程入口,阻塞到消息循环结束);`win::cand::CandWindow::new() -> CandWindow`、`CandWindow::apply(&mut self, UiCmd)`(Task 6 保持此签名替换为真实窗口)。

- [ ] **Step 1: 写 win/cand.rs(桩)**

```rust
//! 候选窗。本版本是日志桩,Task 6 替换为 GDI 实现(签名不变)。

use crate::ui::UiCmd;
use pyime_backend::log;

pub struct CandWindow;

impl CandWindow {
    pub fn new() -> CandWindow {
        CandWindow
    }

    pub fn apply(&mut self, cmd: UiCmd) {
        log::line(&format!("ui: {cmd:?}"));
    }
}
```

- [ ] **Step 2: 替换 win/mod.rs**

```rust
//! Windows 专有部分:单实例、管道服务、候选窗、消息循环。

pub mod cand;
pub mod pipe;
pub mod single;

use crate::core::{run_pending, Dispatcher, Job};
use cand::CandWindow;
use pyime_backend::hub::Hub;
use pyime_backend::ime::Ime;
use pyime_backend::log;
use pipe::WakeFn;
use pyime_protocol::{current_user_sid, pipe_name};
use std::cell::RefCell;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, RegisterClassW,
    TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WNDCLASSW,
};
use windows::core::w;

/// 主线程独占的状态:Hub(Rc,不可跨线程)、候选窗都在这里。
struct State {
    rx: Receiver<Job>,
    disp: Dispatcher,
    cand: CandWindow,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_APP {
        STATE.with(|s| {
            if let Some(State { rx, disp, cand }) = s.borrow_mut().as_mut() {
                run_pending(rx, disp, |cmd| cand.apply(cmd));
            }
        });
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub fn run() {
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    log::init();
    log::install_panic_hook();
    let Some(_single) = single::acquire() else {
        log::line("已有 pyime-server 在运行,退出");
        return;
    };

    // 仅用于被唤醒的消息窗口
    let hinst = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    let class = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        lpszClassName: w!("PYIMEServerMsg"),
        ..Default::default()
    };
    unsafe { RegisterClassW(&class) };
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("PYIMEServerMsg"),
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst.into()),
            None,
        )
    };
    let Ok(hwnd) = hwnd else {
        log::line("创建消息窗口失败,退出");
        return;
    };
    let hwnd_raw = hwnd.0 as isize;
    let post = move || {
        let _ = unsafe { PostMessageW(Some(HWND(hwnd_raw as *mut _)), WM_APP, WPARAM(0), LPARAM(0)) };
    };
    let wake: WakeFn = Arc::new(post);

    let (tx, rx) = channel::<Job>();
    let name = pipe_name(&current_user_sid().unwrap_or_default());
    if let Err(e) = pipe::serve(&name, tx, wake) {
        log::line(&format!("创建管道 {name} 失败(可能已有实例): {e}"));
        return;
    }
    let ime = Ime::load();
    STATE.with(|s| {
        *s.borrow_mut() = Some(State { rx, disp: Dispatcher::new(Hub::new(ime)), cand: CandWindow::new() });
    });
    // STATE 就绪前到达的唤醒会被忽略,这里补一次,排空期间积压的请求。
    let _ = unsafe { PostMessageW(Some(hwnd), WM_APP, WPARAM(0), LPARAM(0)) };
    log::line(&format!("pyime-server 就绪,管道 {name}"));

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
```

- [ ] **Step 3: 替换 main.rs**

```rust
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    pyime_server::win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("pyime-server 目前只支持 Windows");
}
```

- [ ] **Step 4: 交叉编译检查并按编译器提示最小修正**

Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-server > /tmp/a2t5.log 2>&1; tail -40 /tmp/a2t5.log`
Expected: `Finished`。窗口类/`CreateWindowExW` 参数类型(`hInstance` 的类型转换、`Option<HWND>` 等)不符时按提示修正;保持行为:消息窗口为 `HWND_MESSAGE`、STATE 就绪后补一次唤醒、`pipe::serve` 失败即退出。

- [ ] **Step 5: Linux 回归**

Run: `cargo test --workspace > /tmp/a2t5all.log 2>&1; grep -E "^test result|FAILED|error" /tmp/a2t5all.log`
Expected: 全部 ok(server 13)。

- [ ] **Step 6: Commit**(`Cargo.lock` 不提交)

```bash
git add crates/pyime-server
git commit -m "pyime-server:消息循环与入口,候选窗先用日志桩

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 6: GDI 候选窗

**Files:**
- Modify: `crates/pyime-server/src/win/cand.rs`(整体替换)

**Interfaces:**
- Consumes:Task 2 的 `UiCmd/CandView/Rect`。
- Produces:`CandWindow::new() -> CandWindow`、`apply(&mut self, UiCmd)`(签名与桩一致);`Show` 在 `rect.bottom + 4` 处显示(越出工作区则翻到光标上方/左移),`Move` 仅在可见时重新定位,`Hide` 隐藏。样式 `WS_POPUP` + `WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST`;`WM_MOUSEACTIVATE` 返回 `MA_NOACTIVATE`;横排,每项 `"序号 文字"`,当前项浅蓝底;字号按 `候选字号 × DPI / 72` 像素。

- [ ] **Step 1: 整体替换 cand.rs**

```rust
//! 候选窗(GDI):横排、序号、当前项高亮;不抢焦点,不进任务栏。

use crate::ui::{CandView, Rect, UiCmd};
use pyime_backend::log;
use std::cell::RefCell;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect,
    GetDC, GetMonitorInfoW, GetTextExtentPoint32W, InvalidateRect, MonitorFromPoint, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, FW_NORMAL, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
    TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, SetWindowPos, ShowWindow, HWND_TOPMOST,
    MA_NOACTIVATE, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_MOUSEACTIVATE,
    WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{w, PCWSTR};

const PAD_X: i32 = 8;
const PAD_Y: i32 = 5;
const MARGIN: i32 = 4;
const GAP: i32 = 4;

thread_local! {
    static VIEW: RefCell<Option<CandView>> = const { RefCell::new(None) };
}

pub struct CandWindow {
    hwnd: HWND,
    visible: bool,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn make_font(view: &CandView, dpi: u32) -> HFONT {
    let height = -(view.font_size * dpi as i32 / 72);
    let face = wide(&view.font);
    unsafe {
        CreateFontW(height, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, DEFAULT_CHARSET, 0, 0,
            CLEARTYPE_QUALITY, 0, PCWSTR(face.as_ptr()))
    }
}

/// 每项的横向区间 [left,right) 与整窗尺寸。
fn layout(view: &CandView, dpi: u32) -> (Vec<(i32, i32)>, SIZE) {
    let hdc = unsafe { GetDC(None) };
    let font = make_font(view, dpi);
    let old = unsafe { SelectObject(hdc, font.into()) };
    let (mut x, mut text_h, mut spans) = (MARGIN, 0, Vec::new());
    for label in &view.labels {
        let text = wide(label);
        let mut size = SIZE::default();
        let _ = unsafe { GetTextExtentPoint32W(hdc, &text[..text.len() - 1], &mut size) };
        text_h = text_h.max(size.cy);
        spans.push((x, x + size.cx + 2 * PAD_X));
        x += size.cx + 2 * PAD_X + GAP;
    }
    unsafe {
        SelectObject(hdc, old);
        let _ = DeleteObject(font.into());
        ReleaseDC(None, hdc);
    }
    (spans, SIZE { cx: x - GAP + MARGIN, cy: text_h + 2 * PAD_Y + 2 * MARGIN })
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    VIEW.with(|v| {
        let Some(view) = v.borrow().clone() else { return };
        let dpi = unsafe { GetDPIForWindowOr96(hwnd) };
        let (spans, size) = layout(&view, dpi);
        let whole = RECT { left: 0, top: 0, right: size.cx, bottom: size.cy };
        unsafe {
            let white = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00FF_FFFF));
            let border = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00B0_B0B0));
            let hi = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00FF_E4CC)); // BGR:浅蓝
            FillRect(hdc, &whole, white);
            FrameRect(hdc, &whole, border);
            let font = make_font(&view, dpi);
            let old = SelectObject(hdc, font.into());
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(0));
            for (i, label) in view.labels.iter().enumerate() {
                let (l, r) = spans[i];
                let mut item = RECT { left: l, top: MARGIN, right: r, bottom: size.cy - MARGIN };
                if i == view.cursor {
                    FillRect(hdc, &item, hi);
                }
                let mut text = wide(label);
                let n = text.len() - 1;
                DrawTextW(hdc, &mut text[..n], &mut item, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
            }
            SelectObject(hdc, old);
            for obj in [font.into(), white.into(), border.into(), hi.into()] {
                let _ = DeleteObject(obj);
            }
        }
    });
    let _ = unsafe { EndPaint(hwnd, &ps) };
}

#[allow(non_snake_case)]
unsafe fn GetDPIForWindowOr96(hwnd: HWND) -> u32 {
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => 96,
        d => d,
    }
}

impl CandWindow {
    pub fn new() -> CandWindow {
        let hinst = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: w!("PYIMECandidate"),
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("PYIMECandidate"),
                w!(""),
                WS_POPUP,
                0, 0, 0, 0,
                None, None, Some(hinst.into()), None,
            )
        }
        .unwrap_or_else(|e| {
            log::line(&format!("创建候选窗失败: {e}"));
            HWND::default()
        });
        CandWindow { hwnd, visible: false }
    }

    pub fn apply(&mut self, cmd: UiCmd) {
        if self.hwnd.0.is_null() {
            return;
        }
        match cmd {
            UiCmd::Hide => {
                let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
                self.visible = false;
            }
            UiCmd::Show(view) => {
                let rect = view.rect;
                VIEW.with(|v| *v.borrow_mut() = Some(view));
                self.place(rect);
                let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
                let _ = unsafe { InvalidateRect(Some(self.hwnd), None, true) };
                self.visible = true;
            }
            UiCmd::Move(rect) if self.visible => self.place(rect),
            UiCmd::Move(_) => {}
        }
    }

    /// 放在光标矩形下方;越出所在显示器工作区则翻到上方/左移。
    fn place(&self, rect: Rect) {
        let Some(view) = VIEW.with(|v| v.borrow().clone()) else { return };
        let dpi = unsafe { GetDPIForWindowOr96(self.hwnd) };
        let (_, size) = layout(&view, dpi);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let monitor = unsafe { MonitorFromPoint(POINT { x: rect.left, y: rect.bottom }, MONITOR_DEFAULTTONEAREST) };
        let work = if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            info.rcWork
        } else {
            RECT { left: 0, top: 0, right: i32::MAX / 2, bottom: i32::MAX / 2 }
        };
        let mut x = rect.left;
        let mut y = rect.bottom + 4;
        if x + size.cx > work.right {
            x = work.right - size.cx;
        }
        if y + size.cy > work.bottom {
            y = rect.top - size.cy - 4;
        }
        x = x.max(work.left);
        y = y.max(work.top);
        let _ = unsafe { SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, size.cx, size.cy, SWP_NOACTIVATE) };
    }
}
```

- [ ] **Step 2: 交叉编译检查并按编译器提示最小修正**

Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-server > /tmp/a2t6.log 2>&1; tail -50 /tmp/a2t6.log`
Expected: `Finished`。GDI 相关签名(`CreateFontW` 参数类型、`DrawTextW` 的 `&mut [u16]`、`FillRect` 画刷类型、`HGDIOBJ` 转换、`GetDpiForWindow` 所需 feature)大概率需要调整;按提示修正,必要时给 windows 依赖补 feature(补在 `crates/pyime-server/Cargo.toml`,报告说明)。保持行为:不抢焦点样式不变、横排+序号+当前项浅蓝底、越界翻转与左移。

- [ ] **Step 3: Linux 回归**

Run: `cargo test --workspace > /tmp/a2t6all.log 2>&1; grep -E "^test result|FAILED|error" /tmp/a2t6all.log`
Expected: 全部 ok(server 13)。

- [ ] **Step 4: Commit**(`Cargo.lock` 不提交)

```bash
git add crates/pyime-server
git commit -m "pyime-server:GDI 候选窗(横排、序号、高亮、不抢焦点)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 7: 探针、构建脚本与产物自检

**Files:**
- Create: `crates/pyime-server/src/bin/probe.rs`, `scripts/build-server.sh`
- Modify: `crates/pyime-server/Cargo.toml`(显式声明两个 bin)

**Interfaces:**
- Consumes:`pyime_protocol::{pipe_name, current_user_sid, read_frame, write_frame, ClientMessage, ServerMessage}`。
- Produces:`pyime-probe.exe <按键序列> [停留秒数]`:模拟 TSF 客户端——连管道,发 `Open`、`Rect(400,300,410,322)`,逐键发 `Req{onKeyDown}` 并打印响应,停留后发 `Close`。按键映射:`a–z`→大写 VK,`0–9`→ASCII,空格→32,`<`→Esc(27),`-`→189,`=`→187。`scripts/build-server.sh`:单次 `RUSTFLAGS` 交叉编译并检查两个 exe 不依赖 libgcc/libwinpthread/libstdc。

- [ ] **Step 1: Cargo.toml 显式声明 bin**

在 `crates/pyime-server/Cargo.toml` 的 `[dependencies]` 之前插入:

```toml
[[bin]]
name = "pyime-server"
path = "src/main.rs"

[[bin]]
name = "pyime-probe"
path = "src/bin/probe.rs"

```

- [ ] **Step 2: 写 src/bin/probe.rs**

```rust
//! 探针:模拟 TSF 客户端连 pyime-server 的管道,发会话与按键并打印响应。仅用于实机验证。
//! 用法:pyime-probe <按键序列,如 ni1> [停留秒数,默认 5]

use pyime_protocol::{pipe_name, read_frame, write_frame, ClientMessage, ServerMessage};
use serde_json::json;
use std::fs::OpenOptions;
use std::time::Duration;

fn vk(c: char) -> Option<i64> {
    match c {
        'a'..='z' => Some(c.to_ascii_uppercase() as i64),
        '0'..='9' => Some(c as i64),
        ' ' => Some(32),
        '<' => Some(27),
        '-' => Some(189),
        '=' => Some(187),
        _ => None,
    }
}

#[cfg(windows)]
fn user() -> String {
    pyime_protocol::current_user_sid().unwrap_or_default()
}

#[cfg(not(windows))]
fn user() -> String {
    String::new()
}

fn main() -> std::io::Result<()> {
    let keys = std::env::args().nth(1).unwrap_or_else(|| "ni".into());
    let hold: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let name = pipe_name(&user());
    let mut pipe = OpenOptions::new().read(true).write(true).open(&name)?;
    write_frame(&mut pipe, &ClientMessage::Open { sid: 1, host: "probe.exe".into() })?;
    write_frame(&mut pipe, &ClientMessage::Rect { sid: 1, left: 400, top: 300, right: 410, bottom: 322 })?;
    for c in keys.chars() {
        let Some(code) = vk(c) else {
            eprintln!("忽略按键 {c:?}");
            continue;
        };
        let json = json!({"method": "onKeyDown", "seqNum": 1, "keyCode": code});
        write_frame(&mut pipe, &ClientMessage::Req { sid: 1, json })?;
        let ServerMessage::Resp { json, .. } = read_frame(&mut pipe)?;
        println!("{c} -> {json}");
    }
    std::thread::sleep(Duration::from_secs(hold));
    write_frame(&mut pipe, &ClientMessage::Close { sid: 1 })?;
    Ok(())
}
```

- [ ] **Step 3: 写 scripts/build-server.sh 并 `chmod +x`**

```bash
#!/usr/bin/env bash
# 交叉编译 pyime-server 与 pyime-probe(x86_64-pc-windows-gnu)并检查依赖。
# panic=abort 与静态链接只在此处传入,不改 workspace profile。可用 PYIME_RUSTFLAGS 覆盖。
set -euo pipefail
cd "$(dirname "$0")/.."
FLAGS="${PYIME_RUSTFLAGS:--C panic=abort -C target-feature=+crt-static}"
RUSTFLAGS="$FLAGS" cargo build --release --target x86_64-pc-windows-gnu -p pyime-server
for exe in pyime-server pyime-probe; do
  f="target/x86_64-pc-windows-gnu/release/$exe.exe"
  test -f "$f" || { echo "找不到 $f"; exit 1; }
  OUT="$(x86_64-w64-mingw32-objdump -p "$f")"
  if grep -E 'DLL Name:.*(libgcc|libwinpthread|libstdc)' <<<"$OUT"; then
    echo "FAIL: $f 依赖了不应出现的运行库"; exit 1
  fi
  echo "OK: $f"
done
```

- [ ] **Step 4: 构建与自检**

Run: `scripts/build-server.sh > /tmp/a2t7.log 2>&1; tail -8 /tmp/a2t7.log`
Expected: 末两行 `OK: ...pyime-server.exe`、`OK: ...pyime-probe.exe`。若报显式 `[[bin]]` 与自动发现的重复目标,按 cargo 提示处理(例如给自动发现的删除或调整 path),在报告说明。若依赖了 libgcc/winpthread,按 `crt-static` 之外的备选 `-C link-arg=-static-libgcc -C link-arg=-static` 试(同 spike),把可行参数写回脚本默认值。

- [ ] **Step 5: Linux 回归**

Run: `cargo test --workspace > /tmp/a2t7all.log 2>&1; grep -E "^test result|FAILED|error" /tmp/a2t7all.log`
Expected: 全部 ok。Linux 上 `cargo build -p pyime-server` 也应通过(`probe` 在非 Windows 下仅编译)。

- [ ] **Step 6: Commit**(`Cargo.lock` 不提交)

```bash
git add crates/pyime-server scripts/build-server.sh
git commit -m "pyime-server:探针 pyime-probe 与构建脚本

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 8: server 的 Windows 实机验收(用户操作)与记录

**Files:**
- Modify: `docs/superpowers/specs/2026-10-14-tsf-frontend-a-design.md`(末尾追加"A2 结果"一节)

- [ ] **Step 1: 部署**:Windows 建 `C:\PYIME\`,放入 `pyime-server.exe`、`pyime-probe.exe`(来自 `target/x86_64-pc-windows-gnu/release/`)与 `pyime.dict`(来自 `installer/pyime.dict`)。
- [ ] **Step 2: 启动与单实例**:双击 `pyime-server.exe`(无窗口)。`%LOCALAPPDATA%\PYIME\log.txt` 应出现 `pyime-server 就绪,管道 \\.\pipe\pyime-S-1-5-...`;再启动一次应立即退出,日志有"已有 pyime-server 在运行,退出"。
- [ ] **Step 3: 探针出候选窗**:PowerShell 中 `C:\PYIME\pyime-probe.exe ni 8`。期望打印每个键的响应(`n`、`i` 之后含 `compositionString`、`candidateList`),屏幕约 (400,300) 下方出现横排候选窗 `1 你 2 泥 …`,第一项浅蓝底;8 秒后探针退出、窗口消失。
- [ ] **Step 4: 选词与 Esc**:`pyime-probe.exe ni1 3` 的最后一行响应含 `"commitString":"你"` 且窗口随即隐藏;`pyime-probe.exe ni< 3` 的 Esc 响应 `compositionString` 为空。
- [ ] **Step 5: 不抢焦点**:打开记事本并点进去,在另一个终端执行 `Start-Sleep 3; C:\PYIME\pyime-probe.exe ni 10`,3 秒内点回记事本;候选窗出现后记事本光标应保持闪烁、可继续打字。
- [ ] **Step 6: 并发与断线**:同时开两个终端各跑 `pyime-probe.exe ni 10`,两者都应拿到响应且互不报错;用 Ctrl+C 中断其中一个,窗口按归属规则隐藏/保留;结束 `pyime-server.exe` 进程后探针应报"找不到文件",重新启动 server 后恢复正常。
- [ ] **Step 7: 记录**:把各项通过与否、观察到的问题(窗口位置偏差、字体、DPI、闪烁、延迟)追加到 spec 末尾"## A2 结果",Commit:

```bash
git add docs/superpowers/specs/2026-10-14-tsf-frontend-a-design.md
git commit -m "spec:A2 server 实机验收结果记录

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 8:** 向用户汇报;A3(TSF 客户端扩展)的计划在 A2 通过后另出,push / 合并均先问用户。
