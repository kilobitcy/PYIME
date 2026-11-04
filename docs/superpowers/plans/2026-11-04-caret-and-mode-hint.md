# 成对括号光标与中/英切换提示 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按住 Shift 打成对括号时光标停在中间;单击 Shift 切换中/英时光标旁闪"中"/"英",并顺带修好组字中切换不上屏、连按两下第二下不切换。

**Architecture:** 服务端 `send_key` 模拟方向键时把按着的 Shift 在同一次 `SendInput` 里抬起再按回;单击 Shift 的切换从无会话的 `filterKeyUp` 挪到带编辑会话的 `onKeyUp`,回复带 PIME 原有的 `showMessage`;DLL 只改提示窗外观、定位(不开空组字、多级回退)和按键时收起。

**Tech Stack:** Rust(pyime-backend,`cargo test`)、C++(tsf/,mingw 交叉编译 `scripts/build-tsf.sh`)。

**Spec:** `docs/superpowers/specs/2026-11-04-caret-and-mode-hint-design.md`

## Global Constraints

- 回复、commit、注释一律简体中文;commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- 工作区里 `CLAUDE.md`、`Cargo.lock`、`crates/pyime-backend/Cargo.toml`、`crates/pyime-engine/Cargo.toml` 是用户未提交的改动:不得 `git add` 它们,不得 `git stash`/`checkout`/`reset`。提交只 `git add` 本任务列出的文件。
- 不得为通过测试削弱已有测试;只改本任务列出的文件。
- 提示文字固定为 `"中"` / `"英"`(切换后的状态),`duration` 为 `1`。
- 托盘左键、"本程序默认中文"菜单引起的切换不带 `showMessage`。
- 提示窗外观:白底 `RGB(0xFF,0xFF,0xFF)`、1px 边框 `RGB(0xE5,0xE5,0xE5)`、文字 `RGB(0x33,0x33,0x33)`。
- Shift 扫描码:左 `0x2A`、右 `0x36`;Shift 事件不带 `KEYEVENTF_EXTENDEDKEY`,方向键带。
- 推送、打 tag、发布前必须先问用户(本计划不做这些)。

---

### Task 1: 模拟方向键时临时抬起 Shift

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs`(`send_key` 约 40–68 行;在其上方新增纯函数)
- Test: `crates/pyime-backend/src/ime/tests.rs`(末尾追加)

**Interfaces:**
- Produces: `pub(crate) struct KeyEv { pub vk: u16, pub scan: u16, pub up: bool, pub extended: bool }`(派生 `Debug, Clone, Copy, PartialEq, Eq`);`pub(crate) fn arrow_events(vk: u16, lshift: bool, rshift: bool) -> Vec<KeyEv>`。`Ime::press` 签名不变。

- [ ] **Step 1: 写失败测试**(追加到 `crates/pyime-backend/src/ime/tests.rs` 末尾;该文件已 `use super::*;`,若没有则在新测试前加 `use super::{arrow_events, KeyEv};`)

```rust
#[test]
fn 无shift时只发方向键() {
    let ev = arrow_events(37, false, false);
    assert_eq!(ev, vec![KeyEv { vk: 37, scan: 0, up: false, extended: true }, KeyEv { vk: 37, scan: 0, up: true, extended: true }]);
}

#[test]
fn 按着左shift时先抬起再按回() {
    let ev = arrow_events(37, true, false);
    let lift = KeyEv { vk: 0xA0, scan: 0x2A, up: true, extended: false };
    let back = KeyEv { vk: 0xA0, scan: 0x2A, up: false, extended: false };
    assert_eq!(ev, vec![lift, KeyEv { vk: 37, scan: 0, up: false, extended: true }, KeyEv { vk: 37, scan: 0, up: true, extended: true }, back]);
}

#[test]
fn 左右shift都按着时都抬起_按回顺序相反() {
    let ev = arrow_events(39, true, true);
    let vks: Vec<(u16, bool)> = ev.iter().map(|e| (e.vk, e.up)).collect();
    assert_eq!(vks, vec![(0xA0, true), (0xA1, true), (39, false), (39, true), (0xA1, false), (0xA0, false)]);
    assert_eq!(ev[1].scan, 0x36);
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -q -p pyime-backend shift 2>&1 | tail -5`
Expected: 编译失败,`arrow_events`/`KeyEv` 未定义。

- [ ] **Step 3: 实现纯函数**(放在 `send_key` 的文档注释之前,不加 `cfg`)

```rust
/// 模拟按键的一个事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyEv {
    pub vk: u16,
    pub scan: u16,
    pub up: bool,
    pub extended: bool,
}

