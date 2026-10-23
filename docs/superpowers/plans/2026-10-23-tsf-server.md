# pyime-server 管道服务端 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 后端可执行文件改名 `pyime-server.exe`(GUI 子系统),新增 `--supervise` 监护进程、`--quit`、`--worker` 命名管道服务端与看门狗,用户数据改原子写,附开发工具 `pyime-pipe-cli`;旧 PIME stdin 模式(无参数)保留。

**Architecture:** 设计见 `docs/superpowers/specs/2026-10-23-tsf-frontend-design.md` 组件 2、实施顺序第 2 步。纯逻辑(重启策略、请求分发、看门狗状态、SDDL/管道名拼装、原子写)放在不依赖 Win32 的代码里,Linux 上单测;Win32 调用集中在 `win.rs` 的 `#[cfg(windows)] pub mod sys`,沿用 `main.rs` 手写 `extern "system"` 的写法。工作线程独占 `Rc<Ime>` 与全部 `Session`,`session.rs`/`ime.rs` 的单线程模型不变。

**Tech Stack:** Rust 2021、serde_json、std::sync::mpsc;mingw 交叉编译 `x86_64-pc-windows-gnu`;NSIS。

## Global Constraints

- 注释、日志、commit message 一律简体中文;commit 结尾 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 不引入 windows / windows-sys crate;Win32 一律手写 `extern "system"`,每个 `unsafe` 块前写 `// SAFETY:`。
- 管道 `\\.\pipe\<GetUserNameW 用户名>\PYIME`,消息模式;一条消息 = 一个 UTF-8 JSON 请求(字段同现 PIME 协议,可能带结尾 `\n`),回复一条 JSON 消息(即 `Session::handle` 的返回值,含 `success`、`seqNum`)。连接断开 = 会话结束(原 `close`)。
- `pyime-server.exe` 参数:`--supervise` 监护、`--worker` 管道服务、`--quit` 通知退出(等 ≤3s,事件不存在立即返回);无参数 = 旧 PIME stdin 模式;`--register/--unregister <PIME目录>` 照旧。
- 互斥量 `Local\PYIME-supervisor`,退出事件 `Local\PYIME-quit`。
- 重启策略:worker 60s 内退出 5 次即停止重启;看门狗:单条请求处理超过 5s 退出 worker 进程。
- 安装目录(计划 C)`$PROGRAMFILES64\PYIME`,图标 `icons\` 在 exe 同目录。本计划对 `installer/pyime.nsi` 只做改名所需最小改动。
- `session.rs`、`tray.rs`、`punct.rs` 不改;`cargo clippy --workspace --all-targets` 无警告。
- 单个文件编辑 ≤150 行一次。

## 文件结构

| 文件 | 动作 | 职责 |
|---|---|---|
| `crates/pyime-backend/Cargo.toml` | 改 | `[[bin]] pyime-server` |
| `crates/pyime-backend/src/main.rs` | 改 | GUI 子系统;按参数分派 stdin / `--worker` / `--supervise` / `--quit` |
| `crates/pyime-backend/src/register.rs` | 改 | backends.json 的 command 改 `pyime\pyime-server.exe` |
| `crates/pyime-backend/src/ime.rs` | 改 | `write_atomic`;`save`/`save_english` 改用它 |
| `crates/pyime-backend/src/supervise.rs` | 新 | `RestartPolicy`(纯逻辑)+ 监护进程与 `--quit`(Windows) |
| `crates/pyime-backend/src/hub.rs` | 新 | `Msg`、`Hub`、工作线程 `run`、看门狗状态 `Busy`(纯逻辑) |
| `crates/pyime-backend/src/win.rs` | 新 | `pipe_path`/`pipe_sddl`/`object_sddl`(纯字符串)+ `sys`(Win32 声明与收发工具) |
| `crates/pyime-backend/src/pipe.rs` | 新 | `--worker`:建管道、accept、每连接 I/O 线程、看门狗线程(Windows) |
| `crates/pyime-backend/src/bin/pyime-pipe-cli.rs` | 新 | 开发工具,`#[path]` 复用 `win.rs` |
| `scripts/package.sh`、`installer/pyime.nsi`、`.gitignore` | 改 | 文件名 `server.exe` → `pyime-server.exe` |
| `pime/test-kit/pipecheck.txt`、`pime/test-kit/pipe-requests.txt` | 新 | Windows 实机验收清单与 cli 样例请求 |

不在本计划内:`scripts/package-pime-zip.sh` 与 `installer/zip-pime/`(zip 形态附加包)仍引用 `server.exe`,本计划后运行会以"缺少 installer/server.exe"失败退出;它们属 PIME 遗留,由计划 C 第 6 步一并删除。

---

### Task 1: 可执行文件改名 pyime-server + GUI 子系统

**Files:**
- Modify: `crates/pyime-backend/Cargo.toml`
- Modify: `crates/pyime-backend/src/main.rs:1`(加属性)、`:68-73`(启动日志)
- Modify: `crates/pyime-backend/src/register.rs:9`、测试模块
- Modify: `scripts/package.sh:9-13`、`installer/pyime.nsi:53-54,69,95`、`.gitignore:2`

**Interfaces:**
- Produces: 产物 `target/<triple>/release/pyime-server.exe`;安装器内文件名 `pyime-server.exe`;`register::add_backend` 写入 `"command": "pyime\\pyime-server.exe"`。

- [ ] **Step 1: 写失败测试**(`register.rs` 的 `mod tests` 末尾追加)

```rust
    #[test]
    fn 登记的可执行文件为_pyime_server() {
        let list = parse_list(&add_backend(BASE).unwrap()).unwrap();
        assert_eq!(list[1]["command"], "pyime\\pyime-server.exe");
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-backend register::tests::登记的可执行文件为_pyime_server`
Expected: FAIL,`left: "pyime\\server.exe"`。

- [ ] **Step 3: 实现**

`register.rs:9` 改为:
```rust
        "command": "pyime\\pyime-server.exe",
```

`Cargo.toml` 在 `[dependencies]` 之前加:
```toml
# 主程序产物名 pyime-server.exe(安装包与 backends.json 用这个名字);src/bin/ 下的工具照常自动发现
[[bin]]
name = "pyime-server"
path = "src/main.rs"
```

`main.rs` 第 1 行之前插入(必须是文件首个条目):
```rust
// GUI 子系统:由监护进程/Run 键/PIMELauncher 启动时不弹控制台窗;stdin/stdout 由启动方经 STARTUPINFO 传入,照常可用。
#![cfg_attr(windows, windows_subsystem = "windows")]
```

`main.rs` 启动日志中的 `"启动 pyime-backend {} args={:?} exe={:?}"` 改为 `"启动 pyime-server {} args={:?} exe={:?}"`。

`scripts/package.sh` 第 9-13 行改为:
```bash
cp target/x86_64-pc-windows-gnu/release/pyime-server.exe installer/pyime-server.exe
scripts/build-pime-dll.sh  # 补丁版 PIMETextService.dll → target/pime/{x64,x86}
if x86_64-w64-mingw32-objdump -p installer/pyime-server.exe | grep -E 'DLL Name: (libgcc|libwinpthread)'; then
  echo "pyime-server.exe 依赖了 mingw 运行库,中止" >&2; exit 1
fi
```

`installer/pyime.nsi`:第 53-54 行改为
```nsis
  SetOutPath "$INSTDIR\pyime"
  Delete "$INSTDIR\pyime\server.exe"  ; 0.1.15 之前的旧名,Launcher 已退出,不再占用
  File "pyime-server.exe"
```
第 69 行与第 95 行中的 `"$INSTDIR\pyime\server.exe"` 都改为 `"$INSTDIR\pyime\pyime-server.exe"`。

`.gitignore` 第 2 行 `installer/server.exe` 改为 `installer/pyime-server.exe`。

- [ ] **Step 4: 运行确认通过,并核对产物名**

