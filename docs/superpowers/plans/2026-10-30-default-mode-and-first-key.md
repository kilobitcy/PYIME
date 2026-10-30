# 按程序默认中/英文 + 首键丢失 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 输入法默认英文、`chinese-apps.txt` 所列程序默认中文(托盘可勾选);worker 重启后的首键不再丢失,且中/英状态保留。

**Architecture:** 服务端(Rust)按管道对端进程的 exe 名决定新会话的初始中/英,列表存 `%LOCALAPPDATA%\PYIME\chinese-apps.txt`;
回复里附 `english`,DLL(C++)记住并在重连 `init` 时带回。DLL 在按键类请求遇到"管道已断"且无组字时,清按钮、重连、重发一次。

**Tech Stack:** Rust(`pyime-backend`,Linux 单测 + mingw 交叉编译)、C++17(`tsf/`,mingw 交叉编译,纯逻辑头文件用本机 g++ 单测)。

**设计文档:** `docs/superpowers/specs/2026-10-30-default-mode-and-first-key-design.md`

## Global Constraints

- 注释、日志、commit message 一律简体中文;commit 结尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 不改 DLL 协议取 exe 名;协议只新增可选字段 `english`(回复与 `init` 请求),旧 DLL 不带即按程序默认。
- 初始列表(仅文件不存在时写入):`Weixin.exe`、`WeChat.exe`、`wps.exe`、`et.exe`、`wpp.exe`。
- 匹配不区分大小写(ASCII);exe 名为空 = 其他程序 = 默认英文。
- `onCommand`/`onMenu` 回复不得带 `commitString`/`compositionString`/候选(会崩宿主);组字中不切状态。
- 不 push、不打 tag、不发布;版本号已是 0.1.17(工作区未提交的 `Cargo.toml`/`Cargo.lock` 改动,本计划不提交它们)。
- 每个 Rust 任务完成时:`cargo test -p pyime-backend` 全绿、`cargo clippy -p pyime-backend --all-targets` 无新警告。
- 单次写文件 ≤ 约 150 行,长改动分多次 Edit。

## 文件结构

- 新建 `crates/pyime-backend/src/apps.rs`:`ChineseApps` 纯逻辑(解析/匹配/切换/导出)。
- 改 `crates/pyime-backend/src/ime.rs` + `ime/tests.rs`:`chinese-apps.txt` 读写与保护。
- 改 `crates/pyime-backend/src/session.rs` + `session/tests.rs`:会话 exe、初始状态、`init` 带回、回复附 `english`、菜单命令。
- 改 `crates/pyime-backend/src/tray.rs`:菜单项与命令 id。
- 改 `crates/pyime-backend/src/hub.rs`、`pipe.rs`、`win.rs`:`Msg::Opened` 传 exe 名。
- 改 `tsf/PIMETextService/PIMEClient.{h,cpp}`:记 `english`、`init` 带回、断管道重连重发、退避期日志。
- 新建 `tsf/PIMETextService/RpcRetry.h` + `tsf/tests/rpc_retry_test.cpp`;改 `scripts/build-tsf.sh` 跑该单测。
- 改 `CLAUDE.md`、`test-kit/tsf.txt`、设计文档(实现偏差说明)。

---

### Task 1: `ChineseApps` 纯逻辑

**Files:**
- Create: `crates/pyime-backend/src/apps.rs`
- Modify: `crates/pyime-backend/src/main.rs`(模块声明区,`mod hub;` 之前)

**Interfaces:**
- Produces:`pub struct ChineseApps`;`ChineseApps::defaults() -> ChineseApps`;`ChineseApps::parse(&str) -> ChineseApps`;
  `contains(&self, exe: &str) -> bool`;`toggle(&mut self, exe: &str) -> bool`(返回切换后是否在表);`export(&self) -> String`。

- [ ] **Step 1: 写失败的测试与空实现**

`crates/pyime-backend/src/apps.rs`:

```rust
//! 默认中文的程序列表 `chinese-apps.txt`:每行一个 exe 文件名,不区分大小写;不在表里的程序默认英文。

/// 文件不存在时写入的初始列表。
pub const DEFAULTS: [&str; 5] = ["Weixin.exe", "WeChat.exe", "wps.exe", "et.exe", "wpp.exe"];

#[derive(Clone, Debug, PartialEq)]
pub struct ChineseApps(Vec<String>);

impl ChineseApps {
    pub fn defaults() -> ChineseApps {
        todo!()
    }

    /// 解析文件内容:去 BOM,按行去首尾空白(含 CR),忽略空行。
    pub fn parse(_text: &str) -> ChineseApps {
        todo!()
    }

    /// `exe` 是否在表(不区分大小写);空名不在表。
    pub fn contains(&self, _exe: &str) -> bool {
        todo!()
    }

    /// 在表则移除(含各种大小写写法),不在则追加;返回切换后是否在表。
    pub fn toggle(&mut self, _exe: &str) -> bool {
        todo!()
    }

    /// 存盘文本:每行一个,LF 结尾。
    pub fn export(&self) -> String {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 初始列表含微信与wps三件套() {
        let a = ChineseApps::defaults();
        assert_eq!(a.export(), "Weixin.exe\nWeChat.exe\nwps.exe\net.exe\nwpp.exe\n");
    }

    #[test]
    fn 解析去bom_crlf_空白与空行() {
        let a = ChineseApps::parse("\u{feff}Weixin.exe\r\n\r\n  wps.exe  \n");
        assert_eq!(a.export(), "Weixin.exe\nwps.exe\n");
    }

    #[test]
    fn 匹配不区分大小写_空名不在表() {
        let a = ChineseApps::parse("Weixin.exe\n");
        assert!(a.contains("WEIXIN.EXE"));
        assert!(a.contains("weixin.exe"));
        assert!(!a.contains("notepad.exe"));
        assert!(!a.contains(""));
    }

    #[test]
    fn 切换加入与移除() {
        let mut a = ChineseApps::parse("Weixin.exe\n");
        assert!(a.toggle("Notepad.exe"));
        assert_eq!(a.export(), "Weixin.exe\nNotepad.exe\n");
        assert!(!a.toggle("weixin.exe"));
        assert_eq!(a.export(), "Notepad.exe\n");
    }

    #[test]
    fn 空文件即空列表() {
        assert_eq!(ChineseApps::parse("").export(), "");
    }
}
```