/// 模拟一次方向键的事件序列。用户按着 Shift(打 `(` 时常未松开)会让应用收到 Shift+方向键而选中文字,
/// 故按着的 Shift 先抬起、发完方向键再按回;整串由一次 `SendInput` 发出,不与用户输入交错。
pub(crate) fn arrow_events(vk: u16, lshift: bool, rshift: bool) -> Vec<KeyEv> {
    let shifts: Vec<(u16, u16)> = [(lshift, 0xA0, 0x2A), (rshift, 0xA1, 0x36)].iter().filter(|s| s.0).map(|s| (s.1, s.2)).collect();
    let mut ev: Vec<KeyEv> = shifts.iter().map(|&(vk, scan)| KeyEv { vk, scan, up: true, extended: false }).collect();
    ev.push(KeyEv { vk, scan: 0, up: false, extended: true });
    ev.push(KeyEv { vk, scan: 0, up: true, extended: true });
    ev.extend(shifts.iter().rev().map(|&(vk, scan)| KeyEv { vk, scan, up: false, extended: false }));
    ev
}
```

- [ ] **Step 4: `send_key` 改用它**(替换原 `send_key` 函数体中从 `#[link(name = "user32")]` 到末尾的部分;结构体 `KeybdInput`/`Input` 保留)

```rust
    #[link(name = "user32")]
    extern "system" {
        fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
        fn GetAsyncKeyState(vk: i32) -> i16;
    }
    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_EXTENDEDKEY: u32 = 1;
    const KEYEVENTF_KEYUP: u32 = 2;
    // SAFETY: GetAsyncKeyState 只读全局键盘状态。
    let down = |vk: i32| unsafe { GetAsyncKeyState(vk) } < 0;
    // 剩余竞态:查询与 SendInput 之间(微秒级)用户恰好松开 Shift,Shift 会逻辑上停在按下,直到下次按 Shift
    let inputs: Vec<Input> = arrow_events(vk, down(0xA0), down(0xA1))
        .into_iter()
        .map(|e| {
            let flags = if e.up { KEYEVENTF_KEYUP } else { 0 } | if e.extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
            Input { kind: INPUT_KEYBOARD, ki: KeybdInput { vk: e.vk, scan: e.scan, flags, time: 0, extra: 0 }, pad: [0; 8] }
        })
        .collect();
    // SAFETY: inputs 是有效的 INPUT 数组,布局与 Win32 INPUT(64 位下 40 字节)一致。
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<Input>() as i32) };
}
```

并把 `send_key` 上方的文档注释改为:`/// 向前台应用注入一次方向键(按下 + 弹起),用于成对符号补全后回退/跳过光标;按着的 Shift 临时抬起。`

- [ ] **Step 5: 测试与 Windows 交叉编译**

Run: `cargo test -q -p pyime-backend 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: 全部 `ok`。
Run: `cargo build -q --release --target x86_64-pc-windows-gnu -p pyime-backend 2>&1 | grep -E "^(error|warning)" | head`
Expected: 无输出(`send_key` 只在 Windows 下编译,必须用交叉编译验证)。
Run: `cargo clippy -q -p pyime-backend --all-targets 2>&1 | grep -A3 "ime.rs" | head`
Expected: 无涉及新代码的告警。

- [ ] **Step 6: 提交**

```bash
git add crates/pyime-backend/src/ime.rs crates/pyime-backend/src/ime/tests.rs
git commit -m "成对括号:模拟方向键时临时抬起按着的 Shift,免得选中右括号

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 2: 单击 Shift 改在 onKeyUp 切换并提示

**Files:**
- Modify: `crates/pyime-backend/src/session.rs`(字段约 92–98 行、`Session::new` 约 130 行、`handle` 约 183–217 行)
- Test: `crates/pyime-backend/src/session/tests.rs`(`shift_click` 约 171 行;末尾追加新测试)

**Interfaces:**
- Consumes: 无(与 Task 1 独立)。
- Produces: 回复字段 `showMessage = {"message": "中"|"英", "duration": 1}`,只出现在 `onKeyUp` 的回复里;单击 Shift 时 `filterKeyUp` 回 `"return": true`。Task 3 的 DLL 依赖此行为。

