# 子项目 A3:TSF 客户端扩展 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 spike 的 `pyime-tsf`(按 a–z 上屏"测试")改成真正的客户端:按键经命名管道交给 `pyime-server`,按响应维护组字串、上屏、报告光标矩形;server 不可用时按键一律放行。

**Architecture:** DLL 很薄。`wire`(纯逻辑,Linux 可测)负责请求 JSON 构造、响应解析、限流;`com/log`、`com/pipe`(overlapped 管道客户端,150 ms 超时)、`com/launch`(拉起 server)、`com/composition`(edit session:组字/上屏/光标矩形)是 Windows 专有;`com/service.rs` 把它们接到 `ITfKeyEventSink`/`ITfCompositionSink`。协议见 `crates/pyime-protocol`,会话语义见 `crates/pyime-backend/src/session.rs`(`Session::handle`)。

**Tech Stack:** Rust 2021,`windows`/`windows-core` 0.62.2,`pyime-protocol`,`serde_json`;交叉编译目标 `x86_64-pc-windows-gnu`(`scripts/build-tsf.sh`)。

## Global Constraints

- 回复、注释、commit 一律简体中文;commit 末尾加 `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`。
- 不改 `Session`/引擎/`pyime-backend`/`pyime-server`;管道直传 PIME 风格 JSON(消息体就是 `Session::handle` 的请求/响应)。
- 管道名 `\\.\pipe\pyime-<用户 SID>`(`pyime_protocol::pipe_name` + `current_user_sid`);4 字节小端长度前缀 + JSON,上限 1 MiB(`encode_frame`/`MAX_FRAME`)。
- 管道往返总超时 150 ms;超时、断开、server 不在 → 按键一律放行(`return FALSE`),不阻塞宿主 UI 线程。超时后必须丢弃该连接(流已失步),下次调用重连并重新 `Open`。
- 会话号 `sid` 全局唯一:`(进程 ID << 32) | 进程内自增`;`Open` 发在每次(重)连之后;`Close` 发在 `Deactivate`。注意 `Hub::open` 会重建 `Session`(状态清零),所以只在连接建立或激活时发。
- 拉起 `pyime-server.exe`:与本 DLL 同目录;每进程至多每 2 秒尝试一次;不等待其就绪(本次按键放行)。
- DLL 日志追加写 `%LOCALAPPDATA%\PYIME\tsf.log`(不轮转,只记事件不记按键;`%LOCALAPPDATA%\PYIME\log.disabled` 存在则不写)。
- 密码框/`GUID_COMPARTMENT_KEYBOARD_DISABLED`(线程级或上下文级非零)→ 放行,不向 server 发按键。
- 构建只用 `scripts/build-tsf.sh`(`RUSTFLAGS="-C panic=abort -C target-feature=+crt-static"`),DLL 不得依赖 libgcc/libwinpthread/libstdc。
- 不拷贝青简代码;若某段确为直接拷贝,必须加版权头并更新 `installer/NOTICE.md`。
- Windows API 代码只能交叉编译检查,每个 Windows 任务必须通过 `cargo check --target x86_64-pc-windows-gnu -p pyime-tsf` 且无警告;`cargo test --workspace` 保持全绿。
- 不做:绿色高亮/⌄/字号/主题(B)、安装包/32 位(C)、组字串下划线 display attribute(B)、`GUID_PROP_INPUTSCOPE` 密码检测(验收后再定)。

---

### Task 1: `wire` 纯逻辑模块(Linux 可测)

**Files:**
- Create: `crates/pyime-tsf/src/wire.rs`
- Modify: `crates/pyime-tsf/src/lib.rs`(加 `pub mod wire;`)
- Modify: `crates/pyime-tsf/Cargo.toml`(`[dependencies]` 加 `serde_json = "1"`,与 workspace 其他 crate 用的版本写法一致)