Run: `cargo test -q -p pyime-backend`
Expected: 全部 PASS(`test result: ok.`,无 failed)。

Run: `cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys;print(sorted(t['name'] for p in json.load(sys.stdin)['packages'] if p['name']=='pyime-backend' for t in p['targets']))"`
Expected: `['pyime-misses', 'pyime-replay', 'pyime-server']`(不再有 `pyime-backend`)。

Run: `grep -rn 'server\.exe' scripts/package.sh installer/pyime.nsi crates/pyime-backend/src | grep -v pyime-server`
Expected: 只剩 `pyime.nsi` 的 `Delete "$INSTDIR\pyime\server.exe"` 一行与注释。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/Cargo.toml crates/pyime-backend/src/main.rs crates/pyime-backend/src/register.rs scripts/package.sh installer/pyime.nsi .gitignore
git commit -m "后端:可执行文件改名 pyime-server.exe,GUI 子系统不弹控制台

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 用户数据原子写盘

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs`(新增 `write_atomic`,改 `save_english` 的 `:354`、`save` 的 `:370`)
- Test: `crates/pyime-backend/src/ime/tests.rs`

**Interfaces:**
- Produces: `fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()>`(模块私有,`tests.rs` 经 `use super::*` 可见)。

说明:服务端只写 `user.txt` 与 `en_learned.txt`(`en.txt` 只读不写,见 `ime.rs:130`),本任务只改这两处。临时文件为同目录 `<名>.tmp`(`user.tmp`、`en_learned.tmp`)。不调用 `sync_all`:防的是进程被看门狗/taskkill 结束(数据已在系统缓存),不是断电,省下每次选词一次 fsync。外部修改保护的 stamp 仍在写盘成功后由 `stamp(path)` 读取,即 rename 之后,逻辑不变。

- [ ] **Step 1: 写失败测试**(`ime/tests.rs` 末尾追加)

```rust
#[test]
fn 原子写替换已有文件且不留临时文件() {
    let p = temp_user_path("atomic");
    std::fs::write(&p, "旧内容").unwrap();
    write_atomic(&p, "新内容".as_bytes()).unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "新内容");
    assert!(!p.with_extension("tmp").exists());
}

#[test]
fn 原子写可新建文件() {
    let p = temp_user_path("atomic-new");
    write_atomic(&p, b"ni\t\xe4\xbd\xa0\t1").unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "ni\t你\t1");
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-backend ime::tests::原子写`
Expected: 编译失败,`cannot find function `write_atomic``。

- [ ] **Step 3: 实现**

在 `ime.rs` 的 `fn stamp` 之后插入:
```rust
/// 先写同目录临时文件 `<名>.tmp` 再改名替换:写到一半进程被结束(看门狗、taskkill)时原文件不受损。
/// 改名失败(如目标正被别的程序独占打开)时删掉临时文件,退回直接覆盖写。
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        log::line(&format!("改名 {tmp:?} → {path:?} 失败,改为直接覆盖: {e}"));
        return std::fs::write(path, data);
    }
    Ok(())
}
```

`save_english` 中 `std::fs::write(path, en.export_raw())` 改为 `write_atomic(path, en.export_raw().as_bytes())`;
`save` 中 `std::fs::write(path, engine.user().export())` 改为 `write_atomic(path, engine.user().export().as_bytes())`(两个 export 都返回 `String`,见 `pyime-engine/src/english.rs:155`、`user.rs:139`)。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -q -p pyime-backend`
Expected: 全部 PASS;其中 `文件不存在时照常写_自己写盘后再学习照常写回`、`文件被外部修改后不覆盖` 等原有保护测试保持通过。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/ime.rs crates/pyime-backend/src/ime/tests.rs
git commit -m "后端:user.txt 与 en_learned.txt 改为写临时文件再改名,被结束进程时不截断

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 监护进程重启策略(纯逻辑)

**Files:**
- Create: `crates/pyime-backend/src/supervise.rs`
- Modify: `crates/pyime-backend/src/main.rs`(模块声明)

**Interfaces:**
- Produces:
  - `pub struct RestartPolicy`;`RestartPolicy::new(window: Duration, max: usize) -> RestartPolicy`;`fn on_exit(&mut self, now: Instant) -> bool`(true = 应重启)。
  - `pub const RESTART_WINDOW: Duration`(60s)、`pub const RESTART_MAX: usize`(5)、`pub const MUTEX_NAME: &str`、`pub const QUIT_EVENT: &str`。
  - Task 7 在同文件追加 `#[cfg(windows)] pub fn run()` 与 `pub fn quit()`。

- [ ] **Step 1: 写失败测试**——新建 `supervise.rs`,先只放测试与空壳:

```rust
//! `--supervise` 监护进程:单实例,循环启动 `--worker` 子进程,退出后按策略重启;
//! 收到退出事件(`--quit`)时连同子进程退出。纯逻辑部分(重启策略)各平台可测。
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 监护进程单实例互斥量。
pub const MUTEX_NAME: &str = r"Local\PYIME-supervisor";
/// `--quit` 置位的退出事件。
pub const QUIT_EVENT: &str = r"Local\PYIME-quit";
/// worker 在此时间窗内退出达 `RESTART_MAX` 次即停止重启(词典损坏等致命错误时不无限刷日志)。
pub const RESTART_WINDOW: Duration = Duration::from_secs(60);
pub const RESTART_MAX: usize = 5;

pub struct RestartPolicy {
    window: Duration,
    max: usize,
    exits: VecDeque<Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 窗口内前四次退出都重启_第五次停止() {
        let t0 = Instant::now();
        let mut p = RestartPolicy::new(RESTART_WINDOW, RESTART_MAX);
        for i in 0..4 {
            assert!(p.on_exit(t0 + Duration::from_secs(i)), "第 {} 次", i + 1);
        }
        assert!(!p.on_exit(t0 + Duration::from_secs(10)));
    }

    #[test]
    fn 间隔超过窗口的退出不累计() {
        let t0 = Instant::now();
        let mut p = RestartPolicy::new(RESTART_WINDOW, RESTART_MAX);
        for i in 0..20 {
            assert!(p.on_exit(t0 + Duration::from_secs(i * 61)), "第 {} 次", i + 1);
        }
    }

    #[test]
    fn 旧退出滑出窗口后重新计数() {
        let t0 = Instant::now();
        let mut p = RestartPolicy::new(RESTART_WINDOW, RESTART_MAX);
        for i in 0..4 {
            assert!(p.on_exit(t0 + Duration::from_secs(i)));
        }
        // 60s 后前 4 次都已滑出窗口
        assert!(p.on_exit(t0 + Duration::from_secs(64)));
    }
}
```

`main.rs` 的模块声明区加(按字母序放在 `mod session;` 之后):
```rust
#[cfg_attr(not(windows), allow(dead_code))] // 监护进程只在 Windows 运行;纯逻辑在 Linux 上单测
mod supervise;
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-backend supervise`
Expected: 编译失败,`no function or associated item named `new``。

- [ ] **Step 3: 实现**(插在 `struct RestartPolicy` 之后、`mod tests` 之前)

```rust
impl RestartPolicy {
    pub fn new(window: Duration, max: usize) -> RestartPolicy {
        RestartPolicy { window, max, exits: VecDeque::new() }
    }

    /// 记一次 worker 退出;返回是否应重启。
    pub fn on_exit(&mut self, now: Instant) -> bool {
        while self.exits.front().is_some_and(|&t| now.duration_since(t) >= self.window) {
            self.exits.pop_front();
        }
        self.exits.push_back(now);
        self.exits.len() < self.max
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -q -p pyime-backend supervise`
Expected: `test result: ok. 3 passed`。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/supervise.rs crates/pyime-backend/src/main.rs
git commit -m "后端:监护进程重启策略(60s 内退出 5 次即停止)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: 请求分发与看门狗状态(hub.rs,纯逻辑)