- [ ] **Step 1: 改测试辅助函数 `shift_click`,模拟 TSF 流程**(替换 tests.rs 中原函数)

```rust
/// 单击 Shift,按 TSF 流程:filterKeyUp 回 true 时接着发 onKeyUp(带编辑会话),返回最后一个回复。
fn shift_click(s: &mut Session) -> Value {
    key(s, "filterKeyDown", 16);
    let r = key(s, "filterKeyUp", 16);
    if r["return"] == true { key(s, "onKeyUp", 16) } else { r }
}
```

- [ ] **Step 2: 写失败测试**(追加到 tests.rs 末尾)

```rust
#[test]
fn 单击shift在filterkeyup只回true_onkeyup才切换并提示() {
    let mut s = session();
    key(&mut s, "filterKeyDown", 16);
    let r = key(&mut s, "filterKeyUp", 16);
    assert_eq!(r["return"], true);
    assert!(r.get("showMessage").is_none() && r.get("changeButton").is_none());
    assert_eq!(key(&mut s, "filterKeyUp", 16)["return"], true); // OnKeyUp 前的第二次 filterKeyUp
    let r = key(&mut s, "onKeyUp", 16);
    assert_eq!(r["return"], true);
    assert_eq!(r["showMessage"], json!({"message": "英", "duration": 1}));
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], false);
    assert_eq!(shift_click(&mut s)["showMessage"]["message"], "中");
}

#[test]
fn 同一次抬键两次filterkeyup_紧接着再单击仍切换() {
    let mut s = session();
    for _ in 0..2 {
        key(&mut s, "filterKeyDown", 16);
        key(&mut s, "filterKeyUp", 16);
        key(&mut s, "filterKeyUp", 16);
        key(&mut s, "onKeyUp", 16);
    }
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true); // 切了两次,回到中文
}

#[test]
fn 组字中单击shift_拼音在onkeyup回复里上屏() {
    let mut s = session();
    press(&mut s, &[N, I]);
    key(&mut s, "filterKeyDown", 16);
    assert!(key(&mut s, "filterKeyUp", 16).get("commitString").is_none());
    let r = key(&mut s, "onKeyUp", 16);
    assert_eq!(r["commitString"], "ni");
    assert_eq!(r["showMessage"]["message"], "英");
}

#[test]
fn 宿主没调onkeyup时下一次按键补切换() {
    let mut s = session();
    key(&mut s, "filterKeyDown", 16);
    key(&mut s, "filterKeyUp", 16);
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], false);
}

#[test]
fn 托盘左键切换不提示() {
    let mut s = win8_session();
    assert!(left_click(&mut s).get("showMessage").is_none());
}

#[test]
fn 按住shift打括号_注入回声后松开不切换() {
    let mut s = session();
    key(&mut s, "filterKeyDown", 16); // 用户按住 Shift
    pk(&mut s, 57, true); // Shift+9 → （）,并模拟 ←
    key(&mut s, "filterKeyUp", 16); // 注入的 Shift↑ 经输入法回来
    key(&mut s, "filterKeyDown", 16); // 注入的 Shift↓ 回来
    assert_eq!(key(&mut s, "filterKeyUp", 16)["return"], false); // 用户松开
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true); // 仍是中文
}
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -q -p pyime-backend --lib session 2>&1 | grep -E "FAILED|panicked|test result" | head -20`
Expected: 新增的前 4 个测试失败(`filterKeyUp` 当场切换、无 `showMessage`);`托盘左键切换不提示`、`按住shift打括号_注入回声后松开不切换` 可能已通过(它们锁住现有行为)。

- [ ] **Step 4: 实现**

`session.rs` 字段区在 `shift_guard: Duration,` 之后加:

```rust
    /// 本次 Shift 抬起已判为单击:filterKeyUp 的回复没有编辑会话、不能上屏,
    /// 故回 true 让 TSF 接着调带会话的 onKeyUp,在那里切换。
    toggle_on_up: bool,
```

`Session::new` 的结构体字面量在 `shift_guard: SHIFT_GUARD,` 之后加 `toggle_on_up: false,`。

`handle` 中 `m @ ("filterKeyDown" | "onKeyDown") => {` 之后第一行插入:

```rust
                // 宿主没调 onKeyUp(少见):下一次按下时补切换,不提示
                if std::mem::take(&mut self.toggle_on_up) {
                    self.toggle_english(&mut resp);
                }
```

把 `"filterKeyUp" | "onKeyUp"` 分支里原来的