**Interfaces:**
- Produces(后续任务依赖,名字与签名固定):
  - `pub fn key_request(method: &str, vk: u32, char_code: u32, key_states: &[u8]) -> serde_json::Value`
  - `pub struct Action { pub eat: bool, pub commit: Option<String>, pub composition: Option<String>, pub cursor: Option<usize> }`,`#[derive(Debug, Default, PartialEq, Clone)]`
  - `impl Action { pub fn from_resp(resp: &Value) -> Action; pub fn needs_edit(&self) -> bool }`(`needs_edit` = commit 或 composition 有值)
  - `pub fn utf16_offset(s: &str, chars: usize) -> usize`(前 `chars` 个字符对应的 UTF-16 单元数,超出则取整串)
  - `pub fn wants_key_up(vk: u32) -> bool`(仅 Shift 键 16/160/161 需要发 keyUp;`Session` 只在 Shift 上用 keyUp)
  - `pub struct Throttle`,`Throttle::new(interval_ms: u64)`,`allow(&mut self, now_ms: u64) -> bool`(首次必为 true;距上次放行 < interval 为 false,并且不刷新计时)

- [ ] **Step 1: 写失败测试**(`wire.rs` 末尾 `#[cfg(test)] mod tests`,先只放测试和空桩签名,编译通过但断言失败)

```rust
use super::*;
use serde_json::json;

#[test]
fn 请求含方法键码字符与256项键态() {
    let mut states = [0u8; 256];
    states[16] = 0x80;
    let r = key_request("filterKeyDown", 0x4E, 'n' as u32, &states);
    assert_eq!(r["method"], "filterKeyDown");
    assert_eq!(r["keyCode"], 0x4E);
    assert_eq!(r["charCode"], 110);
    assert_eq!(r["keyStates"].as_array().unwrap().len(), 256);
    assert_eq!(r["keyStates"][16], 0x80);
    assert_eq!(r["keyStates"][17], 0);
}

#[test]
fn 键态不足256项时补零() {
    let r = key_request("onKeyUp", 16, 0, &[1, 2, 3]);
    let a = r["keyStates"].as_array().unwrap();
    assert_eq!(a.len(), 256);
    assert_eq!(a[2], 3);
    assert_eq!(a[3], 0);
}

#[test]
fn 解析上屏与组字() {
    let a = Action::from_resp(&json!({"return": true, "commitString": "你", "compositionString": "", "compositionCursor": 0}));
    assert_eq!(a, Action { eat: true, commit: Some("你".into()), composition: Some("".into()), cursor: Some(0) });
    assert!(a.needs_edit());
}

#[test]
fn 缺省字段为空_空上屏串视为无() {
    let a = Action::from_resp(&json!({"return": false, "commitString": ""}));
    assert_eq!(a, Action::default());
    assert!(!a.needs_edit());
    assert_eq!(Action::from_resp(&json!({})), Action::default());
}

#[test]
fn utf16偏移按字符数换算() {
    assert_eq!(utf16_offset("ni好", 3), 3);
    assert_eq!(utf16_offset("a𠀀b", 2), 3);
    assert_eq!(utf16_offset("ab", 9), 2);
    assert_eq!(utf16_offset("ab", 0), 0);
}

#[test]
fn 只有shift需要发keyup() {
    for vk in [16, 160, 161] {
        assert!(wants_key_up(vk));
    }
    assert!(!wants_key_up(0x41));
    assert!(!wants_key_up(17));
}

#[test]
fn 限流首次放行_间隔内拒绝_到点放行() {
    let mut t = Throttle::new(2000);
    assert!(t.allow(5000));
    assert!(!t.allow(6999));
    assert!(t.allow(7000));
    assert!(!t.allow(8999));
}
```

- [ ] **Step 2: 运行确认失败**
Run: `cargo test -p pyime-tsf wire > /tmp/a3t1.log 2>&1; tail -20 /tmp/a3t1.log`
Expected: FAIL(断言失败或 `todo!()`),不是编译错误。