**Files:**
- Create: `crates/pyime-backend/src/hub.rs`
- Modify: `crates/pyime-backend/src/main.rs`(模块声明)

**Interfaces:**
- Consumes: `Session::new(Rc<Ime>) -> Session`、`Session::handle(&mut self, &Value) -> Value`、`Session::debug_state(&self) -> String`(`session.rs:125,160,167`);测试用 `Ime::with_engine(Engine) -> Rc<Ime>`(`ime.rs:205`,`#[cfg(test)]`)。
- Produces(Task 6 使用):
  - `pub enum Msg { Request { conn: u64, body: Vec<u8>, reply: Sender<Vec<u8>> }, Closed { conn: u64 } }`
  - `pub fn run(rx: Receiver<Msg>, ime: Rc<Ime>, busy: &Busy)`:工作线程主循环,所有发送端关闭后返回;无法解析的请求不回复(丢弃 `reply`,I/O 线程据此断开)。
  - `pub struct Busy`(`Default`);`fn begin(&self)`、`fn end(&self)`、`fn stuck(&self, now: u64, limit_ms: u64) -> bool`。
  - `pub fn now_ms() -> u64`、`pub const WATCHDOG_LIMIT_MS: u64 = 5000`。

- [ ] **Step 1: 写失败测试**——新建 `hub.rs`,先放类型、空壳与测试:

```rust
//! 管道连接与会话的分发:所有请求经 mpsc 送到唯一的工作线程,它持有 `Rc<Ime>` 与每个连接的
//! `Session`(session/ime 保持单线程)。连接断开即结束该会话,等价于原 PIME 的 `close`。
use crate::ime::Ime;
use crate::log;
use crate::session::Session;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

/// 单条请求处理超过这么久即视为卡死,看门狗结束 worker 进程,由监护进程重启。
pub const WATCHDOG_LIMIT_MS: u64 = 5000;

pub enum Msg {
    /// 一条请求(管道消息原文)及回传回复的通道。
    Request { conn: u64, body: Vec<u8>, reply: Sender<Vec<u8>> },
    /// 连接断开。
    Closed { conn: u64 },
}

struct Hub {
    ime: Rc<Ime>,
    sessions: HashMap<u64, Session>,
}

/// 工作线程"当前请求开始时刻"(UNIX 毫秒,0 = 空闲),看门狗线程据此判断是否卡死。
#[derive(Default)]
pub struct Busy(AtomicU64);

#[cfg(test)]
mod tests {
    use super::*;
    use pyime_engine::dict::{Dict, DictBuilder};
    use pyime_engine::engine::Engine;
    use pyime_engine::fuzzy::FuzzyConfig;
    use serde_json::json;
    use std::sync::{mpsc, Arc};

    const N: i64 = 78;
    const I: i64 = 73;

    fn engine() -> Engine {
        let mut b = DictBuilder::default();
        b.add("ni", "你", 9000);
        Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default())
    }

    fn req(seq: u64, code: i64) -> Vec<u8> {
        json!({"method": "onKeyDown", "seqNum": seq, "keyCode": code}).to_string().into_bytes()
    }

    fn parse(out: &[u8]) -> Value {
        serde_json::from_slice(out).unwrap()
    }

    #[test]
    fn 不同连接各自独立会话() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        assert_eq!(parse(&hub.request(1, &req(1, N)).unwrap())["compositionString"], "n");
        assert_eq!(parse(&hub.request(2, &req(1, N)).unwrap())["compositionString"], "n");
        assert_eq!(parse(&hub.request(1, &req(2, I)).unwrap())["compositionString"], "ni");
        assert_eq!(hub.session_count(), 2);
    }

    #[test]
    fn 断开即结束会话_同编号重连从空开始() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        hub.request(1, &req(1, N)).unwrap();
        hub.close(1);
        assert_eq!(hub.session_count(), 0);
        assert_eq!(parse(&hub.request(1, &req(2, N)).unwrap())["compositionString"], "n");
    }

    #[test]
    fn 回显序号_结尾换行可解析() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        let mut body = req(42, N);
        body.push(b'\n');
        let r = parse(&hub.request(1, &body).unwrap());
        assert_eq!(r["seqNum"], 42);
        assert_eq!(r["success"], true);
    }

    #[test]
    fn 非法请求不回复() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        assert!(hub.request(1, b"not json").is_none());
        assert!(hub.request(1, &[0xff, 0xfe]).is_none());
    }

    #[test]
    fn 工作线程按顺序回复_非法请求丢弃回复通道() {
        let (tx, rx) = mpsc::channel();
        let busy = Arc::new(Busy::default());
        let b = busy.clone();
        let worker = std::thread::spawn(move || run(rx, Ime::with_engine(engine()), &b));
        let (rtx, rrx) = mpsc::channel();
        for seq in 1..=3 {
            tx.send(Msg::Request { conn: 1, body: req(seq, N), reply: rtx.clone() }).unwrap();
        }
        let seqs: Vec<u64> = (0..3).map(|_| parse(&rrx.recv().unwrap())["seqNum"].as_u64().unwrap()).collect();
        assert_eq!(seqs, [1, 2, 3]);
        let (btx, brx) = mpsc::channel();
        tx.send(Msg::Request { conn: 1, body: b"xx".to_vec(), reply: btx }).unwrap();
        assert!(brx.recv().is_err());
        tx.send(Msg::Closed { conn: 1 }).unwrap();
        drop(tx);
        worker.join().unwrap();
        assert!(!busy.stuck(now_ms() + 60_000, WATCHDOG_LIMIT_MS));
    }

    #[test]
    fn 看门狗只在处理中且超时才触发() {
        let busy = Busy::default();
        assert!(!busy.stuck(now_ms() + 60_000, WATCHDOG_LIMIT_MS));
        busy.begin();
        let now = now_ms();
        assert!(!busy.stuck(now + 1000, WATCHDOG_LIMIT_MS));
        assert!(busy.stuck(now + 6000, WATCHDOG_LIMIT_MS));
        busy.end();
        assert!(!busy.stuck(now + 6000, WATCHDOG_LIMIT_MS));
    }
}
```

`main.rs` 模块声明区加(放在 `mod ime;` 之前):
```rust
#[cfg_attr(not(windows), allow(dead_code))] // 只被 Windows 的 pipe.rs 使用;纯逻辑在 Linux 上单测
mod hub;
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-backend hub`
Expected: 编译失败,`no function or associated item named `new` found for struct `Hub``。

- [ ] **Step 3: 实现**(插在 `pub struct Busy` 之后、`mod tests` 之前)

```rust
impl Hub {
    fn new(ime: Rc<Ime>) -> Hub {
        Hub { ime, sessions: HashMap::new() }
    }

    /// 处理一条请求;不是 UTF-8 JSON 时返回 None(不回复)。日志格式同 stdin 模式,前缀带连接号。
    fn request(&mut self, conn: u64, body: &[u8]) -> Option<Vec<u8>> {
        let Ok(text) = std::str::from_utf8(body) else {
            log::line(&format!("忽略 [{conn}]:请求不是 UTF-8"));
            return None;
        };
        let text = text.trim();
        log::line(&format!("> [{conn}] {text}"));
        let Ok(req) = serde_json::from_str::<Value>(text) else {
            log::line("忽略:请求格式不合法");
            return None;
        };
        let ime = &self.ime;
        let sess = self.sessions.entry(conn).or_insert_with(|| Session::new(ime.clone()));
        let t = std::time::Instant::now();
        let out = sess.handle(&req).to_string();
        log::line(&format!("< [{conn}] {out}"));
        log::line(&format!("  状态: {} 耗时 {:?}", sess.debug_state(), t.elapsed()));
        Some(out.into_bytes())
    }

    fn close(&mut self, conn: u64) {
        if self.sessions.remove(&conn).is_some() {
            log::line(&format!("连接 {conn} 断开,剩余 {} 个会话", self.sessions.len()));
        }
    }

    #[cfg(test)]
    fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

/// 工作线程主循环,直到所有发送端关闭。处理每条请求期间在 `busy` 上记开始时刻供看门狗检查。
pub fn run(rx: Receiver<Msg>, ime: Rc<Ime>, busy: &Busy) {
    let mut hub = Hub::new(ime);
    for msg in rx {
        match msg {
            Msg::Request { conn, body, reply } => {
                busy.begin();
                let out = hub.request(conn, &body);
                busy.end();
                // 无法解析时丢弃 reply,I/O 线程收不到回复即断开该连接
                if let Some(out) = out {
                    let _ = reply.send(out);
                }
            }
            Msg::Closed { conn } => hub.close(conn),
        }
    }
}

impl Busy {
    pub fn begin(&self) {
        self.0.store(now_ms().max(1), Ordering::SeqCst);
    }

    pub fn end(&self) {
        self.0.store(0, Ordering::SeqCst);
    }

    /// 正在处理的请求已超过 `limit_ms` 毫秒。
    pub fn stuck(&self, now: u64, limit_ms: u64) -> bool {
        let start = self.0.load(Ordering::SeqCst);
        start != 0 && now.saturating_sub(start) > limit_ms
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -q -p pyime-backend hub`
Expected: `test result: ok. 6 passed`。