```rust
                if is_shift && req["method"] == "filterKeyUp" {
                    if std::mem::take(&mut self.shift_pending) {
                        if !self.shift_misfire {
                            self.toggle_english(&mut resp);
                        }
                    } else {
                        self.shift_combo_up = Some(Instant::now());
                    }
                }
```

替换为:

```rust
                if is_shift && req["method"] == "filterKeyUp" {
                    if std::mem::take(&mut self.shift_pending) {
                        self.toggle_on_up = !self.shift_misfire;
                    } else if !self.toggle_on_up {
                        // 已判为单击时,同一次抬键的第二次 filterKeyUp(OnTestKeyUp 后 OnKeyUp 又调一次)不算组合松开
                        self.shift_combo_up = Some(Instant::now());
                    }
                    if self.toggle_on_up {
                        resp["return"] = json!(true);
                    }
                } else if is_shift && std::mem::take(&mut self.toggle_on_up) {
                    self.toggle_english(&mut resp);
                    resp["showMessage"] = json!({ "message": if self.english { "英" } else { "中" }, "duration": 1 });
                    resp["return"] = json!(true);
                }
```

- [ ] **Step 5: 跑测试**

Run: `cargo test -q -p pyime-backend 2>&1 | grep -E "test result|FAILED|panicked"`
Expected: 全部 `ok`(含原有 Shift 相关测试:它们经新的 `shift_click` 走 onKeyUp)。
Run: `cargo clippy -q -p pyime-backend --all-targets 2>&1 | grep -B2 -A6 "session" | head -30`
Expected: 除既有的 `session.rs:348` 布尔表达式告警外,无新告警。

- [ ] **Step 6: 提交**

```bash
git add crates/pyime-backend/src/session.rs crates/pyime-backend/src/session/tests.rs
git commit -m "单击 Shift 改在带编辑会话的 onKeyUp 切换并提示中/英;修组字中切换不上屏、连按两下第二下不切换

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 3: DLL 提示窗外观、定位与收起

**Files:**
- Modify: `tsf/libIME2/src/TextService.h`(`selectionRect` 声明后,约 139 行)
- Modify: `tsf/libIME2/src/TextService.cpp`(`selectionRect` 之后,约 940 行)
- Modify: `tsf/libIME2/src/MessageWindow.cpp`(`onPaint` 桌面模式分支,约 94–100 行)
- Modify: `tsf/PIMETextService/PIMETextService.h`(私有成员区 `messageTimerId_` 附近,约 161 行)
- Modify: `tsf/PIMETextService/PIMETextService.cpp`(`onKeyDown` 约 108 行、`updateCandidatesWindow` 约 300 行、`showMessage` 约 354 行)
- Modify: `tsf/PIMETextService/PIMEClient.cpp`(`updateMessageWindow` 约 186–198 行)

**Interfaces:**
- Consumes: Task 2 —— `showMessage` 只在 `onKeyUp` 回复里出现(带编辑会话)。
- Produces: 无(DLL 内部)。

本任务无单测;验收为交叉编译通过 + Task 4 的实机清单。

- [ ] **Step 1: libIME2 新增 `caretRect`**

`TextService.h` 在 `bool selectionRect(EditSession* session, RECT* rect) const;` 下一行加:

```cpp
    bool caretRect(EditSession* session, RECT* rect) const;  // PYIME:同 selectionRect,但不要求在组字中