- [ ] **Step 3: 实现**(桩换成真实现)

```rust
//! 与 Windows 无关的线路逻辑:请求构造、响应解析、限流。Linux 上可单测。

use serde_json::{json, Value};

/// 构造 `Session::handle` 能读的 PIME 风格按键请求。`key_states` 不足 256 项时补零。
pub fn key_request(method: &str, vk: u32, char_code: u32, key_states: &[u8]) -> Value {
    let mut states = vec![0u8; 256];
    let n = key_states.len().min(256);
    states[..n].copy_from_slice(&key_states[..n]);
    json!({ "method": method, "keyCode": vk, "charCode": char_code, "keyStates": states })
}

/// 响应里 TSF 关心的部分。字段缺省 = 不变(`None`);`composition == Some("")` = 清空组字串。
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Action {
    pub eat: bool,
    pub commit: Option<String>,
    pub composition: Option<String>,
    /// 组字串内光标位置,单位为字符(非 UTF-16)。
    pub cursor: Option<usize>,
}

impl Action {
    pub fn from_resp(resp: &Value) -> Action {
        Action {
            eat: resp["return"].as_bool().unwrap_or(false),
            commit: resp["commitString"].as_str().filter(|s| !s.is_empty()).map(String::from),
            composition: resp["compositionString"].as_str().map(String::from),
            cursor: resp["compositionCursor"].as_u64().map(|n| n as usize),
        }
    }

    pub fn needs_edit(&self) -> bool {
        self.commit.is_some() || self.composition.is_some()
    }
}

/// 前 `chars` 个字符占多少个 UTF-16 单元(超出取整串)。
pub fn utf16_offset(s: &str, chars: usize) -> usize {
    s.chars().take(chars).map(char::len_utf16).sum()
}

/// `Session` 只在 Shift 键上使用 keyUp(单击 Shift 切换中英文),其余键不必付出管道往返。
pub fn wants_key_up(vk: u32) -> bool {
    matches!(vk, 16 | 160 | 161)
}

/// 最小间隔限流:用于限制拉起 server 的频率。
pub struct Throttle {
    interval_ms: u64,
    last: Option<u64>,
}

impl Throttle {
    pub fn new(interval_ms: u64) -> Throttle {
        Throttle { interval_ms, last: None }
    }

    pub fn allow(&mut self, now_ms: u64) -> bool {
        match self.last {
            Some(t) if now_ms.saturating_sub(t) < self.interval_ms => false,
            _ => {
                self.last = Some(now_ms);
                true
            }
        }
    }
}
```

- [ ] **Step 4: 运行确认通过**
Run: `cargo test -p pyime-tsf > /tmp/a3t1.log 2>&1; tail -20 /tmp/a3t1.log`
Expected: `wire` 7 个测试 + `keymap` 2 个全部 PASS。再跑 `cargo test --workspace > /tmp/a3all.log 2>&1; grep "test result" /tmp/a3all.log` 全绿。

- [ ] **Step 5: Commit**
```bash
git add crates/pyime-tsf Cargo.lock
git commit -m "pyime-tsf:wire 纯逻辑(请求构造、响应解析、限流)"
```
(提交信息末尾加 Co-Authored-By 行。)

---

### Task 2: 日志、拉起 server、管道客户端(Windows)

**Files:**
- Create: `crates/pyime-tsf/src/com/log.rs`、`crates/pyime-tsf/src/com/launch.rs`、`crates/pyime-tsf/src/com/pipe.rs`
- Modify: `crates/pyime-tsf/src/com/mod.rs`(`mod log; mod launch; mod pipe;`,不导出给外部)
- Modify: `crates/pyime-tsf/Cargo.toml`:`[dependencies]` 加 `pyime-protocol = { path = "../pyime-protocol" }`;`windows` features 追加 `Win32_Security`、`Win32_Storage_FileSystem`、`Win32_System_IO`、`Win32_System_Threading`、`Win32_System_Variant`(后续任务要用,一次加齐;编译报缺哪个 feature 就补哪个)