Run: `cargo clippy -q -p pyime-backend --all-targets`
Expected: 无输出(无警告)。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/hub.rs crates/pyime-backend/src/main.rs
git commit -m "后端:管道请求分发到唯一工作线程(连接即会话),看门狗状态

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: win.rs——管道名、SDDL 与 Win32 声明

**Files:**
- Create: `crates/pyime-backend/src/win.rs`
- Modify: `crates/pyime-backend/src/main.rs`(模块声明)

**Interfaces:**
- Produces(各平台):`pub fn pipe_path(user: &str) -> String`、`pub fn pipe_sddl(user_sid: &str) -> String`、`pub fn object_sddl(user_sid: &str) -> String`。
- Produces(`#[cfg(windows)] pub mod sys`,Task 6/7/8 使用):`type Handle = isize`;常量 `INVALID_HANDLE_VALUE`、`ERROR_MORE_DATA`、`ERROR_PIPE_CONNECTED`、`ERROR_ALREADY_EXISTS`、`INFINITE`、`WAIT_OBJECT_0`;`#[repr(C)] struct SecurityAttributes`;`extern "system"` 声明(见下);工具 `wide(&str) -> Vec<u16>`、`last_error() -> u32`、`user_name() -> Option<String>`、`user_sid() -> Option<String>`、`struct Security { attrs }` + `Security::from_sddl(&str) -> Option<Security>`、`read_message(Handle) -> Option<Vec<u8>>`、`write_message(Handle, &[u8]) -> bool`。

SDDL 依据 PIME `PIMELauncher/PipeSecurity.cpp:123-149`:拒绝 NETWORK(`NU`),允许 SYSTEM(`SY`)、Administrators(`BA`)、ALL APPLICATION PACKAGES(`AC`)、当前用户;SACL 低完整性标签 `(ML;;NWNR;;;LW)`。与原版的差别:原版授权"登录 SID",这里授权当前用户 SID(`TokenUser`),取法更简单;管道名已含用户名,效果相同。退出事件/互斥量用 `object_sddl`:只授 SYSTEM、Administrators、当前用户——**不授 `AC`**,否则任何商店应用都能让输入法退出。

- [ ] **Step 1: 写失败测试**——新建 `win.rs`,先放测试:

```rust
//! Win32 小工具:管道名、安全描述符 SDDL(纯字符串,各平台可测),以及 Windows 下的
//! `extern "system"` 声明与消息模式管道收发。沿用 main.rs 手写声明的写法,不引入 windows crate。
//! `src/bin/pyime-pipe-cli.rs` 经 `#[path]` 复用本文件。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 管道名带用户名() {
        assert_eq!(pipe_path("chenyu"), r"\\.\pipe\chenyu\PYIME");
    }

    #[test]
    fn 管道sddl同_pime_launcher() {
        assert_eq!(
            pipe_sddl("S-1-5-21-1-2-3-1001"),
            "D:(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;AC)(A;;GA;;;S-1-5-21-1-2-3-1001)S:(ML;;NWNR;;;LW)"
        );
    }

    #[test]
    fn 内核对象sddl不授予应用容器() {
        let s = object_sddl("S-1-5-21-1-2-3-1001");
        assert_eq!(s, "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;S-1-5-21-1-2-3-1001)");
        assert!(!s.contains(";AC)"));
    }
}
```

`main.rs` 模块声明区末尾加:
```rust
#[allow(dead_code)] // sys 里部分声明(CreateFileW 等)只给 pyime-pipe-cli 用;纯字符串函数在 Linux 上只被测试用
mod win;
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -q -p pyime-backend win::tests`
Expected: 编译失败,`cannot find function `pipe_path``。

- [ ] **Step 3: 实现**——在 `win.rs` 的 `mod tests` 之前插入纯字符串部分:

```rust
/// 管道路径 `\\.\pipe\<用户名>\PYIME`:带用户名,多用户不串。
pub fn pipe_path(user: &str) -> String {
    format!(r"\\.\pipe\{user}\PYIME")
}

/// 管道安全描述符(同 PIME Launcher,`PipeSecurity.cpp:123-149`):拒绝网络访问;允许 SYSTEM、
/// Administrators、ALL APPLICATION PACKAGES 与当前用户;低完整性标签,让 AppContainer/低完整性宿主里的 DLL 能连上。
pub fn pipe_sddl(user_sid: &str) -> String {
    format!("D:(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;AC)(A;;GA;;;{user_sid})S:(ML;;NWNR;;;LW)")
}

/// 退出事件与互斥量的安全描述符:只允许 SYSTEM、Administrators(提权安装器)与当前用户,不对应用容器开放。
pub fn object_sddl(user_sid: &str) -> String {
    format!("D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{user_sid})")
}
```

接着插入 Windows 部分(第一段:声明):
```rust
#[cfg(windows)]
pub mod sys {
    use std::ffi::c_void;

    pub type Handle = isize;
    pub const INVALID_HANDLE_VALUE: Handle = -1;
    pub const ERROR_MORE_DATA: u32 = 234;
    pub const ERROR_PIPE_CONNECTED: u32 = 535;
    pub const ERROR_ALREADY_EXISTS: u32 = 183;
    pub const INFINITE: u32 = 0xFFFF_FFFF;
    pub const WAIT_OBJECT_0: u32 = 0;

    #[repr(C)]
    pub struct SecurityAttributes {
        pub length: u32,
        pub descriptor: *mut c_void,
        pub inherit: i32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateNamedPipeW(name: *const u16, open_mode: u32, pipe_mode: u32, max_instances: u32, out_size: u32, in_size: u32, timeout_ms: u32, sa: *const SecurityAttributes) -> Handle;
        pub fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
        pub fn DisconnectNamedPipe(pipe: Handle) -> i32;
        pub fn CreateFileW(name: *const u16, access: u32, share: u32, sa: *const SecurityAttributes, disposition: u32, flags: u32, template: Handle) -> Handle;
        pub fn SetNamedPipeHandleState(pipe: Handle, mode: *const u32, max_count: *const u32, timeout: *const u32) -> i32;
        pub fn ReadFile(h: Handle, buf: *mut u8, len: u32, read: *mut u32, overlapped: *mut c_void) -> i32;
        pub fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, overlapped: *mut c_void) -> i32;
        pub fn CloseHandle(h: Handle) -> i32;
        pub fn GetLastError() -> u32;
        pub fn LocalFree(mem: *mut c_void) -> *mut c_void;
        pub fn GetCurrentProcess() -> Handle;
        pub fn CreateMutexW(sa: *const SecurityAttributes, initial_owner: i32, name: *const u16) -> Handle;
        pub fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> Handle;
        pub fn CreateEventW(sa: *const SecurityAttributes, manual_reset: i32, initial_state: i32, name: *const u16) -> Handle;
        pub fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> Handle;
        pub fn SetEvent(h: Handle) -> i32;
        pub fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
        pub fn WaitForMultipleObjects(count: u32, handles: *const Handle, wait_all: i32, ms: u32) -> u32;
    }

    #[link(name = "advapi32")]
    extern "system" {
        pub fn GetUserNameW(buf: *mut u16, len: *mut u32) -> i32;
        pub fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        pub fn GetTokenInformation(token: Handle, class: u32, info: *mut c_void, len: u32, ret_len: *mut u32) -> i32;
        pub fn ConvertSidToStringSidW(sid: *mut c_void, out: *mut *mut u16) -> i32;
        pub fn ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl: *const u16, revision: u32, sd: *mut *mut c_void, size: *mut u32) -> i32;
    }