`main.rs` 在 `mod hub;` 那组声明前加一行 `mod apps;`。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend apps::`
Expected: 5 个用例 FAIL(panicked at 'not yet implemented')。

- [ ] **Step 3: 实现**

把各 `todo!()` 换成:

```rust
    pub fn defaults() -> ChineseApps {
        ChineseApps(DEFAULTS.iter().map(|s| s.to_string()).collect())
    }

    pub fn parse(text: &str) -> ChineseApps {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        ChineseApps(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
    }

    pub fn contains(&self, exe: &str) -> bool {
        !exe.is_empty() && self.0.iter().any(|a| a.eq_ignore_ascii_case(exe))
    }

    pub fn toggle(&mut self, exe: &str) -> bool {
        if self.contains(exe) {
            self.0.retain(|a| !a.eq_ignore_ascii_case(exe));
            false
        } else {
            self.0.push(exe.to_string());
            true
        }
    }

    pub fn export(&self) -> String {
        self.0.iter().map(|a| format!("{a}\n")).collect()
    }
```

(参数名去掉前导下划线。)

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-backend apps::`
Expected: 5 passed。此时 `ChineseApps` 尚未被使用,clippy/rustc 可能报 dead_code,Task 2 用上后消失,不要为此加 `allow`。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/apps.rs crates/pyime-backend/src/main.rs
git commit -m "默认中文程序列表:解析、匹配与切换"
```

### Task 2: `Ime` 读写 `chinese-apps.txt`

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs`(`use` 区、`Ime` 结构体、`fuzzy_path` 附近加辅助函数、`Ime::build`、`toggle_fuzzy` 之后加方法)
- Test: `crates/pyime-backend/src/ime/tests.rs`(文件末尾追加)

**Interfaces:**
- Consumes:Task 1 的 `ChineseApps`。
- Produces:`Ime::is_chinese_app(&self, exe: &str) -> bool`;`Ime::toggle_chinese_app(&self, exe: &str) -> bool`(返回切换后是否在表;空名不处理返回 false)。
  `Ime::with_engine` 用内存中的初始列表、不读写盘;`Ime::with_user_path(p)` 读写 `p` 同目录的 `chinese-apps.txt`。

- [ ] **Step 1: 写失败的测试**

追加到 `ime/tests.rs`:

```rust
fn apps_file(p: &Path) -> PathBuf {
    p.with_file_name("chinese-apps.txt")
}

#[test]
fn 默认中文列表_文件不存在时写入初始列表() {
    let p = temp_user_path("apps-init");
    let ime = Ime::with_user_path(engine(), p.clone());
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "Weixin.exe\nWeChat.exe\nwps.exe\net.exe\nwpp.exe\n");
    assert!(ime.is_chinese_app("weixin.exe"));
    assert!(!ime.is_chinese_app("notepad.exe"));
}

#[test]
fn 默认中文列表_空文件不重写() {
    let p = temp_user_path("apps-empty");
    std::fs::write(apps_file(&p), "").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "");
    assert!(!ime.is_chinese_app("Weixin.exe"));
}

#[test]
fn 默认中文列表_勾选加入与移除并写盘() {
    let p = temp_user_path("apps-toggle");
    std::fs::write(apps_file(&p), "Weixin.exe\r\n").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    assert!(ime.toggle_chinese_app("Notepad.exe"));
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "Weixin.exe\nNotepad.exe\n");
    assert!(ime.is_chinese_app("notepad.exe"));
    assert!(!ime.toggle_chinese_app("notepad.exe"));
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "Weixin.exe\n");
    assert!(!ime.toggle_chinese_app("")); // 空名不处理
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "Weixin.exe\n");
}

#[test]
fn 默认中文列表_非utf8按空列表且不覆盖() {
    let p = temp_user_path("apps-badutf8");
    std::fs::write(apps_file(&p), b"Weixin.exe\n\xc4\xe3").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    assert!(!ime.is_chinese_app("Weixin.exe"));
    assert!(ime.toggle_chinese_app("notepad.exe")); // 只改内存
    assert!(ime.is_chinese_app("notepad.exe"));
    assert_eq!(std::fs::read(apps_file(&p)).unwrap(), b"Weixin.exe\n\xc4\xe3");
}

#[test]
fn 默认中文列表_无数据目录时用内存初始列表() {
    let ime = Ime::with_engine(engine());
    assert!(ime.is_chinese_app("WPS.EXE"));
    assert!(ime.toggle_chinese_app("notepad.exe"));
    assert!(ime.is_chinese_app("notepad.exe"));
}
```

(`ime/tests.rs` 顶部是 `use super::*;`,`Path`/`PathBuf` 已由 `ime.rs` 引入。)

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend ime::tests::默认中文列表`
Expected: 编译失败,`no method named is_chinese_app`。

- [ ] **Step 3: 实现**

`ime.rs` 的 `use` 区加:

```rust
use crate::apps::ChineseApps;
```

`Ime` 结构体在 `generation` 字段之后加:

```rust
    /// 默认中文的程序列表(`chinese-apps.txt`);`apps_path` 为 None 时只在内存。
    apps: RefCell<ChineseApps>,
    apps_path: Option<PathBuf>,
    /// 启动时文件读不出(如非 UTF-8):勾选只改内存,不覆盖用户的文件。
    apps_writable: Cell<bool>,
```

在 `read_fuzzy` 之后加:

```rust
/// 默认中文程序列表 `chinese-apps.txt`:与 `user.txt` 同目录。
fn apps_path(user: Option<&Path>) -> Option<PathBuf> {
    user.map(|p| p.with_file_name("chinese-apps.txt"))
}

/// 读入默认中文程序列表,返回 (列表, 能否写回)。文件不存在时写入初始列表;
/// 读不出(非 UTF-8 等)按空列表处理且不写回;没有数据目录时用内存初始列表。
fn read_apps(path: Option<&Path>) -> (ChineseApps, bool) {
    let Some(p) = path else { return (ChineseApps::defaults(), true) };
    match std::fs::read(p) {
        Ok(b) => match String::from_utf8(b) {
            Ok(t) => (ChineseApps::parse(&t), true),
            Err(_) => {
                log::line(&format!("{p:?} 不是 UTF-8,按空列表处理且不写回"));
                (ChineseApps::parse(""), false)
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let apps = ChineseApps::defaults();
            write_apps(p, &apps);
            (apps, true)
        }
        Err(e) => {
            log::line(&format!("读取 {p:?} 失败,按空列表处理且不写回: {e}"));
            (ChineseApps::parse(""), false)
        }
    }
}

fn write_apps(p: &Path, apps: &ChineseApps) {
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = write_atomic(p, apps.export().as_bytes()) {
        log::line(&format!("写入 {p:?} 失败: {e}"));
    }
}
```

`Ime::build` 改为(签名不变,由 `user_path` 推出列表路径):

```rust
    fn build(engine: Engine, user_path: Option<PathBuf>, en_learned_path: Option<PathBuf>, user_stamp: Option<(SystemTime, u64)>) -> Rc<Ime> {
        let apps_path = apps_path(user_path.as_deref());
        let (apps, apps_writable) = read_apps(apps_path.as_deref());
        Rc::new(Ime {
            engine: RefCell::new(engine),
            user_path,
            en_learned_path,
            user_stamp: Cell::new(user_stamp),
            skip_logged: Cell::new(false),
            generation: Cell::new(0),
            apps: RefCell::new(apps),
            apps_path,
            apps_writable: Cell::new(apps_writable),
            #[cfg(test)]
            pressed: RefCell::new(Vec::new()),
            #[cfg(test)]
            launched: RefCell::new(Vec::new()),
        })
    }
```

在 `toggle_fuzzy` 之后加:

```rust
    /// 程序 `exe` 是否默认中文。
    pub fn is_chinese_app(&self, exe: &str) -> bool {
        self.apps.borrow().contains(exe)
    }

    /// 菜单"本程序默认中文":切换 `exe` 是否在表并写 `chinese-apps.txt`;返回切换后是否在表。空名不处理。
    pub fn toggle_chinese_app(&self, exe: &str) -> bool {
        if exe.is_empty() {
            return false;
        }
        let on = self.apps.borrow_mut().toggle(exe);
        log::line(&format!("菜单:本程序默认中文 {exe} {}", if on { "加入" } else { "移除" }));
        match &self.apps_path {
            Some(p) if self.apps_writable.get() => write_apps(p, &self.apps.borrow()),
            Some(p) => log::line(&format!("{p:?} 启动时读不出,只改内存不写回")),
            None => {}
        }
        on
    }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-backend`
Expected: 全部 passed(含新增 5 个)。再跑 `cargo clippy -p pyime-backend --all-targets`,无新警告。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/ime.rs crates/pyime-backend/src/ime/tests.rs
git commit -m "默认中文程序列表:chinese-apps.txt 读写,初始列表与非 UTF-8 保护"
```

### Task 3: 会话按程序定初始中/英,`init` 带回,回复附 `english`

**Files:**
- Modify: `crates/pyime-backend/src/session.rs`(`Session` 结构体末尾字段、`impl Session` 的 `new` 之后、`handle` 里 `"init"`/`"onActivate"` 分支、`toggle_english`)
- Test: `crates/pyime-backend/src/session/tests.rs`(文件末尾追加)

**Interfaces:**
- Consumes:Task 2 的 `Ime::is_chinese_app`。
- Produces:`Session::open(ime: Rc<Ime>, exe: &str) -> Session`(生产路径用);`Session::new(ime)` 保持原样(exe 为空、中文,供既有测试);
  字段 `exe: String`(Task 4 用)。回复字段:`onActivate`(win8)与切换时的回复带 `"english": bool`;`init` 请求可选 `"english": bool`。

- [ ] **Step 1: 写失败的测试**

追加到 `session/tests.rs`:

```rust
#[test]
fn 新会话按程序决定默认中英() {
    let mut s = Session::open(Ime::with_engine(engine()), "WeChat.exe");
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
    let mut s = Session::open(Ime::with_engine(engine()), "notepad.exe");
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], false);
    let mut s = Session::open(Ime::with_engine(engine()), "");
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], false);
}

#[test]
fn init带回的中英状态覆盖程序默认() {
    let mut s = Session::open(Ime::with_engine(engine()), "notepad.exe");
    s.handle(&json!({"method": "init", "seqNum": 1, "isWindows8Above": true, "english": false}));
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
    let mut s = Session::open(Ime::with_engine(engine()), "Weixin.exe");
    s.handle(&json!({"method": "init", "seqNum": 1, "isWindows8Above": true, "english": true}));
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], false);
}

#[test]
fn 激活与切换的回复带中英状态() {
    let mut s = win8_session();
    let r = s.handle(&json!({"method": "onActivate", "seqNum": 2, "isKeyboardOpen": true}));
    assert_eq!(r["english"], false);
    assert_eq!(shift_click(&mut s)["english"], true);
    assert_eq!(left_click(&mut s)["english"], false);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend session::tests::`
Expected: 编译失败,`no function or associated item named open`。

- [ ] **Step 3: 实现**

`Session` 结构体末尾(`win8` 之后)加字段:

```rust
    /// 宿主程序的 exe 文件名(取不到为空),托盘"本程序默认中文"用。
    exe: String,
```

`Session::new` 的结构体字面量末尾加 `exe: String::new()`(`win8: false` 之后)。在 `new` 之后加:

```rust
    /// 新连接的会话:宿主 `exe` 在 `chinese-apps.txt` 里则默认中文,否则默认英文。
    pub fn open(ime: Rc<Ime>, exe: &str) -> Session {
        let english = !ime.is_chinese_app(exe);
        Session { exe: exe.to_string(), english, ..Session::new(ime) }
    }
```

`handle` 里把

```rust
            "init" => self.win8 = req["isWindows8Above"] == true,
            "onActivate" if self.win8 => resp["addButton"] = json!([tray::mode_button(self.english)]),
```

改为

```rust
            "init" => {
                self.win8 = req["isWindows8Above"] == true;
                // worker 重启后重连:DLL 带回断线前的中/英状态
                if let Some(english) = req["english"].as_bool() {
                    self.english = english;
                }
            }
            "onActivate" if self.win8 => {
                resp["addButton"] = json!([tray::mode_button(self.english)]);
                resp["english"] = json!(self.english);
            }
```

`toggle_english` 里 `if self.win8 { … }` 块改为:

```rust
        if self.win8 {
            resp["changeButton"] = json!([tray::mode_change(self.english)]);
            // DLL 记住它,重连时经 init 带回
            resp["english"] = json!(self.english);
        }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-backend`
Expected: 全部 passed。`cargo clippy -p pyime-backend --all-targets` 无新警告(`open` 此时只被测试用,非测试构建可能报 dead_code;Task 5 接入 hub 后消失,不要加 `allow`)。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "会话按宿主程序定默认中英,init 可带回断线前状态"
```

### Task 4: 托盘「本程序默认中文」

**Files:**
- Modify: `crates/pyime-backend/src/tray.rs`(常量区、`menu`)
- Modify: `crates/pyime-backend/src/session.rs`(`handle` 的 `"onMenu"` 分支、`on_command`)
- Test: `crates/pyime-backend/src/session/tests.rs`(改 `右键菜单含版本与三项命令`、`菜单命令分发到编辑_重载与打开目录` 的下标;末尾追加新用例)

**Interfaces:**
- Consumes:Task 2 `Ime::toggle_chinese_app`/`is_chinese_app`,Task 3 `Session::open`、字段 `exe`、`toggle_english` 回复带 `english`。
- Produces:`tray::CMD_CHINESE_APP: u64 = 14`;`tray::menu(user_changed: bool, fuzzy: &FuzzyConfig, exe: &str, chinese_app: bool) -> Value`。
  菜单顺序:版本 / 分隔 / 模糊音 ▸ / 本程序默认中文 / 分隔 / 编辑用户词库… / 重新加载用户词库 / 分隔 / 打开数据目录。

- [ ] **Step 1: 写失败的测试**

改既有用例(菜单多了一项,属预期变化,断言不放松):

- `右键菜单含版本与三项命令`:`assert_eq!(texts[1..], [...])` 改为
  `assert_eq!(texts[1..], ["模糊音", "本程序默认中文", "编辑用户词库…", "重新加载用户词库", "打开数据目录"]);`
- `菜单命令分发到编辑_重载与打开目录`:`r["return"][5]["text"]` 改为 `r["return"][6]["text"]`。

追加:

```rust
fn app_session(p: &std::path::Path, exe: &str) -> Session {
    let mut s = Session::open(Ime::with_user_path(engine(), p.to_path_buf()), exe);
    s.handle(&json!({"method": "init", "seqNum": 1, "isWindows8Above": true}));
    s
}

fn app_item(s: &mut Session) -> Value {
    let r = s.handle(&json!({"method": "onMenu", "seqNum": 5, "id": "windows-mode-icon"}));
    r["return"].as_array().unwrap().iter().find(|i| i["id"] == tray::CMD_CHINESE_APP).unwrap().clone()
}

#[test]
fn 本程序默认中文_菜单项显示程序名与勾选() {
    let p = crate::ime::temp_user_path("app-item");
    let mut s = app_session(&p, "notepad.exe");
    let item = app_item(&mut s);
    assert_eq!(item["text"], "本程序默认中文(notepad.exe)");
    assert_eq!(item["checked"], false);
    let mut s = app_session(&p, "Weixin.exe");
    assert_eq!(app_item(&mut s)["checked"], true);
}

#[test]
fn 本程序默认中文_勾选写盘并立即切换() {
    let p = crate::ime::temp_user_path("app-toggle");
    let f = p.with_file_name("chinese-apps.txt");
    let mut s = app_session(&p, "notepad.exe");
    let r = menu_cmd(&mut s, tray::CMD_CHINESE_APP);
    assert_eq!(r["return"], true);
    assert_eq!(r["english"], false);
    assert!(icon_of(&r["changeButton"]).ends_with("chi_full_capsoff.ico"));
    assert!(std::fs::read_to_string(&f).unwrap().contains("notepad.exe"));
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
    let r = menu_cmd(&mut s, tray::CMD_CHINESE_APP);
    assert_eq!(r["english"], true);
    assert!(!std::fs::read_to_string(&f).unwrap().contains("notepad.exe"));
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], false);
}