**Interfaces:**
- Produces:
  - `log::write(msg: &str)`:追加一行 `<秒级时间戳> [pid] msg` 到 `%LOCALAPPDATA%\PYIME\tsf.log`;目录不存在则创建;`log.disabled` 存在则什么都不做;任何失败静默忽略(日志不得影响宿主)。
  - `launch::try_spawn_server()`:用 `static Mutex<Throttle>`(`Throttle::new(2000)`,时间取 `GetTickCount64`)限流;放行时启动 `<本 DLL 所在目录>\pyime-server.exe`(`std::process::Command`,`creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)`,不等待);文件不存在或启动失败只写日志。
  - `pipe::Client`:
    - `Client::new(host: String) -> Client`(`host` 为宿主进程 exe 文件名,用于 `Open`;`sid` 在此分配:`(GetCurrentProcessId() as u64) << 32 | 静态 AtomicU32 自增`)
    - `fn sid(&self) -> u64`
    - `fn call(&mut self, json: Value) -> Option<Value>`:发 `ClientMessage::Req`,读一帧 `ServerMessage::Resp`,返回其 `json`;任何失败返回 `None`
    - `fn send(&mut self, msg: ClientMessage)`:只写不读(`Rect`、`Close` 用);失败静默
    - 内部 `ensure_connected()`:未连接时 `CreateFileW(pipe_name(sid_string), GENERIC_READ|GENERIC_WRITE, 无共享, 默认安全, OPEN_EXISTING, FILE_FLAG_OVERLAPPED)`;失败(含 `ERROR_FILE_NOT_FOUND`)时调用 `launch::try_spawn_server()` 并返回失败;成功后立即写 `Open{sid, host}`,写失败则丢弃连接
    - 任何 I/O 超时/错误 → `CloseHandle`、置未连接、写一行日志(只在“连接→断开”的跳变时写,避免刷屏)

- [ ] **Step 1: 实现 `log.rs`、`launch.rs`**(各自 ≤ 60 行)。`launch` 取 DLL 目录用 `super::module_path()`(已存在,返回 `HSTRING`),`Path::parent`。`Throttle` 来自 `crate::wire`。

- [ ] **Step 2: 实现 `pipe.rs`**。核心是带总截止时间的 overlapped 读写,骨架如下(类型细节以编译器为准):

```rust
const DEADLINE_MS: u64 = 150;

/// 读/写恰好 buf.len() 字节,整个调用共用同一截止时间。
fn io_exact(handle: HANDLE, event: HANDLE, buf: &mut [u8], write: bool, deadline: Instant) -> io::Result<()> {
    let mut done = 0;
    while done < buf.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() { return Err(io::ErrorKind::TimedOut.into()); }
        let mut ov = OVERLAPPED { hEvent: event, ..Default::default() };
        unsafe { ResetEvent(event)? };
        let chunk = &mut buf[done..];
        let r = unsafe { if write { WriteFile(handle, Some(chunk), None, Some(&mut ov)) } else { ReadFile(handle, Some(chunk), None, Some(&mut ov)) } };
        if let Err(e) = r {
            if e.code() != ERROR_IO_PENDING.to_hresult() { return Err(io::Error::other(e)); }
            if unsafe { WaitForSingleObject(event, left.as_millis() as u32) } != WAIT_OBJECT_0 {
                // 超时:取消并等取消完成(ov 在栈上,必须等内核不再引用它)。
                unsafe { let _ = CancelIoEx(handle, Some(&ov)); let mut n = 0; let _ = GetOverlappedResult(handle, &ov, &mut n, true); }
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
        let mut n = 0u32;
        unsafe { GetOverlappedResult(handle, &ov, &mut n, false) }.map_err(io::Error::other)?;
        if n == 0 { return Err(io::ErrorKind::UnexpectedEof.into()); }
        done += n as usize;
    }
    Ok(())
}
```
  读帧:先读 4 字节头,校验 `<= MAX_FRAME`,再读正文,`serde_json::from_slice::<ServerMessage>`;写帧用 `pyime_protocol::encode_frame`。`call` = 取一个 `Instant::now() + 150ms` 的截止时间,写帧 + 读帧共用。事件对象每个 `Client` 一个(`CreateEventW(None, true, false, None)`),`Drop` 时关闭它和管道句柄。