```

第二段:工具函数(紧接上段,仍在 `mod sys` 内,最后的 `}` 关闭 `mod sys`):
```rust
    /// 以 NUL 结尾的 UTF-16。
    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn last_error() -> u32 {
        // SAFETY: 无参数,只读线程的最近错误码。
        unsafe { GetLastError() }
    }

    /// 当前进程的用户名(管道名用)。
    pub fn user_name() -> Option<String> {
        let mut buf = [0u16; 257];
        let mut len = buf.len() as u32;
        // SAFETY: buf 与 len 是本地变量,长度如实传入。
        if unsafe { GetUserNameW(buf.as_mut_ptr(), &mut len) } == 0 {
            return None;
        }
        // 成功时 len 含结尾 NUL
        Some(String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]))
    }

    /// 当前进程用户的 SID 字符串(如 `S-1-5-21-…`)。
    pub fn user_sid() -> Option<String> {
        const TOKEN_QUERY: u32 = 0x0008;
        const TOKEN_USER: u32 = 1;
        let mut token: Handle = 0;
        // u64 数组保证 TOKEN_USER 结构里指针字段对齐
        let mut buf = [0u64; 64];
        let mut len = 0u32;
        // SAFETY: token/buf/len 均为本地变量;TOKEN_USER 的首字段是指向 buf 内部的 SID 指针,
        // ConvertSidToStringSidW 分配的字符串读完后 LocalFree。
        unsafe {
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return None;
            }
            let ok = GetTokenInformation(token, TOKEN_USER, buf.as_mut_ptr().cast(), std::mem::size_of_val(&buf) as u32, &mut len);
            CloseHandle(token);
            if ok == 0 {
                return None;
            }
            let sid = *(buf.as_ptr() as *const *mut c_void);
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(sid, &mut s) == 0 {
                return None;
            }
            let n = (0..).take_while(|&i| *s.add(i) != 0).count();
            let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s.cast());
            Some(out)
        }
    }

    /// 由 SDDL 生成的安全属性,析构时释放描述符。
    pub struct Security {
        pub attrs: SecurityAttributes,
    }

    impl Security {
        pub fn from_sddl(sddl: &str) -> Option<Security> {
            let w = wide(sddl);
            let mut sd: *mut c_void = std::ptr::null_mut();
            // SAFETY: w 以 NUL 结尾;sd 由系统分配,Drop 时 LocalFree。
            if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(w.as_ptr(), 1, &mut sd, std::ptr::null_mut()) } == 0 {
                return None;
            }
            let length = std::mem::size_of::<SecurityAttributes>() as u32;
            Some(Security { attrs: SecurityAttributes { length, descriptor: sd, inherit: 0 } })
        }
    }

    impl Drop for Security {
        fn drop(&mut self) {
            // SAFETY: descriptor 来自 ConvertStringSecurityDescriptorToSecurityDescriptorW,只释放一次。
            unsafe { LocalFree(self.attrs.descriptor) };
        }
    }

    /// 读一条完整消息(消息模式;超过缓冲区时循环处理 ERROR_MORE_DATA)。对端断开或出错返回 None。
    pub fn read_message(h: Handle) -> Option<Vec<u8>> {
        let mut msg = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let mut n = 0u32;
            // SAFETY: buf 为本地缓冲区,长度如实传入;同步句柄,不传 OVERLAPPED。
            let ok = unsafe { ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) };
            msg.extend_from_slice(&buf[..n as usize]);
            if ok != 0 {
                return Some(msg);
            }
            if last_error() != ERROR_MORE_DATA {
                return None;
            }
        }
    }

    /// 写一条完整消息。
    pub fn write_message(h: Handle, data: &[u8]) -> bool {
        let mut n = 0u32;
        // SAFETY: data 在调用期间有效,长度如实传入;同步句柄。
        let ok = unsafe { WriteFile(h, data.as_ptr(), data.len() as u32, &mut n, std::ptr::null_mut()) };
        ok != 0 && n as usize == data.len()
    }
}
```

- [ ] **Step 4: 运行确认通过,并交叉编译**

Run: `cargo test -q -p pyime-backend win::tests`
Expected: `test result: ok. 3 passed`。

Run: `cargo build -q --release --target x86_64-pc-windows-gnu -p pyime-backend`
Expected: 成功,无警告。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/win.rs crates/pyime-backend/src/main.rs
git commit -m "后端:管道名、管道与内核对象 SDDL,Win32 管道收发声明

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: `--worker` 管道服务端(pipe.rs)与参数分派

**Files:**
- Create: `crates/pyime-backend/src/pipe.rs`
- Modify: `crates/pyime-backend/src/main.rs`(模块声明、`start_logs`、`run_windows_mode`、stdin 模式开头)

**Interfaces:**
- Consumes: `hub::{run, Msg, Busy, now_ms, WATCHDOG_LIMIT_MS}`(Task 4);`win::{pipe_path, pipe_sddl}`、`win::sys::*`(Task 5);`ime::Ime::load() -> Rc<Ime>`(`ime.rs:183`)。
- Produces: `#[cfg(windows)] pub fn pipe::run()`;`main.rs` 的 `fn start_logs(args: &[String])`、`fn run_windows_mode(args: &[String]) -> bool`(Task 7 往里加分支)。

本任务全是 Win32,Linux 上无单测;验收靠交叉编译、Windows 目标 clippy 与 Task 9 实机清单。

- [ ] **Step 1: 新建 `pipe.rs`**