#[test]
fn 本程序默认中文_已是对应状态时不重复切换() {
    let p = crate::ime::temp_user_path("app-same");
    let mut s = app_session(&p, "notepad.exe");
    shift_click(&mut s); // 手动切到中文
    let r = menu_cmd(&mut s, tray::CMD_CHINESE_APP);
    assert_eq!(r["return"], true);
    assert!(r.get("changeButton").is_none());
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
}

#[test]
fn 本程序默认中文_组字中只改列表不切状态() {
    let p = crate::ime::temp_user_path("app-composing");
    let f = p.with_file_name("chinese-apps.txt");
    let mut s = app_session(&p, "Weixin.exe");
    press(&mut s, &[N, I]);
    let r = menu_cmd(&mut s, tray::CMD_CHINESE_APP);
    assert_eq!(r["return"], true);
    assert!(r.get("changeButton").is_none() && r.get("commitString").is_none());
    assert!(!std::fs::read_to_string(&f).unwrap().contains("Weixin.exe"));
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
}

#[test]
fn 本程序默认中文_取不到程序名时置灰且不处理() {
    let mut s = win8_session();
    let item = app_item(&mut s);
    assert_eq!(item["text"], "本程序默认中文");
    assert_eq!(item["enabled"], false);
    assert_eq!(menu_cmd(&mut s, tray::CMD_CHINESE_APP)["return"], false);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend session::tests::`
Expected: 编译失败,`cannot find value CMD_CHINESE_APP in module tray`。

- [ ] **Step 3: 实现**

`tray.rs` 在 `CMD_OPEN_DIR` 之后加:

```rust
/// 菜单"本程序默认中文"。
pub const CMD_CHINESE_APP: u64 = 14;
```

`menu` 改为:

```rust
/// 右键菜单;`user_changed` 为真(user.txt 被外部修改、尚未重载)时重载项带提示;`fuzzy` 决定模糊音子菜单的勾选;
/// `exe` 为宿主程序名(空则"本程序默认中文"置灰),`chinese_app` 为它是否在默认中文列表里。
pub fn menu(user_changed: bool, fuzzy: &FuzzyConfig, exe: &str, chinese_app: bool) -> Value {
    let reload = if user_changed { "重新加载用户词库(文件已修改)" } else { "重新加载用户词库" };
    let rules: Vec<Value> = FuzzyConfig::rule_names()
        .into_iter()
        .enumerate()
        .map(|(i, n)| json!({ "text": n, "id": CMD_FUZZY + i as u64, "checked": fuzzy.is_on(i) }))
        .collect();
    let app = if exe.is_empty() {
        json!({ "text": "本程序默认中文", "id": CMD_CHINESE_APP, "enabled": false })
    } else {
        json!({ "text": format!("本程序默认中文({exe})"), "id": CMD_CHINESE_APP, "checked": chinese_app })
    };
    json!([
        { "text": format!("PYIME {}", env!("CARGO_PKG_VERSION")), "id": CMD_VERSION, "enabled": false },
        {},
        { "text": "模糊音", "submenu": rules },
        app,
        {},
        { "text": "编辑用户词库…", "id": CMD_EDIT_USER },
        { "text": reload, "id": CMD_RELOAD_USER },
        {},
        { "text": "打开数据目录", "id": CMD_OPEN_DIR },
    ])
}
```

`session.rs` 的 `"onMenu"` 分支改为:

```rust
            "onMenu" if req["id"] == tray::MODE_ICON => {
                resp["return"] = tray::menu(self.ime.user_file_changed(), &self.ime.fuzzy(), &self.exe, self.ime.is_chinese_app(&self.exe))
            }
```

`on_command` 的 `match` 里,在 `(tray::CMD_OPEN_DIR, tray::COMMAND_MENU)` 分支之前加:

```rust
            (tray::CMD_CHINESE_APP, tray::COMMAND_MENU) if self.exe.is_empty() => false,
            (tray::CMD_CHINESE_APP, tray::COMMAND_MENU) => {
                let chinese = self.ime.toggle_chinese_app(&self.exe);
                // 当前会话随即切到新的默认;组字中只改列表(onCommand 回复无编辑会话,不能上屏)
                if !self.composing() && self.english == chinese {
                    self.toggle_english(resp);
                }
                true
            }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-backend`
Expected: 全部 passed。`cargo clippy -p pyime-backend --all-targets` 无新警告。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/tray.rs crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "托盘菜单:本程序默认中文,勾选写 chinese-apps.txt 并即时切换"
```

### Task 5: 管道对端 exe 名经 hub 交给会话

**Files:**
- Modify: `crates/pyime-backend/src/win.rs`(`object_sddl` 之后加纯函数 `exe_name`;`sys` 模块加声明与 `client_exe`;`tests` 加用例)
- Modify: `crates/pyime-backend/src/hub.rs`(`Msg`、`Hub`、`request`、`close`、`run`、tests)
- Modify: `crates/pyime-backend/src/pipe.rs`(`serve` 开头)

**Interfaces:**
- Consumes:Task 3 `Session::open`;Task 2 `Ime::is_chinese_app`。
- Produces:`win::exe_name(path: &str) -> Option<String>`;`win::sys::client_exe(pipe: Handle) -> Option<String>`(仅 Windows);
  `Msg::Opened { conn: u64, exe: String }`;`Hub::open(&mut self, conn: u64, exe: String)`。

- [ ] **Step 1: 写失败的测试**

`win.rs` 的 `tests` 加:

```rust
    #[test]
    fn 从全路径取exe文件名() {
        assert_eq!(exe_name(r"C:\Program Files\Tencent\Weixin\Weixin.exe").as_deref(), Some("Weixin.exe"));
        assert_eq!(exe_name("notepad.exe").as_deref(), Some("notepad.exe"));
        assert_eq!(exe_name(""), None);
        assert_eq!(exe_name(r"C:\dir\"), None);
    }
```

`hub.rs` 的 `tests`:把三处 `let mut hub = Hub::new(Ime::with_engine(engine()));` 后面紧跟的请求之前加 `hub.open(1, "Weixin.exe".into());`
(`不同连接各自独立会话` 里再加 `hub.open(2, "Weixin.exe".into());`);
`断开即结束会话_同编号重连从空开始` 在 `hub.close(1);` 之后、再次 `request` 之前加 `hub.open(1, "Weixin.exe".into());`
(真实连接号不复用,每个连接都先有 `Opened`)。再追加:

```rust
    #[test]
    fn 按连接的程序名定默认中英_断开即忘记() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        hub.open(1, "notepad.exe".into());
        assert_eq!(parse(&hub.request(1, &req(1, N)).unwrap())["return"], false);
        hub.open(2, "WPS.exe".into());
        assert_eq!(parse(&hub.request(2, &req(1, N)).unwrap())["compositionString"], "n");
        hub.close(2);
        assert_eq!(parse(&hub.request(2, &req(2, N)).unwrap())["return"], false); // 没有 Opened:按其他程序
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pyime-backend -- hub:: win::`
Expected: 编译失败,`no method named open`、`cannot find function exe_name`。

- [ ] **Step 3: 实现**

`win.rs` 在 `object_sddl` 之后加:

```rust
/// 进程映像全路径里的 exe 文件名(`C:\…\Weixin.exe` → `Weixin.exe`);路径为空或以分隔符结尾时为 None。
pub fn exe_name(path: &str) -> Option<String> {
    path.rsplit(['\\', '/']).next().filter(|s| !s.is_empty()).map(String::from)
}
```

`sys` 模块的 kernel32 `extern "system"` 块末尾加:

```rust
        pub fn GetNamedPipeClientProcessId(pipe: Handle, pid: *mut u32) -> i32;
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        pub fn QueryFullProcessImageNameW(process: Handle, flags: u32, name: *mut u16, size: *mut u32) -> i32;
```

`sys` 模块里 `write_message` 之后加:

```rust
    /// 管道对端进程的 exe 文件名(如 `Weixin.exe`);对端已退出、受保护进程等取不到时为 None。
    pub fn client_exe(pipe: Handle) -> Option<String> {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        let mut pid = 0u32;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: pid/buf/len 为本地变量,长度如实传入;进程句柄用完即关。OpenProcess 失败返回 0(NULL)。
        let ok = unsafe {
            if GetNamedPipeClientProcessId(pipe, &mut pid) == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process == 0 {
                return None;
            }
            let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(process);
            ok
        };
        if ok == 0 {
            return None;
        }
        super::exe_name(&String::from_utf16_lossy(&buf[..len as usize]))
    }
```

`hub.rs`:

1. `Msg` 加变体(放在 `Request` 之前):

```rust
    /// 新连接建立,`exe` 为对端宿主程序名(取不到为空);先于该连接的任何请求到达。
    Opened { conn: u64, exe: String },
```

2. `Hub` 加字段 `exes: HashMap<u64, String>,`(注释:`/// 各连接的宿主程序名,会话创建时据此定默认中/英。`),`Hub::new` 里初始化 `exes: HashMap::new()`。

3. `impl Hub` 加:

```rust
    fn open(&mut self, conn: u64, exe: String) {
        self.exes.insert(conn, exe);
    }
```

4. `request` 里把

```rust
        let ime = &self.ime;
        let sess = self.sessions.entry(conn).or_insert_with(|| Session::new(ime.clone()));
```

改为

```rust
        let (ime, exes) = (&self.ime, &self.exes);
        let sess = self.sessions.entry(conn).or_insert_with(|| {
            let exe = exes.get(&conn).map_or("", String::as_str);
            log::line(&format!("会话 [{conn}] exe={exe} 默认{}", if ime.is_chinese_app(exe) { "中文" } else { "英文" }));
            Session::open(ime.clone(), exe)
        });
```

5. `close` 开头加 `self.exes.remove(&conn);`。

6. `run` 的 `match` 加分支 `Msg::Opened { conn, exe } => hub.open(conn, exe),`。

`pipe.rs` 的 `serve`:把开头的 `while let Some(body) = read_message(h) {` 一行替换为下面三行(循环体不变)。
发送失败时不进循环,直接走到循环后的 `Closed` 与关句柄,不提前 `return`(否则句柄泄漏)。
`client_exe` 经 `use crate::win::sys::*;` 已在作用域内。

```rust
    // 宿主程序名决定新会话默认中/英;取不到按其他程序(默认英文)
    let opened = tx.send(Msg::Opened { conn, exe: client_exe(h).unwrap_or_default() }).is_ok();
    while let Some(body) = opened.then(|| read_message(h)).flatten() {
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pyime-backend`
Expected: 全部 passed。

Run: `cargo clippy -p pyime-backend --all-targets`
Expected: 无新警告(`Session::open` 已被 hub 用上)。

Run: `cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend`
Expected: `Finished`,无 error(验证 Windows 专用的 `pipe.rs`/`client_exe` 能编译)。

Run: `cargo clippy --target x86_64-pc-windows-gnu -p pyime-backend`
Expected: 无新警告。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/win.rs crates/pyime-backend/src/hub.rs crates/pyime-backend/src/pipe.rs
git commit -m "按管道对端进程的 exe 名定新会话默认中英"
```

### Task 6: DLL 记住中/英状态,重连 `init` 带回

**Files:**
- Modify: `tsf/PIMETextService/PIMEClient.h`(`private:` 成员区,`shouldWaitConnection_` 之后)
- Modify: `tsf/PIMETextService/PIMEClient.cpp`(`Client::handleRpcResponse`、`Client::init`)

**Interfaces:**
- Consumes:Task 3 的回复字段 `english`、`init` 请求字段 `english`。
- Produces:成员 `bool hasEnglish_`、`bool english_`(Task 7 不直接用,重连走 `init` 自动带上)。

- [ ] **Step 1: 实现**(DLL 无单测框架,靠编译 + 实机验证)

`PIMEClient.h` 在 `bool shouldWaitConnection_;` 之后加:

```cpp
    bool hasEnglish_ = false;  // PYIME:服务端告知过中/英状态(回复里的 english)
    bool english_ = false;     // PYIME:最近一次的中/英状态,worker 重启重连时经 init 带回
```

`PIMEClient.cpp` 的 `Client::handleRpcResponse` 改为:

```cpp
bool Client::handleRpcResponse(Json::Value& msg, Ime::EditSession* session) {
	bool success = msg.get("success", false).asBool();
	if (success) {
		// PYIME:记下服务端告知的中/英状态,worker 重启重连后经 init 带回
		if (msg["english"].isBool()) {
			english_ = msg["english"].asBool();
			hasEnglish_ = true;
		}
		updateStatus(msg, session);
	}
	return success;
}
```

`Client::init` 在 `req["isConsole"] = …;` 之后加:

```cpp
	if (hasEnglish_)
		req["english"] = english_;  // PYIME:重连时带回断线前的中/英状态
```

- [ ] **Step 2: 编译**

Run: `scripts/build-tsf.sh`
Expected: backoff 单测通过,x64/x86 两个 `PYIME.dll` 生成,无"依赖了 mingw 运行库""缺导出"报错,无新增 warning。

- [ ] **Step 3: Commit**

```bash
git add tsf/PIMETextService/PIMEClient.h tsf/PIMETextService/PIMEClient.cpp
git commit -m "TSF:记住服务端告知的中英状态,重连 init 时带回"
```

### Task 7: 断管道立即重连重发 + 退避期日志 + 随版发布首键诊断

**Files:**
- Create: `tsf/PIMETextService/RpcRetry.h`
- Create: `tsf/tests/rpc_retry_test.cpp`
- Modify: `scripts/build-tsf.sh`(纯逻辑单测段)
- Modify: `tsf/PIMETextService/PIMEClient.h`(私有方法声明,`void onRpcFailure(...)` 之后)
- Modify: `tsf/PIMETextService/PIMEClient.cpp`(include 区、退避辅助函数、`callRpcMethod`、新增 `resendAfterReconnect`、`waitForRpcConnection`)
- 一并提交工作区已有的首键诊断改动:`tsf/PIMETextService/PIMETextService.{h,cpp}`、`tsf/PIMETextService/TsfLog.h`、`tsf/libIME2/src/TextService.{h,cpp}`

**Interfaces:**
- Produces:`PIME::shouldRetryRpc(const char* method, uint32_t err, bool composing) -> bool`(`RpcRetry.h`);
  `bool Client::resendAfterReconnect(Json::Value& request, std::string& serializedResponse, unsigned int& seqNum, ULONGLONG deadline)`。

- [ ] **Step 1: 写失败的单测**

`tsf/tests/rpc_retry_test.cpp`:

```cpp
// PIME::shouldRetryRpc 的本机单测(Linux g++ 编译运行,由 scripts/build-tsf.sh 调用)。
#include "RpcRetry.h"
#include <cstdio>
#include <initializer_list>

static int failures = 0;
#define CHECK(cond) do { if (!(cond)) { std::printf("失败 %s:%d: %s\n", __FILE__, __LINE__, #cond); ++failures; } } while (0)

int main() {
    using PIME::shouldRetryRpc;
    // 按键类请求 + 管道已断 + 无组字:重发
    for (const char* m : {"filterKeyDown", "onKeyDown", "filterKeyUp", "onKeyUp"}) {
        CHECK(shouldRetryRpc(m, 109, false));  // ERROR_BROKEN_PIPE
        CHECK(shouldRetryRpc(m, 232, false));  // ERROR_NO_DATA
        CHECK(shouldRetryRpc(m, 233, false));  // ERROR_PIPE_NOT_CONNECTED
    }
    // 超时(服务端卡住)、其他错误不重发
    CHECK(!shouldRetryRpc("filterKeyDown", 258, false));  // WAIT_TIMEOUT
    CHECK(!shouldRetryRpc("filterKeyDown", 0, false));
    CHECK(!shouldRetryRpc("filterKeyDown", 6, false));    // ERROR_INVALID_HANDLE
    // 有组字不重发
    CHECK(!shouldRetryRpc("onKeyDown", 109, true));
    // 非按键请求不重发
    for (const char* m : {"init", "onActivate", "onDeactivate", "onCommand", "onMenu", "onCompositionTerminated"})
        CHECK(!shouldRetryRpc(m, 109, false));
    if (failures == 0) std::printf("rpc_retry_test: 全部通过\n");
    return failures == 0 ? 0 : 1;
}
```

`scripts/build-tsf.sh` 在 `"$SHIM/backoff_test"` 之后加:

```bash
g++ -std=c++17 -Wall -I"$SRC/PIMETextService" "$SRC/tests/rpc_retry_test.cpp" -o "$SHIM/rpc_retry_test"
"$SHIM/rpc_retry_test"
```

- [ ] **Step 2: 运行确认失败**

Run: `scripts/build-tsf.sh`
Expected: 编译 `rpc_retry_test.cpp` 失败,`RpcRetry.h: No such file or directory`。

- [ ] **Step 3: 写 `RpcRetry.h`**

```cpp
// PYIME:请求失败后是否立即重连并重发一次(纯逻辑,无 Windows 依赖,可在 Linux 上单测)。
// 只对按键类请求、"管道已断"类错误、且无组字时重发:超时说明服务端卡住,重发会让宿主 UI 线程再冻一轮;
// 有组字时新会话里没有这段拼音,与 DLL 的组字状态对不上;非按键请求(如宿主退出时的 onDeactivate)不值得重连。
#ifndef PYIME_RPC_RETRY_H
#define PYIME_RPC_RETRY_H

#include <cstdint>
#include <cstring>
#include <initializer_list>

namespace PIME {

// Win32 错误码(winerror.h),写成数值以便脱离 Windows 头文件单测
constexpr uint32_t kErrorBrokenPipe = 109;        // ERROR_BROKEN_PIPE
constexpr uint32_t kErrorNoData = 232;            // ERROR_NO_DATA
constexpr uint32_t kErrorPipeNotConnected = 233;  // ERROR_PIPE_NOT_CONNECTED

inline bool isPipeGone(uint32_t err) {
    return err == kErrorBrokenPipe || err == kErrorNoData || err == kErrorPipeNotConnected;
}

inline bool isKeyMethod(const char* method) {
    for (const char* m : {"filterKeyDown", "onKeyDown", "filterKeyUp", "onKeyUp"})
        if (std::strcmp(method, m) == 0)
            return true;
    return false;
}

inline bool shouldRetryRpc(const char* method, uint32_t err, bool composing) {
    return !composing && isKeyMethod(method) && isPipeGone(err);
}

}

#endif
```

- [ ] **Step 4: 运行单测确认通过**

Run: `scripts/build-tsf.sh`
Expected: 输出 `rpc_retry_test: 全部通过`,随后两个 DLL 照常生成(此时 `PIMEClient.cpp` 尚未用到该头文件)。

- [ ] **Step 5: 接入 `callRpcMethod`**

`PIMEClient.cpp` include 区(`#include "TsfLog.h"` 附近)加 `#include "RpcRetry.h"`。

`PIMEClient.h` 在 `void onRpcFailure(const char* method, const char* what);` 之后加:

```cpp
    bool resendAfterReconnect(Json::Value& request, std::string& serializedResponse, unsigned int& seqNum, ULONGLONG deadline);
```

`callRpcMethod` 中从 `auto seqNum = nextSeqNum_++;` 到第一个 `onRpcFailure(... "超时" : "读写失败")` 的 `return false; }` 为止,替换为:

```cpp
    // Add a sequence number for the request.
    unsigned int seqNum = nextSeqNum_++;
    request["seqNum"] = seqNum;

	Json::FastWriter writer;
	std::string serializedRequest = writer.write(request); // convert the json object to string
    std::string method = request["method"].asString();

    std::string serializedResponse;
    const ULONGLONG deadline = ::GetTickCount64() + kRpcTimeoutMs;
    bool ok = callRpcPipe(serializedRequest, serializedResponse, kRpcTimeoutMs);
    if (!ok && shouldRetryRpc(method.c_str(), lastIoError_, textService_->isComposing())) {
        ok = resendAfterReconnect(request, serializedResponse, seqNum, deadline);
        if (!ok && pipe_ == INVALID_HANDLE_VALUE)
            return false;  // PYIME:重连失败,已记日志;连接失败已由 waitForRpcConnection 计入退避
    }
    if (!ok) {
        onRpcFailure(method.c_str(), lastIoError_ == WAIT_TIMEOUT ? "超时" : "读写失败");
        return false;
    }
```

(其后的 `Json::Reader` 解析与 `seqNum` 比对保持不变,比对的是重发后的新 `seqNum`。)

在 `callRpcMethod` 之后加:

```cpp
// PYIME:管道已断(worker 被重启或崩溃)时立即重连并重发一次,救回本次按键,这一次断开不计入退避。
// 先清掉旧连接的按钮:否则重连后 onActivate 回复的 addButton 会在 TSF 里留下重复的模式图标,
// 之后 changeButton 更新的还是旧按钮。init/onActivate 各自受 kRpcTimeoutMs 约束;重发用本次请求剩余的时间。
bool Client::resendAfterReconnect(Json::Value& request, std::string& serializedResponse, unsigned int& seqNum, ULONGLONG deadline) {
    const std::string method = request["method"].asString();
    const DWORD err = lastIoError_;
    closeRpcConnection();
    resetTextServiceState();
    if (!waitForRpcConnection()) {
        closeRpcConnection();  // init/onActivate 握手中途失败时可能残留半开连接
        tsfLog("%s 管道已断(err=%lu),重连失败,放行", method.c_str(), (unsigned long)err);
        return false;
    }
    seqNum = nextSeqNum_++;
    request["seqNum"] = seqNum;
    Json::FastWriter writer;
    serializedResponse.clear();
    const ULONGLONG now = ::GetTickCount64();
    if (!callRpcPipe(writer.write(request), serializedResponse, now >= deadline ? 0 : DWORD(deadline - now)))
        return false;  // 调用方 onRpcFailure:断线、退避、放行
    tsfLog("%s 管道已断(err=%lu),已重连重发", method.c_str(), (unsigned long)err);
    return true;
}
```

- [ ] **Step 6: 退避期内放行的日志(每个退避周期每进程报一次)**

`PIMEClient.cpp` 的 `static SRWLOCK g_backoffLock = SRWLOCK_INIT;` 之后加:

```cpp
// PYIME:退避期内放行已报过(每个退避周期每进程只报一次,与退避状态同锁)
static bool g_blockedReported = false;

static bool takeBlockedReport() {
    ::AcquireSRWLockExclusive(&g_backoffLock);
    bool first = !g_blockedReported;
    g_blockedReported = true;
    ::ReleaseSRWLockExclusive(&g_backoffLock);
    return first;
}
```

`backoffFail` 里 `DWORD delay = g_backoff.fail(::GetTickCount64());` 之后加 `g_blockedReported = false;  // 新的退避周期`。

`waitForRpcConnection` 里

```cpp
    if (backoffBlocked()) {
        return false;
    }
```

改为

```cpp
    if (backoffBlocked()) {
        if (takeBlockedReport())
            tsfLog("退避期内不连接,请求未发送,按键放行");
        return false;
    }
```

- [ ] **Step 7: 编译**

Run: `scripts/build-tsf.sh`
Expected: `rpc_retry_test: 全部通过`;x64/x86 DLL 生成,无新增 warning,无运行库依赖/缺导出报错。

- [ ] **Step 8: Commit(连同工作区已有的首键诊断改动)**

先 `git diff tsf/PIMETextService/PIMETextService.cpp tsf/PIMETextService/PIMETextService.h tsf/PIMETextService/TsfLog.h tsf/libIME2/src/TextService.cpp tsf/libIME2/src/TextService.h`
确认只有 `reportPassThrough`/`onKeyPassedThrough` 诊断相关改动,再:

```bash
git add tsf/PIMETextService/RpcRetry.h tsf/tests/rpc_retry_test.cpp scripts/build-tsf.sh \
  tsf/PIMETextService/PIMEClient.h tsf/PIMETextService/PIMEClient.cpp \
  tsf/PIMETextService/PIMETextService.cpp tsf/PIMETextService/PIMETextService.h tsf/PIMETextService/TsfLog.h \
  tsf/libIME2/src/TextService.cpp tsf/libIME2/src/TextService.h
git commit -m "TSF:断管道立即重连重发救回首键;补退避期放行日志与字母键放行诊断"
```

### Task 8: 文档与实机清单

**Files:**
- Modify: `CLAUDE.md`(Project Layout 的用户数据文件列表、"托盘图标与菜单"、"待办")
- Modify: `test-kit/tsf.txt`(末尾追加一节)
- Modify: `docs/superpowers/specs/2026-10-30-default-mode-and-first-key-design.md`(2.1 实现偏差)

- [ ] **Step 1: 改 `CLAUDE.md`**

1. Project Layout 里 "模糊音开关 `fuzzy.txt`(…)同目录" 之后补:
   `、默认中文程序列表 `chinese-apps.txt`(每行一个 exe 名,不分大小写;不在表的程序默认英文;文件不存在时写入 Weixin.exe/WeChat.exe/wps.exe/et.exe/wpp.exe,想全英文请留空文件)`。
2. "托盘图标与菜单" 的右键菜单列表在 `模糊音 ▸(…)` 之后加 `/本程序默认中文(<exe>)(勾选即写 chinese-apps.txt 并切当前会话;组字中只改列表;取不到 exe 置灰;0.1.17 加,待实机验证)`。
3. "待办" 加一条:
   `- 默认英文 + 按程序默认中文、断管道重连重发(2026-10-30,设计 `docs/superpowers/specs/2026-10-30-default-mode-and-first-key-design.md`):待按 `test-kit/tsf.txt` 第十五至十八项实机验证;首键丢失的 TSF 上下文路径待用户贴回 `tsf.log`("字母键未交给输入法""退避期内…")后对症修。会话中/英状态经回复 `english` 由 DLL 记住、重连 `init` 带回。`

- [ ] **Step 2: 追加实机清单**

`test-kit/tsf.txt` 末尾追加:

```text

八、默认中/英与首键(0.1.17)
15. 新开记事本、浏览器地址栏:默认英文(托盘"英");新开微信聊天框、WPS 文字:默认中文(托盘"中")。
16. 记事本里右键托盘图标,勾"本程序默认中文(notepad.exe)":立即变中文;%LOCALAPPDATA%\PYIME\chinese-apps.txt 多出 notepad.exe;关掉记事本再开,默认中文。再取消勾选:立即变英文,文件里的 notepad.exe 消失。
17. 记事本(默认英文)单击 Shift 切中文;任务管理器结束内存较大的 pyime-server.exe,等 5 秒;回记事本打 dui:候选正常出现(d 没丢),仍是中文,托盘只有一个图标且显示"中";%LOCALAPPDATA%\PYIME\tsf.log 有"已重连重发"。
18. 平常使用中再遇到首字母丢失:记下大概时间,把 tsf.log 与 log.txt 对应时段贴回。
```

- [ ] **Step 3: 设计文档记实现偏差**

在设计文档 2.1 的"耗时预算"一条改为:
`- 耗时预算:重发使用首次请求剩余的时间(与首次请求共用 kRpcTimeoutMs=2000 ms 的截止时间);重连中的 init/onActivate 握手各自受 kRpcTimeoutMs 约束(与原有断线重连一致),连接受 kConnectTimeoutMs 约束。`
并把"(0 字节成功读也按断开处理,但不是主判据)"删去,在其后注明:`0 字节读不单独判断:回复为空会走"回复无法解析"照旧 onRpcFailure。`

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md test-kit/tsf.txt docs/superpowers/specs/2026-10-30-default-mode-and-first-key-design.md docs/superpowers/plans/2026-10-30-default-mode-and-first-key.md
git commit -m "文档:默认中英与首键重连的说明、实机清单与实现偏差"
```

### 收尾(不在本计划内执行)

- 打包 `scripts/package.sh 0.1.17`、提交版本号、打 tag、发布:先问用户。
- 实机验证清单第十五至十八项由用户执行。