```

`TextService.cpp` 在 `selectionRect` 函数之后加:

```cpp
// PYIME:同 selectionRect,但不要求在组字中(不组字时显示中/英提示用)。
bool TextService::caretRect(EditSession* session, RECT* rect) const {
    bool ret = false;
    ComPtr<ITfContextView> view;
    if(session->context()->GetActiveView(&view) == S_OK) {
        BOOL clipped;
        TF_SELECTION selection;
        ULONG selectionNum;
        if(session->context()->GetSelection(session->editCookie(), TF_DEFAULT_SELECTION, 1, &selection, &selectionNum) == S_OK) {
            if(view->GetTextExt(session->editCookie(), selection.range, rect, &clipped) == S_OK)
                ret = true;
            selection.range->Release();
        }
    }
    return ret;
}
```

- [ ] **Step 2: 提示窗外观与候选窗一致**

`MessageWindow.cpp` `onPaint` 的 `else { // desktop` 分支,把

```cpp
        SetTextColor(hDC, GetSysColor(COLOR_INFOTEXT));
        SetBkColor(hDC, GetSysColor(COLOR_INFOBK));
        // draw a 3d border in desktop mode
        ::FillSolidRect(hDC, &rc, ::GetSysColor(COLOR_INFOBK));
        ::Draw3DBorder(hDC, &rc, GetSysColor(COLOR_3DFACE), 0);
```

替换为:

```cpp
        // PYIME:与候选窗一致,白底、1px 浅灰边框、深灰字
        SetTextColor(hDC, RGB(0x33, 0x33, 0x33));
        ::FillSolidRect(hDC, &rc, RGB(0xFF, 0xFF, 0xFF));
        HBRUSH border = ::CreateSolidBrush(RGB(0xE5, 0xE5, 0xE5));
        ::FrameRect(hDC, &rc, border);
        ::DeleteObject(border);
```

- [ ] **Step 3: 定位回退与记住候选窗位置**

`PIMETextService.h` 私有成员区 `UINT messageTimerId_;` 下一行加:

```cpp
	POINT lastCandPos_ = {0, 0};  // PYIME:最近一次候选窗位置,提示定位的最后回退
	bool hasCandPos_ = false;
```

并在同一私有区(`private:` 下第一行)加声明:

```cpp
	bool messageAnchor(Ime::EditSession* session, POINT* pt);
```

`PIMETextService.cpp` `updateCandidatesWindow` 中 `candidateWindow_->move(textRect.left, textRect.bottom);` 之后加:

```cpp
            lastCandPos_ = {textRect.left, textRect.bottom};
            hasCandPos_ = true;
```

把整个 `TextService::showMessage` 替换为下面两个函数:

```cpp
// message window
// PYIME:不再临时开组字;定位取不到就不显示(不落到屏幕左上角)
void TextService::showMessage(Ime::EditSession* session, std::wstring message, int duration) {
	// remove previous message if there's any
	hideMessage();
	POINT pt;
	if (!messageAnchor(session, &pt))
		return;
	messageWindow_ = make_unique<Ime::MessageWindow>(this, session);
	messageWindow_->setFont(font_);
	messageWindow_->setText(message);
	messageWindow_->move(pt.x, pt.y);
	messageWindow_->show();

	messageTimerId_ = ::SetTimer(messageWindow_->hwnd(), 1, duration * 1000, (TIMERPROC)TextService::onMessageTimeout);
}

// PYIME:提示位置依次取 组字/光标处文字矩形的左下角、系统插入符、最近一次候选窗位置。
bool TextService::messageAnchor(Ime::EditSession* session, POINT* pt) {
	RECT rc = {0};
	bool ok = isComposing() ? selectionRect(session, &rc) : caretRect(session, &rc);
	if (ok && (rc.left || rc.top || rc.right || rc.bottom)) {
		pt->x = rc.left;
		pt->y = rc.bottom;
		return true;
	}
	GUITHREADINFO gti = {sizeof(gti)};
	if (::GetGUIThreadInfo(::GetCurrentThreadId(), &gti) && gti.hwndCaret) {
		POINT p = {gti.rcCaret.left, gti.rcCaret.bottom};
		if (::ClientToScreen(gti.hwndCaret, &p)) {
			*pt = p;
			return true;
		}
	}
	if (hasCandPos_) {
		*pt = lastCandPos_;
		return true;
	}
	return false;
}
```

- [ ] **Step 4: 不开空组字、无会话不显示**

`PIMEClient.cpp` `updateMessageWindow` 中把

```cpp
        if (message.isString() && duration.isInt()) {
            if (!textService_->isComposing()) {
                textService_->startComposition(session->context());
                endComposition = true;
            }
            textService_->showMessage(session, utf8ToUtf16(message.asCString()), duration.asInt());
        }
```

替换为:

```cpp
        // PYIME:无编辑会话(filterKey*/onCommand)时不显示,否则 MessageWindow 取父窗口会解引用空会话;
        // 不再临时开空组字取位置(网页编辑器会收到一对空的 compositionstart/end),定位见 TextService::messageAnchor
        if (message.isString() && duration.isInt() && session != nullptr) {
            textService_->showMessage(session, utf8ToUtf16(message.asCString()), duration.asInt());
        }
```

`endComposition` 参数保留不动(调用方仍传入,此函数不再置位)。

- [ ] **Step 5: 接着打字时收起提示**

`PIMETextService.cpp` `TextService::onKeyDown` 中 `if (!client_) return false;` 之前加:

```cpp
	// PYIME:接着打字时收起中/英提示,免得与候选窗叠在一起
	if (messageWindow_)
		hideMessage();
```

- [ ] **Step 6: 交叉编译**

Run: `scripts/build-tsf.sh > /tmp/tsf-build.log 2>&1; echo exit=$?; grep -iE "error|warning" /tmp/tsf-build.log | head -20; ls -la target/tsf/x64/PYIME.dll target/tsf/x86/PYIME.dll`
Expected: `exit=0`,无 error;两个 DLL 时间为刚才。若有 warning,只处理本任务新代码引起的。

- [ ] **Step 7: 提交**

```bash
git add tsf/libIME2/src/TextService.h tsf/libIME2/src/TextService.cpp tsf/libIME2/src/MessageWindow.cpp tsf/PIMETextService/PIMETextService.h tsf/PIMETextService/PIMETextService.cpp tsf/PIMETextService/PIMEClient.cpp
git commit -m "前端:中/英提示窗改白底细边框,不开空组字定位并多级回退,打字时收起

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 4: 实机清单、说明与全量验证

**Files:**
- Modify: `test-kit/tsf.txt`(末尾追加第十节;文件为 UTF-8 BOM + CRLF,保持)
- Modify: `CLAUDE.md`(只改不提交)

**Interfaces:**
- Consumes: Task 1–3 全部产物。

- [ ] **Step 1: 实机清单**

用 Python 以二进制追加(保持 CRLF),节前空一行,与前面各节一致;编号接第九节的 23:

```python
p = 'test-kit/tsf.txt'
lines = ["", "十、成对括号光标与中/英提示(0.1.18)",
 "24. 记事本、Chrome/Edge、微信、WPS 中(中文状态)按住 Shift 连打 (),松开:光标在（）中间,没有选中右括号;再打 ) 光标越过右括号,不多出字符。",
 "25. 单击 Shift:光标旁闪\"英\"约 1 秒(白底细边框),托盘图标同步变;再单击闪\"中\"。快速连按两下 Shift,两下都切换(最终回到原状态)。",
 "26. 组字中(如打了 ni)单击 Shift:拼音 ni 原样上屏并切到英文,闪\"英\"。",
 "27. 单击 Shift 后接着打字母:提示立即收起;字母不是大写(Shift 没有卡住)。",
 "28. 托盘左键切换中/英:不闪提示。",
 "29. 记下提示没出现或位置明显不对的程序名,连同 tsf.log 贴回。"]
with open(p, 'ab') as f:
    f.write(("\r\n".join(lines) + "\r\n").encode())
```

Run: `tail -8 test-kit/tsf.txt | cut -c1-40; file test-kit/tsf.txt`
Expected: 末尾为第十节 24–29 项;`file` 仍报 `UTF-8 (with BOM) text, with CRLF line terminators`。

- [ ] **Step 2: CLAUDE.md 说明(不 `git add`)**

- "托盘图标与菜单"小节第一条 `左键切中英(…)` 之后补一句:"单击 Shift 在带编辑会话的 `onKeyUp` 切换(`filterKeyUp` 只回 true),回复带 `showMessage` 在光标旁闪"中/英"约 1 秒;托盘、菜单切换不提示。"
- "目标:尽量接近微信输入法"小节 `符号自动补全成对括号(默认开)` 之后补:"(模拟方向键回退光标时,按着的 Shift 在同一次 SendInput 里临时抬起再按回)"。
- "待办"末尾加一条:"成对括号光标与中/英提示(2026-11-04,分支 caret-and-mode-hint,设计 `docs/superpowers/specs/2026-11-04-caret-and-mode-hint-design.md`):待按 `test-kit/tsf.txt` 第十节实机验证。"

- [ ] **Step 3: 全量验证**

Run: `cargo test -q 2>&1 | grep -E "test result|FAILED|panicked" | sort | uniq -c`
Expected: 全部 `ok`,无 FAILED。
Run: `cargo clippy -q --all-targets 2>&1 | grep -E "^(warning|error)" | sort | uniq -c`
Expected: 与改动前相同的 7 条既有告警,无新增。
Run: `cargo build -q --release --target x86_64-pc-windows-gnu -p pyime-backend 2>&1 | grep -E "^(error|warning)" | head`
Expected: 无输出。

- [ ] **Step 4: 提交**

```bash
git add test-kit/tsf.txt
git commit -m "实机清单:第十节 成对括号光标与中/英提示

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
