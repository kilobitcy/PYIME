# 子项目 A1:协议 crate、backend 库化与 Hub Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完成子项目 A 中全部可在 Linux 上自动测试的部分:`pyime-protocol`(帧与信封)、`pyime-backend` 库化、`Hub`(多会话中枢)。

**Architecture:** 新增纯 Rust crate `pyime-protocol`;`pyime-backend` 增加 `lib.rs`,把会话相关模块作为库导出,PIME 版 `main.rs` 改用库而行为不变;库内新增 `hub` 模块按会话号持有 `Session`。`Session` 一行不改。server(A2)与 TSF 管道客户端(A3)另出计划。

**Tech Stack:** Rust 2021,`serde`/`serde_json`,`cargo test`。

**Spec:** `docs/superpowers/specs/2026-10-14-tsf-frontend-a-design.md`

## Global Constraints

- 分支 `spike/tsf-gnu`,不 push;回复、commit、注释一律简体中文;commit 末尾加 `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`。
- `Session`(`crates/pyime-backend/src/session.rs`、`session/tests.rs`)一行不改;PIME 版 `pyime-backend` 的运行行为不变,现有全部测试保持通过且数量不减。
- Windows 专有依赖不得进入 `pyime-protocol` 与 `pyime-backend`。
- 帧格式:4 字节小端长度前缀 + JSON,单帧上限 1 MiB;管道名 `\\.\pipe\pyime-<用户 SID>`。
- 信封:`ClientMessage{Open{sid,host}, Req{sid,json}, Rect{sid,left,top,right,bottom}, Close{sid}}`,`ServerMessage{Resp{sid,json}}`,均以 `"type"` 字段区分。
- 不改 workspace 根 `[profile]`;`Cargo.lock` 变化须一并提交(先 `git diff Cargo.lock` 确认仅为新增/同步依赖)。
- 本机无 clippy,不跑;不得为过测试削弱既有测试。

## File Structure

- `Cargo.toml`(根):members 加 `crates/pyime-protocol`。
- `crates/pyime-protocol/Cargo.toml`、`src/lib.rs`:信封类型、帧编解码、管道名。
- `crates/pyime-backend/src/lib.rs`:新增,导出库模块。
- `crates/pyime-backend/src/main.rs`:改为 `use pyime_backend::{...}`。
- `crates/pyime-backend/src/hub.rs`:新增,多会话中枢及单测。

---

### Task 1: pyime-protocol(帧与信封,TDD)

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/pyime-protocol/Cargo.toml`, `crates/pyime-protocol/src/lib.rs`

**Interfaces:**
- Produces:`ClientMessage`、`ServerMessage`(derive `Debug, Clone, PartialEq, Serialize, Deserialize`);`MAX_FRAME: usize`;`encode_frame<T: Serialize>(&T) -> io::Result<Vec<u8>>`;`try_decode_frame<T: DeserializeOwned>(&[u8]) -> io::Result<Option<(T, usize)>>`(缓冲不足返回 `Ok(None)`,成功返回消息与消耗字节数);`write_frame`/`read_frame`(基于 `Write`/`Read` 的薄封装);`pipe_name(user: &str) -> String`。

- [ ] **Step 1: 根 Cargo.toml 加成员**

把 members 改为:

```toml
members = ["crates/pyime-backend", "crates/pyime-engine", "crates/pyime-tsf", "crates/pyime-protocol"]
```

- [ ] **Step 2: 写 crates/pyime-protocol/Cargo.toml**

```toml
[package]
name = "pyime-protocol"
version = "0.0.1"
edition = "2021"
license = "GPL-3.0-only"
publish = false

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 3: 先写测试与桩(使其失败)**

`crates/pyime-protocol/src/lib.rs`:

```rust
//! TSF DLL 与 pyime-server 之间的管道协议:信封类型与长度前缀帧。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Read, Write};

/// 单帧正文上限(字节)。
pub const MAX_FRAME: usize = 1 << 20;

/// DLL → server。`Req.json` 是 `Session::handle` 的 PIME 风格请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Open { sid: u64, host: String },
    Req { sid: u64, json: Value },
    Rect { sid: u64, left: i32, top: i32, right: i32, bottom: i32 },
    Close { sid: u64 },
}

/// server → DLL。`Resp.json` 是 `Session::handle` 的 PIME 风格响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Resp { sid: u64, json: Value },
}

/// 每用户一个管道,避免多用户/RDP 下互相连错。
pub fn pipe_name(_user: &str) -> String {
    String::new()
}

pub fn encode_frame<T: Serialize>(_msg: &T) -> io::Result<Vec<u8>> {
    Err(io::Error::other("未实现"))
}

pub fn try_decode_frame<T: DeserializeOwned>(_buf: &[u8]) -> io::Result<Option<(T, usize)>> {
    Err(io::Error::other("未实现"))
}

pub fn write_frame<W: Write, T: Serialize>(_w: &mut W, _msg: &T) -> io::Result<()> {
    Err(io::Error::other("未实现"))
}

pub fn read_frame<R: Read, T: DeserializeOwned>(_r: &mut R) -> io::Result<T> {
    Err(io::Error::other("未实现"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    fn samples() -> Vec<ClientMessage> {
        vec![
            ClientMessage::Open { sid: 1, host: "notepad.exe".into() },
            ClientMessage::Req { sid: 2, json: json!({"method": "onKeyDown", "keyCode": 78}) },
            ClientMessage::Rect { sid: 3, left: -10, top: 20, right: 30, bottom: 44 },
            ClientMessage::Close { sid: 4 },
        ]
    }

    #[test]
    fn 管道名带用户标识() {
        assert_eq!(pipe_name("S-1-5-21-1"), r"\\.\pipe\pyime-S-1-5-21-1");
    }

    #[test]
    fn 信封往返() {
        for m in samples() {
            let bytes = encode_frame(&m).unwrap();
            let (back, used): (ClientMessage, usize) = try_decode_frame(&bytes).unwrap().unwrap();
            assert_eq!(back, m);
            assert_eq!(used, bytes.len());
        }
        let r = ServerMessage::Resp { sid: 9, json: json!({"return": true, "compositionString": ""}) };
        let bytes = encode_frame(&r).unwrap();
        let (back, _): (ServerMessage, usize) = try_decode_frame(&bytes).unwrap().unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn 帧头是小端长度() {
        let bytes = encode_frame(&ClientMessage::Close { sid: 4 }).unwrap();
        let n = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert_eq!(n, bytes.len() - 4);
        assert!(bytes[4..].starts_with(b"{"));
    }

    #[test]
    fn 缓冲不足返回none() {
        let bytes = encode_frame(&ClientMessage::Close { sid: 4 }).unwrap();
        assert!(try_decode_frame::<ClientMessage>(&bytes[..2]).unwrap().is_none());
        assert!(try_decode_frame::<ClientMessage>(&bytes[..bytes.len() - 1]).unwrap().is_none());
    }

    #[test]
    fn 连续两帧按消耗字节数切分() {
        let mut buf = encode_frame(&ClientMessage::Close { sid: 1 }).unwrap();
        buf.extend(encode_frame(&ClientMessage::Close { sid: 2 }).unwrap());
        let (a, used): (ClientMessage, usize) = try_decode_frame(&buf).unwrap().unwrap();
        assert_eq!(a, ClientMessage::Close { sid: 1 });
        let (b, _): (ClientMessage, usize) = try_decode_frame(&buf[used..]).unwrap().unwrap();
        assert_eq!(b, ClientMessage::Close { sid: 2 });
    }

    #[test]
    fn 超长帧被拒绝() {
        let mut buf = ((MAX_FRAME + 1) as u32).to_le_bytes().to_vec();
        buf.extend([0u8; 8]);
        let e = try_decode_frame::<ClientMessage>(&buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn 非法json报invaliddata() {
        let mut buf = 3u32.to_le_bytes().to_vec();
        buf.extend(b"{{{");
        let e = try_decode_frame::<ClientMessage>(&buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn 读写封装往返并在截断时报eof() {
        let mut out = Vec::new();
        for m in samples() {
            write_frame(&mut out, &m).unwrap();
        }
        let mut cur = Cursor::new(out.clone());
        for m in samples() {
            let back: ClientMessage = read_frame(&mut cur).unwrap();
            assert_eq!(back, m);
        }
        let mut cut = Cursor::new(out[..out.len() - 1].to_vec());
        let mut last = Ok(ClientMessage::Close { sid: 0 });
        for _ in 0..samples().len() {
            last = read_frame(&mut cut);
        }
        assert_eq!(last.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
```