```rust
//! `--worker`:命名管道服务端。先加载引擎再建管道(加载期间 DLL 连不上,按键放行);
//! 每个连接一个阻塞 I/O 线程,请求经 mpsc 送唯一工作线程(见 `hub`);
//! 看门狗发现单条请求处理超过 5s 即结束进程,由监护进程重启。
use crate::hub::{self, Busy, Msg};
use crate::win::sys::*;
use crate::{ime, log, win};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const PIPE_ACCESS_DUPLEX: u32 = 0x3;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
const PIPE_TYPE_MESSAGE: u32 = 0x4;
const PIPE_READMODE_MESSAGE: u32 = 0x2;
const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x8;
const PIPE_UNLIMITED_INSTANCES: u32 = 255;
const BUF_SIZE: u32 = 64 * 1024;

/// 建一个管道实例。`first` 为真时带 FILE_FLAG_FIRST_PIPE_INSTANCE:名字已被占用(已有 worker 或冒名者)即失败。
fn create_instance(name: &[u16], sec: &Security, first: bool) -> Option<Handle> {
    let open_mode = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
    let pipe_mode = PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_REJECT_REMOTE_CLIENTS;
    // SAFETY: name 以 NUL 结尾,sec 在调用期间有效。
    let h = unsafe { CreateNamedPipeW(name.as_ptr(), open_mode, pipe_mode, PIPE_UNLIMITED_INSTANCES, BUF_SIZE, BUF_SIZE, 0, &sec.attrs) };
    if h == INVALID_HANDLE_VALUE {
        log::line(&format!("建管道实例失败: 错误码 {}", last_error()));
        return None;
    }
    Some(h)
}

/// 一个连接的 I/O:读请求 → 交工作线程 → 写回复。对端断开、工作线程不回复(请求无法解析)或写失败即结束。
fn serve(h: Handle, conn: u64, tx: mpsc::Sender<Msg>) {
    while let Some(body) = read_message(h) {
        let (reply_tx, reply_rx) = mpsc::channel();
        if tx.send(Msg::Request { conn, body, reply: reply_tx }).is_err() {
            break;
        }
        let Ok(out) = reply_rx.recv() else { break };
        if !write_message(h, &out) {
            break;
        }
    }
    let _ = tx.send(Msg::Closed { conn });
    // SAFETY: h 是本线程独占的管道实例句柄,此后不再使用。
    unsafe {
        DisconnectNamedPipe(h);
        CloseHandle(h);
    }
}

pub fn run() {
    let Some(user) = user_name() else { return log::line("取不到用户名,退出") };
    let Some(sid) = user_sid() else { return log::line("取不到用户 SID,退出") };
    let Some(sec) = Security::from_sddl(&win::pipe_sddl(&sid)) else {
        return log::line(&format!("管道安全描述符无效: 错误码 {}", last_error()));
    };
    let path = win::pipe_path(&user);
    let name = wide(&path);
    let busy = Arc::new(Busy::default());
    let (tx, rx) = mpsc::channel::<Msg>();
    let (ready_tx, ready_rx) = mpsc::channel();
    let b = busy.clone();
    std::thread::spawn(move || {
        let ime = ime::Ime::load();
        let _ = ready_tx.send(());
        hub::run(rx, ime, &b);
    });
    // 引擎加载 panic 时 ready_tx 随线程销毁,recv 返回错误
    if ready_rx.recv().is_err() {
        return log::line("引擎加载失败,退出");
    }
    let b = busy.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if b.stuck(hub::now_ms(), hub::WATCHDOG_LIMIT_MS) {
            log::line("看门狗:单条请求处理超过 5s,结束 worker");
            std::process::exit(3);
        }
    });
    let Some(mut h) = create_instance(&name, &sec, true) else {
        return log::line(&format!("{path} 已被占用(已有 worker?),退出"));
    };
    log::line(&format!("管道就绪: {path}"));
    let mut conn = 0u64;
    loop {
        // SAFETY: h 是有效的管道实例句柄;同步等待客户端连接。
        let connected = unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) } != 0 || last_error() == ERROR_PIPE_CONNECTED;
        // 先建好下一个实例再分发,两次连接之间客户端不会找不到管道
        let Some(next) = create_instance(&name, &sec, false) else { break };
        if connected {
            conn += 1;
            let tx = tx.clone();
            std::thread::spawn(move || serve(h, conn, tx));
        } else {
            log::line(&format!("等待连接失败: 错误码 {}", last_error()));
            // SAFETY: 未交给任何线程的实例句柄,关闭后不再使用。
            unsafe { CloseHandle(h) };
        }
        h = next;
    }
    log::line("管道服务结束,退出");
}
```

- [ ] **Step 2: 改 `main.rs`**

模块声明区加:
```rust
#[cfg(windows)]
mod pipe;
```

在 `fn main` 之前加:
```rust
/// 打开日志、输入日志与 panic 钩子,记启动信息。
fn start_logs(args: &[String]) {
    log::init();
    inputlog::init();
    log::install_panic_hook();
    log::line(&format!(
        "启动 pyime-server {} args={:?} exe={:?}",
        env!("CARGO_PKG_VERSION"),
        args,
        std::env::current_exe().ok()
    ));
}

/// Windows 专有模式:`--worker` 管道服务(`--supervise`/`--quit` 见 supervise.rs)。处理了返回 true。
#[cfg(windows)]
fn run_windows_mode(args: &[String]) -> bool {
    match args.get(1).map(String::as_str) {
        Some("--worker") => {
            start_logs(args);
            pipe::run();
        }
        _ => return false,
    }
    true
}

#[cfg(not(windows))]
fn run_windows_mode(_: &[String]) -> bool {
    false
}
```

`fn main` 中从 `let stdin = io::stdin();` 到启动日志 `log::line(&format!("启动 pyime-server …"));` 这一段(原 `main.rs:59-73`)整段替换为:
```rust
    if run_windows_mode(&args) {
        return;
    }
    // 无参数:旧 PIME 后端模式,经 PIMELauncher 的 stdin/stdout 行协议通信(实施第 6 步删除)
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut sessions: HashMap<String, session::Session> = HashMap::new();
    start_logs(&args);
    #[cfg(windows)]
    disinherit_std_handles();
    let ime = ime::Ime::load();
```
(启动日志现在在加载引擎之前写,其余 stdin 循环不变。)

- [ ] **Step 3: 编译与检查**

Run: `cargo test -q -p pyime-backend`
Expected: 全部 PASS。

Run: `cargo clippy -q -p pyime-backend --all-targets && cargo clippy -q -p pyime-backend --target x86_64-pc-windows-gnu`
Expected: Linux 目标无输出;Windows 目标只有 `supervise.rs` 中 `MUTEX_NAME`、`QUIT_EVENT`、`RESTART_WINDOW`、`RESTART_MAX`、`RestartPolicy` 未使用的 dead_code 警告(Task 7 接上后消失),别无其他。

Run: `cargo build -q --release --target x86_64-pc-windows-gnu -p pyime-backend && x86_64-w64-mingw32-objdump -p target/x86_64-pc-windows-gnu/release/pyime-server.exe | grep 'DLL Name'`
Expected: 含 `ADVAPI32.dll`、`KERNEL32.dll`;不含 `libgcc`、`libwinpthread`。

- [ ] **Step 4: Commit**

```bash
git add crates/pyime-backend/src/pipe.rs crates/pyime-backend/src/main.rs
git commit -m "后端:--worker 命名管道服务端(先加载引擎再建管道,每连接一线程,看门狗 5s)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: `--supervise` 监护进程与 `--quit`

**Files:**
- Modify: `crates/pyime-backend/src/supervise.rs`(追加 Windows 部分)
- Modify: `crates/pyime-backend/src/main.rs`(`run_windows_mode` 加两个分支)

**Interfaces:**
- Consumes: `RestartPolicy`、`MUTEX_NAME`、`QUIT_EVENT`、`RESTART_*`(Task 3);`win::object_sddl`、`win::sys::*`(Task 5)。
- Produces: `#[cfg(windows)] pub fn run()`、`#[cfg(windows)] pub fn quit()`。计划 C 的安装器调用 `pyime-server.exe --supervise`(Run 键、ExecShellAsUser)与 `pyime-server.exe --quit`。

行为:
- `run`:建 `Local\PYIME-supervisor` 互斥量,已存在即静默退出(不写日志,避免多次启动刷屏);建手动复位事件 `Local\PYIME-quit`;循环启动 `<自身> --worker` 并 `WaitForMultipleObjects` 同时等 worker 退出与退出事件。事件触发 → 结束 worker 并退出;worker 退出 → `RestartPolicy::on_exit`,允许则等 500ms 重启,否则记日志退出。两者的安全描述符均用 `object_sddl`(当前用户 + Administrators + SYSTEM),提权安装器能打开。
- `quit`:打开事件(不存在 = 监护进程不在,立即返回)、打开互斥量、置位事件,在互斥量上等 ≤3s(监护进程退出时互斥量被遗弃,等待即返回)。

- [ ] **Step 1: 在 `supervise.rs` 的 `mod tests` 之前追加**