- [ ] **Step 3: 验证**
Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-tsf > /tmp/a3t2.log 2>&1; tail -20 /tmp/a3t2.log`
Expected: 无错误、无警告(此时 `pipe`/`launch`/`log` 尚未被使用,允许在模块上加 `#![allow(dead_code)]`,Task 4 接线后必须删掉)。再跑 `cargo test --workspace` 全绿。

- [ ] **Step 4: Commit**
```bash
git add crates/pyime-tsf Cargo.lock
git commit -m "pyime-tsf:日志、拉起 server、overlapped 管道客户端(150 ms 超时)"
```

---

### Task 3: 组字串与上屏的 edit session(Windows)

**Files:**
- Create: `crates/pyime-tsf/src/com/composition.rs`
- Modify: `crates/pyime-tsf/src/com/mod.rs`(`mod composition;`)

**Interfaces:**
- Consumes: `crate::wire::{Action, utf16_offset}`
- Produces:
  - `pub struct Composition`(持有 `RefCell<Option<ITfComposition>>`),`Composition::new()`,`is_active(&self) -> bool`,`clear(&self)`(只丢引用,不碰文档;给 `OnCompositionTerminated` 用)
  - `pub fn apply(comp: &Rc<Composition>, ctx: &ITfContext, client_id: u32, sink: &ITfCompositionSink, action: &Action) -> Option<RECT>`:通过 `ctx.RequestEditSession(client_id, session, TF_ES_SYNC | TF_ES_READWRITE)` 应用 `action`;返回编辑后组字范围的屏幕矩形(`ITfContextView::GetTextExt`),无组字或失败返回 `None`;edit session 请求失败时返回 `None` 且不 panic

**应用顺序(在 `DoEditSession(ec)` 内,必须严格如此):**
1. `commit = Some(text)`:
   - 有进行中的组字:`range = composition.GetRange()`,`range.SetText(ec, 0, utf16)`,`composition.EndComposition(ec)`,清空状态;再 `range.Collapse(ec, TF_ANCHOR_END)` 并 `SetSelection` 到该处,使后续输入接在上屏文字后。
   - 无组字:`ITfInsertAtSelection::InsertTextAtSelection(ec, 0, utf16)`(同 spike)。
2. `composition = Some(s)`:
   - `s` 非空且无进行中的组字:`ITfInsertAtSelection::InsertTextAtSelection(ec, TF_IAS_QUERYONLY, &[])` 取得插入点 range;`ctx.cast::<ITfContextComposition>()?.StartComposition(ec, &range, sink)` 得到 `ITfComposition` 存入状态。
   - `s` 非空:`composition.GetRange().SetText(ec, 0, utf16(s))`;若 `action.cursor = Some(n)`:克隆 range,`Collapse(START)`,`ShiftEnd(ec, utf16_offset(s, n) as i32, ...)`,`Collapse(END)`,`SetSelection` 到该处(`TF_SELECTION` 的 `style.ase = TF_AE_NONE`,`fInterimChar = FALSE`)。
   - `s` 为空且有进行中的组字:`SetText(ec, 0, &[])`,`EndComposition(ec)`,清空状态。
3. 若仍有进行中的组字:`ctx.GetActiveView()?.GetTextExt(ec, &composition.GetRange()?, &mut rect, &mut clipped)` 取矩形。