- [ ] **Step 4: 运行测试确认失败**

Run: `cargo test -p pyime-protocol > /tmp/p1.log 2>&1; grep -E "test result|FAILED|panicked" /tmp/p1.log | head`
Expected: 多个用例 FAILED("未实现"/断言失败)。

- [ ] **Step 5: 实现**

把 5 个桩函数替换为:

```rust
pub fn pipe_name(user: &str) -> String {
    format!(r"\\.\pipe\pyime-{user}")
}

pub fn encode_frame<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
    let body = serde_json::to_vec(msg).map_err(io::Error::other)?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "帧过大"));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}

pub fn try_decode_frame<T: DeserializeOwned>(buf: &[u8]) -> io::Result<Option<(T, usize)>> {
    let Some(head) = buf.get(..4) else { return Ok(None) };
    let n = u32::from_le_bytes(head.try_into().unwrap()) as usize;
    if n > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "帧过大"));
    }
    let Some(body) = buf.get(4..4 + n) else { return Ok(None) };
    let msg = serde_json::from_slice(body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some((msg, 4 + n)))
}

pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    w.write_all(&encode_frame(msg)?)?;
    w.flush()
}

pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<T> {
    let mut head = [0u8; 4];
    r.read_exact(&mut head)?;
    let n = u32::from_le_bytes(head) as usize;
    if n > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "帧过大"));
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p pyime-protocol > /tmp/p1.log 2>&1; grep -E "test result|FAILED|panicked" /tmp/p1.log`
Expected: `test result: ok. 8 passed`。

- [ ] **Step 7: Commit**

先 `git diff Cargo.lock` 确认只新增 `pyime-protocol` 条目,再:

```bash
git add Cargo.toml Cargo.lock crates/pyime-protocol
git commit -m "pyime-protocol:管道信封与长度前缀帧

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 2: pyime-backend 库化

**Files:**
- Create: `crates/pyime-backend/src/lib.rs`
- Modify: `crates/pyime-backend/src/main.rs:1-8`

**Interfaces:**
- Produces:库 `pyime_backend` 导出 `pub mod ime, inputlog, log, punct, session`(Task 3 再加 `hub`)。`protocol` 与 `register` 仍只属 PIME 版 bin。

- [ ] **Step 1: 记录基线测试数**

Run: `cargo test -p pyime-backend > /tmp/base.log 2>&1; grep -E "^test result" /tmp/base.log`
Expected: 记下各 target 的 passed 数(后续必须相同)。

- [ ] **Step 2: 写 lib.rs**

```rust
//! PYIME 会话逻辑库:PIME 版后端与独立 server 共用。

pub mod ime;
pub mod inputlog;
pub mod log;
pub mod punct;
pub mod session;
```

- [ ] **Step 3: 改 main.rs 顶部**

把

```rust
mod ime;
mod inputlog;
mod log;
mod punct;
mod protocol;
mod register;
mod session;
```

替换为:

```rust
mod protocol;
mod register;