```rust
#[cfg(windows)]
pub fn run() {
    use crate::win::sys::*;
    use crate::{log, win};
    use std::os::windows::io::AsRawHandle;

    log::init();
    log::install_panic_hook();
    let Some(sec) = user_sid().and_then(|sid| Security::from_sddl(&win::object_sddl(&sid))) else {
        return log::line("监护进程:安全描述符无效,退出");
    };
    // SAFETY: 名字以 NUL 结尾(临时 Vec 活到语句结束);sec 在调用期间有效。
    let mutex = unsafe { CreateMutexW(&sec.attrs, 1, wide(MUTEX_NAME).as_ptr()) };
    if mutex == 0 || last_error() == ERROR_ALREADY_EXISTS {
        return; // 已有监护进程
    }
    // SAFETY: 同上;手动复位、初始未置位。
    let quit = unsafe { CreateEventW(&sec.attrs, 1, 0, wide(QUIT_EVENT).as_ptr()) };
    if quit == 0 {
        return log::line(&format!("监护进程:建退出事件失败: 错误码 {}", last_error()));
    }
    let Ok(exe) = std::env::current_exe() else { return log::line("监护进程:取不到自身路径,退出") };
    log::line(&format!("监护进程启动 pyime-server {}", env!("CARGO_PKG_VERSION")));
    let mut policy = RestartPolicy::new(RESTART_WINDOW, RESTART_MAX);
    loop {
        let mut child = match std::process::Command::new(&exe).arg("--worker").spawn() {
            Ok(c) => c,
            Err(e) => return log::line(&format!("监护进程:启动 worker 失败,退出: {e}")),
        };
        let handles = [child.as_raw_handle() as Handle, quit];
        // SAFETY: 两个句柄在等待期间都有效(child 未被 drop,quit 未关闭)。
        let r = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
        if r != WAIT_OBJECT_0 {
            log::line(if r == WAIT_OBJECT_0 + 1 { "监护进程:收到退出通知" } else { "监护进程:等待失败,退出" });
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        log::line(&format!("worker 退出: {:?}", child.wait()));
        if !policy.on_exit(Instant::now()) {
            return log::line("worker 60s 内退出 5 次,停止重启");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// `--quit`:通知监护进程连同 worker 退出,最多等 3s;监护进程不在时立即返回。
#[cfg(windows)]
pub fn quit() {
    use crate::win::sys::*;
    const EVENT_MODIFY_STATE: u32 = 0x0002;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    // SAFETY: 名字以 NUL 结尾;打开的句柄在本函数末尾关闭。
    unsafe {
        let event = OpenEventW(EVENT_MODIFY_STATE, 0, wide(QUIT_EVENT).as_ptr());
        if event == 0 {
            return;
        }
        let mutex = OpenMutexW(SYNCHRONIZE, 0, wide(MUTEX_NAME).as_ptr());
        SetEvent(event);
        if mutex != 0 {
            WaitForSingleObject(mutex, 3000);
            CloseHandle(mutex);
        }
        CloseHandle(event);
    }
}
```

- [ ] **Step 2: `main.rs` 的 `run_windows_mode` 在 `Some("--worker")` 分支之后加**

```rust
        Some("--supervise") => supervise::run(),
        Some("--quit") => supervise::quit(),
```
并把该函数文档注释改为 `/// Windows 专有模式:`--worker` 管道服务、`--supervise` 监护进程、`--quit` 通知退出。处理了返回 true。`

- [ ] **Step 3: 测试与检查**

Run: `cargo test -q -p pyime-backend`
Expected: 全部 PASS。

Run: `cargo clippy -q -p pyime-backend --all-targets && cargo clippy -q -p pyime-backend --target x86_64-pc-windows-gnu`
Expected: 均无输出。

- [ ] **Step 4: Commit**

```bash
git add crates/pyime-backend/src/supervise.rs crates/pyime-backend/src/main.rs
git commit -m "后端:--supervise 监护进程(单实例、崩溃重启)与 --quit

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: 开发工具 pyime-pipe-cli

**Files:**
- Create: `crates/pyime-backend/src/bin/pyime-pipe-cli.rs`

**Interfaces:**
- Consumes: `win.rs` 整个文件经 `#[path = "../win.rs"]` 编入(`pipe_path`、`sys::{CreateFileW, SetNamedPipeHandleState, read_message, write_message, user_name, wide, last_error, CloseHandle, INVALID_HANDLE_VALUE}`)。
- Produces: `target/x86_64-pc-windows-gnu/release/pyime-pipe-cli.exe`;从 stdin 逐行读 JSON 请求,每行发一条消息,打印回复与耗时。不进安装包。

- [ ] **Step 1: 新建文件**

```rust
//! 开发工具:直连 `pyime-server --worker` 的命名管道,从 stdin 逐行读 JSON 请求、打印回复与耗时。
//! 用法(Windows):`pyime-pipe-cli < pipe-requests.txt`,或交互输入,每行一个 JSON。不进安装包。
#[cfg(windows)]
#[path = "../win.rs"]
#[allow(dead_code)]
mod win;

#[cfg(not(windows))]
fn main() {
    eprintln!("pyime-pipe-cli 仅在 Windows 上可用");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    use std::io::BufRead;
    use win::sys::*;
    const GENERIC_READ: u32 = 0x8000_0000;
    // 与 DLL 相同的访问掩码:不要 GENERIC_WRITE(含建管道实例权限),见 win.rs 的 pipe_sddl
    const FILE_WRITE_DATA: u32 = 0x2;
    const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
    const OPEN_EXISTING: u32 = 3;
    const PIPE_READMODE_MESSAGE: u32 = 0x2;

    let Some(user) = user_name() else {
        eprintln!("取不到用户名");
        std::process::exit(1);
    };
    let path = win::pipe_path(&user);
    let name = wide(&path);
    // SAFETY: name 以 NUL 结尾;不传安全属性与模板。
    let h = unsafe { CreateFileW(name.as_ptr(), GENERIC_READ | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES, 0, std::ptr::null(), OPEN_EXISTING, 0, 0) };
    if h == INVALID_HANDLE_VALUE {
        eprintln!("连不上 {path}: 错误码 {}", last_error());
        std::process::exit(1);
    }
    let mode = PIPE_READMODE_MESSAGE;
    // SAFETY: h 为刚打开的管道句柄,mode 为本地变量。
    unsafe { SetNamedPipeHandleState(h, &mode, std::ptr::null(), std::ptr::null()) };
    eprintln!("已连接 {path}");
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let t = std::time::Instant::now();
        if !write_message(h, line.as_bytes()) {
            eprintln!("写入失败,服务端已断开");
            break;
        }
        match read_message(h) {
            Some(r) => println!("{}  ({:?})", String::from_utf8_lossy(&r), t.elapsed()),
            None => {
                eprintln!("服务端断开(请求无法解析或 worker 已退出)");
                break;
            }
        }
    }
    // SAFETY: h 此后不再使用。
    unsafe { CloseHandle(h) };
}
```

- [ ] **Step 2: 编译与检查**

Run: `cargo test -q -p pyime-backend && cargo clippy -q -p pyime-backend --all-targets && cargo clippy -q -p pyime-backend --target x86_64-pc-windows-gnu --bins`
Expected: 测试全 PASS,clippy 无输出。

Run: `cargo build -q --release --target x86_64-pc-windows-gnu -p pyime-backend && ls target/x86_64-pc-windows-gnu/release/pyime-pipe-cli.exe`
Expected: 文件存在。

- [ ] **Step 3: Commit**