- [ ] **Step 1: 实现**。`InsertTextAtSelection` 对只读文档会失败,`apply` 在任何一步失败时返回 `None` 并写日志(`log::write`),不残留半个状态:若 `StartComposition` 之后 `SetText` 失败,调用 `EndComposition` 再清状态。
- [ ] **Step 2: 验证**
Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-tsf > /tmp/a3t3.log 2>&1; tail -20 /tmp/a3t3.log`
Expected: 无错误无警告(`allow(dead_code)` 同上)。
- [ ] **Step 3: Commit** `pyime-tsf:组字串与上屏的 edit session`

---

### Task 4: 接线 `service.rs`(Windows)

**Files:**
- Modify: `crates/pyime-tsf/src/com/service.rs`(重写;去掉 spike 的 `TEXT`/`InsertSession`/`would_eat`)
- Modify: `crates/pyime-tsf/src/com/mod.rs`、`pipe.rs`、`launch.rs`、`log.rs`、`composition.rs`(删 Task 2/3 的 `allow(dead_code)`)
- Modify: `crates/pyime-tsf/src/keymap.rs`(`is_letter_vk` 若不再被使用则删除其测试与函数;若仍被引用则保留)

**Interfaces:**
- Consumes: Task 1–3 的全部接口。
- `TextService` 实现 `ITfTextInputProcessor`、`ITfKeyEventSink`、`ITfCompositionSink`(`#[implement(...)]` 三个接口)。字段:`thread_mgr`、`client_id`、`client: RefCell<Option<pipe::Client>>`、`comp: Rc<Composition>`、`last_up: RefCell<Option<(u32, Action)>>`(OnTestKeyUp 缓存)。

**行为(逐条对应 spec「按键与数据流」):**
1. `Activate`:同 spike 注册 key sink;`client = Some(Client::new(宿主 exe 名))`;**不**在此连接(连接发生在第一次按键,避免激活变慢)。
2. `Deactivate`:`client.send(Close{sid})`,丢弃 client;`comp.clear()`;注销 key sink。
3. `OnTestKeyDown`:若 `bypass(ctx)`(见第 8 条)→ FALSE;否则 `client.call(key_request("filterKeyDown", vk, char_code, &states))`;`None` → FALSE;否则 `Action::from_resp(..).eat`。
4. `OnKeyDown`:同上但 `"onKeyDown"`;`call` 返回 `None` → FALSE。得到 `Action` 后若 `needs_edit()`,调 `composition::apply`;`apply` 返回 `Some(rect)` 时 `client.send(Rect{sid, left, top, right, bottom})`。返回 `action.eat`;若 `apply` 失败(edit session 请求失败)且 `eat` 为真,仍返回 `eat`(server 状态已变,放行会使拼音状态与文档错位——宁可吃掉)并写日志。
5. `OnTestKeyUp`:仅当 `wants_key_up(vk)`:`call(key_request("filterKeyUp", ...))`,结果 `Action` 连同 `vk` 存入 `last_up`;返回 `action.needs_edit()`(有上屏/清空才吃,让 `OnKeyUp` 应用);其余键返回 FALSE。
6. `OnKeyUp`:若 `last_up` 的 `vk` 与当前相同,取出其 `Action` 并 `apply`(同第 4 条的 Rect 处理);返回 `needs_edit()`。取不到则 FALSE。
7. `OnCompositionTerminated`(`ITfCompositionSink`):`comp.clear()`,再 `client.call(key_request("onKeyDown", 27 /*VK_ESCAPE*/, 0, &[0; 256]))`,忽略响应(让 `Session` 清空组字且保留中英文状态,server 因响应里 `showCandidates:false` 隐藏候选窗)。注意此回调可能发生在 edit session 内部,**不得**在其中再请求 edit session。
8. `bypass(ctx)`:`thread_mgr` 或 `ctx` 的 `GUID_COMPARTMENT_KEYBOARD_DISABLED` compartment 值非零 → true。取值失败当作 false。
9. `char_code`:`ToUnicode(vk, scancode, &states, buf, 4 /*不改变键盘状态*/)`;仅 `onKeyDown`/`filterKeyDown` 且返回 1 且无 Ctrl/Alt 时取 `buf[0]`,否则 0。`scancode = (lparam >> 16) & 0xFF`。`states` 由 `GetKeyboardState` 取,失败则全零。
10. 全程不 `unwrap`;任何 COM 失败按放行处理。