use pyime_backend::{ime, inputlog, log, session};
```

- [ ] **Step 4: 编译并对比测试数**

Run: `cargo build -p pyime-backend > /tmp/b2.log 2>&1; tail -5 /tmp/b2.log; cargo test -p pyime-backend > /tmp/t2.log 2>&1; grep -E "^test result|warning: unused|error" /tmp/t2.log`
Expected: 无 error;各 target passed 数与 Step 1 一致。若出现新的 `unused`/`dead_code` 警告(模块变 pub 后通常消失),如实记录。

- [ ] **Step 5: 交叉编译检查(含 `cfg(windows)` 代码)**

Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-backend > /tmp/c2.log 2>&1; tail -5 /tmp/c2.log`
Expected: `Finished`。

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-backend/src/lib.rs crates/pyime-backend/src/main.rs
git commit -m "pyime-backend:拆出库,会话相关模块对外导出

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 3: Hub(多会话中枢,TDD)

**Files:**
- Create: `crates/pyime-backend/src/hub.rs`
- Modify: `crates/pyime-backend/src/lib.rs`(加 `pub mod hub;`)

**Interfaces:**
- Consumes:`session::Session::new(Rc<Ime>)`、`Session::handle(&mut self, &Value) -> Value`、`Ime::with_engine`(`#[cfg(test)]`,crate 内可用)。
- Produces:`Hub::new(Rc<Ime>) -> Hub`、`open(&mut self, sid: u64)`、`close(&mut self, sid: u64)`、`handle(&mut self, sid: u64, req: &Value) -> Value`(会话不存在时自动创建)、`session_count(&self) -> usize`。

- [ ] **Step 1: lib.rs 加 `pub mod hub;`**(放在 `pub mod ime;` 之前,保持字母序)。

- [ ] **Step 2: 先写测试与桩(使其失败)**

`crates/pyime-backend/src/hub.rs`:

```rust
//! 多会话中枢:按会话号持有 Session,供独立 server 把管道请求分发到各自的组词状态。

use crate::ime::Ime;
use crate::session::Session;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Hub {
    ime: Rc<Ime>,
    sessions: HashMap<u64, Session>,
}

impl Hub {
    pub fn new(ime: Rc<Ime>) -> Hub {
        Hub { ime, sessions: HashMap::new() }
    }

    pub fn open(&mut self, _sid: u64) {}

    pub fn close(&mut self, _sid: u64) {}

    pub fn handle(&mut self, _sid: u64, _req: &Value) -> Value {
        Value::Null
    }

    pub fn session_count(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyime_engine::dict::{Dict, DictBuilder};
    use pyime_engine::engine::Engine;
    use pyime_engine::fuzzy::FuzzyConfig;
    use serde_json::json;

    const N: i64 = 78;
    const I: i64 = 73;
    const W: i64 = 87;
    const O: i64 = 79;
    const VK_ESCAPE: i64 = 27;
    const KEY_1: i64 = 49;

    fn hub() -> Hub {
        let mut b = DictBuilder::default();
        for (i, w) in "你泥尼拟逆腻匿溺".chars().enumerate() {
            b.add("ni", &w.to_string(), 9000 - i as u32 * 100);
        }
        b.add("wo", "我", 8000);
        let engine = Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default());
        Hub::new(Ime::with_engine(engine))
    }

    fn key(h: &mut Hub, sid: u64, method: &str, code: i64) -> Value {
        h.handle(sid, &json!({"method": method, "seqNum": 1, "keyCode": code}))
    }

    #[test]
    fn 按需创建并可关闭会话() {
        let mut h = hub();
        assert_eq!(h.session_count(), 0);
        h.open(1);
        assert_eq!(h.session_count(), 1);
        key(&mut h, 2, "onKeyDown", N); // 未 open 的会话号自动创建
        assert_eq!(h.session_count(), 2);
        h.close(1);
        h.close(99); // 关闭不存在的会话不报错
        assert_eq!(h.session_count(), 1);
    }

    #[test]
    fn 会话之间组词状态互不干扰() {
        let mut h = hub();
        key(&mut h, 1, "onKeyDown", N);
        key(&mut h, 2, "onKeyDown", W);
        key(&mut h, 1, "onKeyDown", I);
        key(&mut h, 2, "onKeyDown", O);
        assert_eq!(key(&mut h, 1, "onKeyDown", KEY_1)["commitString"], "你");
        assert_eq!(key(&mut h, 2, "onKeyDown", KEY_1)["commitString"], "我");
    }

    #[test]
    fn esc清空组字且保留英文状态() {
        let mut h = hub();
        // 单击 Shift 切到英文:字母放行
        key(&mut h, 1, "filterKeyDown", 16);
        key(&mut h, 1, "filterKeyUp", 16);
        assert_eq!(key(&mut h, 1, "filterKeyDown", N)["return"], false);
        key(&mut h, 1, "onKeyDown", VK_ESCAPE);
        assert_eq!(key(&mut h, 1, "filterKeyDown", N)["return"], false);
    }

    #[test]
    fn esc清空中文组字() {
        let mut h = hub();
        key(&mut h, 1, "onKeyDown", N);
        key(&mut h, 1, "onKeyDown", I);
        let r = key(&mut h, 1, "onKeyDown", VK_ESCAPE);
        assert_eq!(r["compositionString"], "");
    }
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -p pyime-backend hub > /tmp/h3.log 2>&1; grep -E "test result|FAILED|panicked" /tmp/h3.log | head`
Expected: 4 个用例中至少 `按需创建并可关闭会话`、`会话之间组词状态互不干扰`、`esc清空中文组字` FAILED。