```bash
git add crates/pyime-backend/src/bin/pyime-pipe-cli.rs
git commit -m "工具:pyime-pipe-cli 直连管道发请求,用于实机验收服务端

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: 实机验收包与清单

**Files:**
- Create: `pime/test-kit/pipe-requests.txt`(纯 ASCII,无 BOM——cli 逐行当 JSON 发送,BOM 会让首行解析失败)
- Create: `pime/test-kit/pipecheck.txt`(UTF-8 BOM + CRLF,同 `menucheck.txt`)

**Interfaces:**
- Consumes: Task 1-8 全部产物。
- Produces: `installer/PYIME-0.0.0-pipecheck-setup.exe` 与 `dist/pipecheck/`(cli + 两个文本),交用户在 Win11 上验收。

- [ ] **Step 1: 写 `pipe-requests.txt`**

```bash
cat > pime/test-kit/pipe-requests.txt <<'REQ'
{"method":"init","seqNum":1,"id":"pipecheck","isWindows8Above":true}
{"method":"onActivate","seqNum":2}
{"method":"filterKeyDown","seqNum":3,"keyCode":78,"charCode":110}
{"method":"onKeyDown","seqNum":4,"keyCode":78,"charCode":110}
{"method":"filterKeyDown","seqNum":5,"keyCode":73,"charCode":105}
{"method":"onKeyDown","seqNum":6,"keyCode":73,"charCode":105}
{"method":"filterKeyDown","seqNum":7,"keyCode":32,"charCode":32}
{"method":"onKeyDown","seqNum":8,"keyCode":32,"charCode":32}
REQ
```

- [ ] **Step 2: 写 `pipecheck.txt`**(先按 LF 写,再转 BOM + CRLF)

```bash
cat > /tmp/pipecheck.txt <<'DOC'
PYIME 0.0.0-pipecheck 实机验证清单(服务端管道模式)

本包两件事:① 后端改名 pyime-server.exe 后,PIME 下照常打字(回归);② 手工启动新的管道服务端,用 pyime-pipe-cli 验收。
每项记"通过 / 不通过 + 现象"。② 的日志在 %TEMP%\pyime-pipetest\PYIME\log.txt,不碰你平时的 user.txt。

一、安装与回归(PIME 下照常使用)
 1. 双击 PYIME-0.0.0-pipecheck-setup.exe 覆盖安装。完成后 C:\Program Files (x86)\PIME\pyime\ 下有 pyime-server.exe,没有 server.exe。
 2. 记事本里用 PYIME 打几句话、选几个词:与 0.1.15 相同。
 3. 托盘右键菜单四项都能用;全程没有弹出黑色控制台窗口。

二、管道服务端(在普通、非管理员的"命令提示符"里操作)
 4. 执行:
      set LOCALAPPDATA=%TEMP%\pyime-pipetest
      "C:\Program Files (x86)\PIME\pyime\pyime-server.exe" --supervise
    命令立即返回。任务管理器"详细信息"页(右键列标题加"命令行"列):多出两个 pyime-server.exe,命令行分别以 --supervise、--worker 结尾。
 5. 再执行一次第 4 步的第二行:不会多出进程(单实例)。
 6. 把 dist\pipecheck 文件夹拷到本机,进入该文件夹执行:
      pyime-pipe-cli < pipe-requests.txt
    先显示"已连接 \\.\pipe\<你的用户名>\PYIME",随后 8 行回复,seqNum 依次为 1 到 8;seqNum 4 的 compositionString 为 "n",seqNum 6 为 "ni" 且 candidateList 里有"你",seqNum 8 带 commitString(首选词)。每行末尾括号里的耗时应在几十毫秒以内。
 7. 第 6 步后 %TEMP%\pyime-pipetest\PYIME\ 下有 user.txt,没有 user.tmp。
 8. 开两个命令提示符,各执行 pyime-pipe-cli(不带文件,手工逐行粘贴 pipe-requests.txt 的第 3、4 行,再在另一个窗口粘贴第 5、6 行):两边的 compositionString 互不影响(一边 "n",另一边 "i")。log.txt 里请求行前缀 [1]、[2] 不同。
 9. 在第 4 步的窗口执行:
      "C:\Program Files (x86)\PIME\pyime\pyime-server.exe" --worker
    立即返回;log.txt 末尾有"已被占用(已有 worker?),退出"。

三、崩溃恢复与退出
10. 任务管理器结束命令行以 --worker 结尾的那个进程:约 1 秒内出现新的 --worker 进程;再跑一次第 6 步,结果同前。
11. 60 秒内连续结束 --worker 5 次:第 5 次后不再出现新 worker,--supervise 进程也退出;log.txt 有"worker 60s 内退出 5 次,停止重启"。
12. 重新执行第 4 步,再执行:
      "C:\Program Files (x86)\PIME\pyime\pyime-server.exe" --quit
    3 秒内返回,两个进程都消失;log.txt 有"监护进程:收到退出通知"。
13. 监护进程不在时再执行一次 --quit:立即返回。
14. 重新执行第 4 步,然后在"以管理员身份运行"的命令提示符里执行 --quit(第 12 步的命令):两个进程同样消失。

做完后执行第 12 步的 --quit 收尾;%TEMP%\pyime-pipetest 可删除。
看门狗(单条请求超过 5 秒结束 worker)无法在实机上人为触发,不在本清单内。
DOC
{ printf '\xEF\xBB\xBF'; sed 's/$/\r/' /tmp/pipecheck.txt; } > pime/test-kit/pipecheck.txt
head -c 3 pime/test-kit/pipecheck.txt | od -An -tx1
```
Expected: ` ef bb bf`。

- [ ] **Step 3: 全量检查并出包**

Run: `cargo test -q --workspace && cargo clippy -q --workspace --all-targets && cargo clippy -q -p pyime-backend --target x86_64-pc-windows-gnu --bins`
Expected: 测试全 PASS(各 `test result: ok.`),clippy 无输出。

Run: `scripts/package.sh 0.0.0-pipecheck > /tmp/pkg-pipecheck.log 2>&1; echo exit=$?; grep -c 'test result: ok' /tmp/pkg-pipecheck.log; ls -la installer/PYIME-0.0.0-pipecheck-setup.exe`
Expected: `exit=0`,安装包存在。

Run: `x86_64-w64-mingw32-objdump -p installer/pyime-server.exe | grep 'DLL Name'`
Expected: 不含 `libgcc`、`libwinpthread`。

Run:
```bash
rm -rf dist/pipecheck && mkdir -p dist/pipecheck
cp target/x86_64-pc-windows-gnu/release/pyime-pipe-cli.exe pime/test-kit/pipe-requests.txt pime/test-kit/pipecheck.txt installer/PYIME-0.0.0-pipecheck-setup.exe dist/pipecheck/
ls dist/pipecheck
```
Expected: 四个文件。

- [ ] **Step 4: Commit**

```bash
git add pime/test-kit/pipe-requests.txt pime/test-kit/pipecheck.txt
git commit -m "测试包:服务端管道模式实机验证清单与 cli 样例请求

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 5: 交用户实机验证**

把 `dist/pipecheck/` 交给用户,按 `pipecheck.txt` 逐项验证;任一项不通过,收集 `%TEMP%\pyime-pipetest\PYIME\log.txt` 后按 superpowers:systematic-debugging 排查。全部通过后才进入计划 A 第 3 步(DLL 改连新管道)。

---

## 自查记录(写计划时已核对)

- 设计组件 2 各条与任务对应:改名/GUI 子系统 → T1;原子写 → T2;重启策略 → T3、T7;单线程分发/断开即 close → T4;SDDL/管道名 → T5;FIRST_PIPE_INSTANCE、先建下一实例、ERROR_MORE_DATA、先加载引擎再建管道、看门狗 5s → T5、T6;`--supervise`/`--quit`/命名事件与互斥量 → T7;`pyime-pipe-cli` → T8;保留 stdin 模式 → T6(stdin 段落只前移日志);Windows 实机项 → T9。
- 名称一致:`Msg::{Request, Closed}`、`hub::run`、`Busy::{begin, end, stuck}`、`hub::now_ms`、`WATCHDOG_LIMIT_MS`、`win::{pipe_path, pipe_sddl, object_sddl}`、`win::sys::{Security, read_message, write_message, user_name, user_sid, wide, last_error}`、`supervise::{run, quit, RestartPolicy, MUTEX_NAME, QUIT_EVENT}`、`main::{start_logs, run_windows_mode}`。