- [ ] **Step 1: 实现** `service.rs`(≤ 150 行/次写入,分多次)与 `bypass`/`char_code` 辅助函数(可放 `com/keys.rs`)。
- [ ] **Step 2: 验证**
Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-tsf > /tmp/a3t4.log 2>&1; tail -20 /tmp/a3t4.log` → 无错误无警告;`cargo test --workspace > /tmp/a3all.log 2>&1; grep "test result" /tmp/a3all.log` → 全绿。
- [ ] **Step 3: Commit** `pyime-tsf:接入 server 管道,组字串/上屏/光标矩形/Esc/放行`

---

### Task 5: 构建、自检、部署说明

**Files:**
- Modify: `crates/pyime-tsf/src/com/mod.rs`(`SERVICE_DESCRIPTION` 改为 `"PYIME(自研预览)"`,便于与 spike 的"PYIME 实验"区分;CLSID/GUID 不变)
- Modify: `docs/superpowers/specs/2026-10-14-tsf-frontend-a-design.md`(末尾追加 `## A3 部署` 小节:`C:\PYIME\` 内放 `pyime_tsf.dll`、`pyime-server.exe`、`pyime.dict`;`regsvr32` 须管理员)

- [ ] **Step 1:** 运行 `scripts/build-tsf.sh > /tmp/a3build.log 2>&1; tail -15 /tmp/a3build.log`,再 `scripts/check-tsf-dll.sh`,Expected: `OK: ...pyime_tsf.dll`,无 libgcc/libwinpthread。
- [ ] **Step 2:** 记录 `pyime_tsf.dll` 大小到报告。
- [ ] **Step 3: Commit** `pyime-tsf:服务名区分预览版,补 A3 部署说明`

---

### Task 6: Windows 实机验收(用户操作)

前置:A2 的 server 验收已通过。把 `pyime_tsf.dll` 放进 `C:\PYIME\`(与 `pyime-server.exe`、`pyime.dict` 同目录),管理员终端 `regsvr32 C:\PYIME\pyime_tsf.dll`;语言列表里添加"PYIME(自研预览)"。若之前装过 spike 版,先 `regsvr32 /u` 旧 DLL。

1. 记事本:切到该输入法,输入 `ni`:组字串 `ni` 出现在光标处,候选窗在其下方,按 `1` 上屏"你"。
2. 翻页(`-`/`=`)、空格上屏、回车上屏拼音、退格、Esc 取消。
3. 组字中单击 Shift:拼音原样上屏并切到英文,不丢字;再次单击切回。
4. 中文标点;输入 `(` 自动补全成对并把光标放在括号内。
5. 选过的词再输入,排名上升(学习生效)。
6. 组字中点击别处:组字被终止后再输入,状态正常(无残留候选窗)。
7. 任务管理器结束 `pyime-server.exe`:按键放行(输入不卡);再按键后自动重新拉起并恢复(`tsf.log` 有记录)。
8. 浏览器输入框、Windows Terminal 同样可用;密码框不弹候选窗(若弹出,记录宿主,后续补 InputScope 检测)。
9. 打字体感:连续快速打字是否卡顿(每键一次管道往返),如有记录 `tsf.log`。

出问题时提供:现象、宿主程序、`%LOCALAPPDATA%\PYIME\tsf.log` 与 `log.txt` 末尾。通过后在 A spec 追加「## A3 结果」并提交。