- [ ] **Step 4: 实现**

把 4 个桩方法替换为:

```rust
    pub fn open(&mut self, sid: u64) {
        self.sessions.insert(sid, Session::new(self.ime.clone()));
    }

    pub fn close(&mut self, sid: u64) {
        self.sessions.remove(&sid);
    }

    pub fn handle(&mut self, sid: u64, req: &Value) -> Value {
        let ime = &self.ime;
        self.sessions.entry(sid).or_insert_with(|| Session::new(ime.clone())).handle(req)
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p pyime-backend hub > /tmp/h3.log 2>&1; grep -E "test result|FAILED|panicked" /tmp/h3.log`
Expected: `4 passed`。若 `会话之间组词状态互不干扰` 因 `Session` 的真实语义(例如数字键 1 的 commit 字段)与断言不符而失败,**先读 `session/tests.rs` 里同类用例确认实际行为**,只修正测试里的期望值并在报告中说明依据,不得改 `Session`。

- [ ] **Step 6: 全量回归**

Run: `cargo test --workspace > /tmp/all.log 2>&1; grep -E "^test result|FAILED|error" /tmp/all.log`
Expected: 全部 ok,`pyime-backend` 的 passed 数 = Task 2 基线 + 4。

- [ ] **Step 7: Commit**

```bash
git add crates/pyime-backend/src/hub.rs crates/pyime-backend/src/lib.rs
git commit -m "pyime-backend:Hub 按会话号持有 Session

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## 后续计划(本计划完成后另出)

- **A2 `pyime-server`**:管道服务端(I/O 线程 + 主线程消息循环 + `mpsc`/`PostMessage`)、单实例、DACL、最小候选窗;Windows 专有,先 `cargo check --target x86_64-pc-windows-gnu`,再实机。
- **A3 `pyime-tsf` 扩展**:组字串、`GetTextExt` 光标矩形、管道客户端(overlapped + 150 ms 超时放行)、`OnTestKeyUp` 缓存、终止补发 Esc、拉起 server 退避、`tsf.log`。
- A2/A3 都依赖 Task 1 的 `pyime-protocol` 与 Task 3 的 `Hub`。
