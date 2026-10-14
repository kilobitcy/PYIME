# TSF mingw 交叉编译 Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 验证 `x86_64-pc-windows-gnu` 交叉编译出的 TSF DLL 能在 Win11 上注册、被宿主加载并上屏"测试",且不崩。

**Architecture:** 新增独立 cdylib crate `pyime-tsf`,先编空壳 DLL(阶段 0)验证工具链与注册,再加最小 TSF 逻辑(阶段 1)。纯逻辑(按键判定)放在不依赖 Windows 的模块里,可在 Linux 单测。不改 workspace profile,不拷青简代码。

**Tech Stack:** Rust,`windows` 0.62.2 + `windows-core` 0.62.2 + `windows-registry` 0.6.1,mingw-w64(`x86_64-w64-mingw32-gcc`,已装),`objdump`。

**Spec:** `docs/superpowers/specs/2026-10-14-tsf-gnu-spike-design.md`

## Global Constraints

- 分支 `spike/tsf-gnu`;不触碰 PIME 及 PYIME v0.1.x 的任何注册项,使用全新 GUID。
- 不改 workspace 根 `Cargo.toml` 的 `[profile]`;`panic=abort` 与静态链接只经 `scripts/build-tsf.sh` 的单次 `cargo rustc` 传入。
- Windows 专有依赖只放 `[target.'cfg(windows)'.dependencies]`;Linux 上 `cargo test --workspace` 与 `cargo clippy --workspace` 不得新增报错。
- DLL 的 `objdump -p` DLL Name 不得含 `libgcc`/`libwinpthread`;导出表须含 `DllGetClassObject`、`DllCanUnloadNow`、`DllRegisterServer`、`DllUnregisterServer`。
- Windows 实机:DLL 放 `C:\PYIME-spike\`,`regsvr32` 用管理员终端。
- 回复、commit、注释一律简体中文;commit 末尾加 `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`。
- push / 打 tag 前必须先问用户。不做:拷贝青简代码、引擎、server/IPC、候选窗、组字串、安装包、32 位。

## File Structure

- `Cargo.toml`(根):members 加 `crates/pyime-tsf`。
- `crates/pyime-tsf/Cargo.toml`:crate 定义与 cfg(windows) 依赖。
- `crates/pyime-tsf/src/lib.rs`:`mod keymap;` + `#[cfg(windows)] mod com;`。
- `crates/pyime-tsf/src/keymap.rs`:`is_letter_vk`,纯逻辑,Linux 可测。
- `crates/pyime-tsf/src/com/mod.rs`:GUID 常量、引用计数、`DllMain` 与 4 个导出。
- `crates/pyime-tsf/src/com/factory.rs`:`ClassFactory`(阶段 1)。
- `crates/pyime-tsf/src/com/registry.rs`:注册/反注册(阶段 1)。
- `crates/pyime-tsf/src/com/service.rs`:`TextService`,TIP + key sink + edit session(阶段 1)。
- `scripts/build-tsf.sh`:交叉编译。`scripts/check-tsf-dll.sh`:objdump 自检。
- `docs/superpowers/specs/2026-10-14-tsf-gnu-spike-design.md`:Task 8 回填结果。

---

## 阶段 0:空壳 DLL

### Task 1: crate 骨架 + keymap(TDD)

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/pyime-tsf/Cargo.toml`, `crates/pyime-tsf/src/lib.rs`, `crates/pyime-tsf/src/keymap.rs`

**Interfaces:**
- Produces: `pyime_tsf::keymap::is_letter_vk(vk: u32) -> bool`(a–z 的虚拟键码 0x41..=0x5A 为 true)。

- [ ] **Step 1: 根 Cargo.toml 加成员**

把 `members = ["crates/pyime-backend", "crates/pyime-engine"]` 改为:

```toml
members = ["crates/pyime-backend", "crates/pyime-engine", "crates/pyime-tsf"]
```

- [ ] **Step 2: 写 crates/pyime-tsf/Cargo.toml**

```toml
[package]
name = "pyime-tsf"
version = "0.0.1"
edition = "2021"
license = "GPL-3.0-only"
publish = false

[lib]
crate-type = ["cdylib"]

[target.'cfg(windows)'.dependencies]
windows-core = "0.62.2"
windows-registry = "0.6.1"
windows = { version = "0.62.2", features = [
    "Win32_Foundation",
    "Win32_System_Com",
    "Win32_System_LibraryLoader",
    "Win32_System_SystemServices",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_TextServices",
] }
```

- [ ] **Step 3: 写 lib.rs 与带失败测试的 keymap.rs**

`src/lib.rs`:

```rust
//! PYIME TSF 文本服务(spike)。

pub mod keymap;

#[cfg(windows)]
mod com;
```

`src/keymap.rs`(先只写测试,函数留桩使其失败):

```rust
//! 按键判定的纯逻辑,不依赖 Windows,Linux 上可单测。

/// 虚拟键码是否为字母 a–z(VK_A..=VK_Z 即 0x41..=0x5A)。
pub fn is_letter_vk(_vk: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 字母键范围() {
        assert!(is_letter_vk(0x41));
        assert!(is_letter_vk(0x5A));
        assert!(is_letter_vk(0x4D));
    }

    #[test]
    fn 非字母键() {
        assert!(!is_letter_vk(0x40));
        assert!(!is_letter_vk(0x5B));
        assert!(!is_letter_vk(0x30)); // 数字 0
        assert!(!is_letter_vk(0x20)); // 空格
    }
}
```

- [ ] **Step 4: 运行测试确认失败**

Run: `cargo test -p pyime-tsf 2>&1 | tail -15`
Expected: `字母键范围` FAILED(assertion failed)。

- [ ] **Step 5: 实现**

把函数体改为 `(0x41..=0x5A).contains(&vk)`。

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test -p pyime-tsf 2>&1 | tail -8`
Expected: `test result: ok. 2 passed`。

- [ ] **Step 7: 确认不影响 Linux 工作区**

Run: `cargo test --workspace 2>&1 | grep -E "test result|error|FAILED" | head; cargo clippy --workspace 2>&1 | tail -5`
Expected: 全部 `ok`,clippy 无新增 warning/error。

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock crates/pyime-tsf
git commit -m "pyime-tsf:crate 骨架与按键判定

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

注意:`Cargo.lock` 在开工前已有未提交改动,提交前先 `git diff Cargo.lock | head -30` 确认,只有新增依赖相关改动才一并提交,否则先问用户。

### Task 2: 空壳 DLL + 构建与自检脚本

**Files:**
- Create: `crates/pyime-tsf/src/com/mod.rs`, `scripts/build-tsf.sh`, `scripts/check-tsf-dll.sh`

**Interfaces:**
- Produces(供 Task 4–6 使用,均在 `com/mod.rs`):`CLSID_PYIME: GUID`、`CLSID_PYIME_STR: &str`、`GUID_PROFILE: GUID`、`LANGID_ZH_CN: u16`、`SERVICE_DESCRIPTION: &str`、`lock_module()`、`unlock_module()`、`module_path() -> windows::core::Result<HSTRING>`。

- [ ] **Step 1: 写 com/mod.rs(阶段 0:注册只写 InprocServer32)**

```rust
//! COM 入口:模块句柄、引用计数、4 个 Dll* 导出与 DllMain。

use core::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};

use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, E_FAIL, HINSTANCE, HMODULE, S_FALSE, S_OK,
};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::core::{BOOL, GUID, HRESULT, HSTRING};
use windows_registry::CLASSES_ROOT;

pub(crate) const CLSID_PYIME: GUID = GUID::from_u128(0xafe649c4_87d0_4e83_8bb0_733256caeadb);
/// [`CLSID_PYIME`] 的注册表字符串形式,两者必须同步改。
pub(crate) const CLSID_PYIME_STR: &str = "{AFE649C4-87D0-4E83-8BB0-733256CAEADB}";
pub(crate) const GUID_PROFILE: GUID = GUID::from_u128(0x152b7a26_b14e_4e0a_9251_358fd482a5bc);
/// zh-CN。
pub(crate) const LANGID_ZH_CN: u16 = 0x0804;
pub(crate) const SERVICE_DESCRIPTION: &str = "PYIME 实验";

/// 存活的 COM 对象 + LockServer 计数,DllCanUnloadNow 据它判断能否卸载。
static DLL_REFERENCES: AtomicIsize = AtomicIsize::new(0);
/// 本 DLL 的模块句柄,DllMain 加载时记下。
static DLL_MODULE: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

#[allow(dead_code)] // 阶段 1 的类厂与 TextService 使用
pub(crate) fn lock_module() {
    DLL_REFERENCES.fetch_add(1, Ordering::SeqCst);
}

#[allow(dead_code)]
pub(crate) fn unlock_module() {
    DLL_REFERENCES.fetch_sub(1, Ordering::SeqCst);
}

/// 本 DLL 在磁盘上的完整路径,注册 InprocServer32 用。
pub(crate) fn module_path() -> windows::core::Result<HSTRING> {
    let module = HMODULE(DLL_MODULE.load(Ordering::SeqCst));
    let mut buf = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
    if len == 0 || len >= buf.len() {
        return Err(E_FAIL.into());
    }
    Ok(HSTRING::from_wide(&buf[..len]))
}

fn clsid_key() -> String {
    format!("CLSID\\{CLSID_PYIME_STR}")
}

fn register() -> windows::core::Result<()> {
    let module = module_path()?;
    let base = CLASSES_ROOT.create(clsid_key())?;
    base.set_string("", SERVICE_DESCRIPTION)?;
    let inproc = base.create("InprocServer32")?;
    inproc.set_string("", module.to_string())?;
    inproc.set_string("ThreadingModel", "Apartment")?;
    Ok(())
}

fn unregister() {
    let _ = CLASSES_ROOT.remove_tree(clsid_key());
}

#[no_mangle]
extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return E_FAIL;
    }
    // 阶段 0 没有类厂;阶段 1 (Task 5) 在此返回 ClassFactory。
    CLASS_E_CLASSNOTAVAILABLE
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    if DLL_REFERENCES.load(Ordering::SeqCst) == 0 { S_OK } else { S_FALSE }
}

#[no_mangle]
extern "system" fn DllRegisterServer() -> HRESULT {
    match register() {
        Ok(()) => S_OK,
        Err(error) => error.code(),
    }
}

#[no_mangle]
extern "system" fn DllUnregisterServer() -> HRESULT {
    unregister();
    S_OK
}

#[no_mangle]
extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        DLL_MODULE.store(hinst.0, Ordering::SeqCst);
    }
    true.into()
}
```

- [ ] **Step 2: 写 scripts/build-tsf.sh 并 `chmod +x`**

```bash
#!/usr/bin/env bash
# 交叉编译 pyime-tsf(x86_64-pc-windows-gnu)。panic=abort 与静态链接只在此处传入,不改 workspace profile。
# 可用 PYIME_TSF_RUSTFLAGS 覆盖默认 rustflags(阶段 0 用来试静态链接参数)。
set -euo pipefail
cd "$(dirname "$0")/.."
FLAGS="${PYIME_TSF_RUSTFLAGS:--C panic=abort -C target-feature=+crt-static}"
RUSTFLAGS="$FLAGS" cargo build --release --target x86_64-pc-windows-gnu -p pyime-tsf
echo "产物: target/x86_64-pc-windows-gnu/release/pyime_tsf.dll"
```

- [ ] **Step 3: 写 scripts/check-tsf-dll.sh 并 `chmod +x`**

```bash
#!/usr/bin/env bash
# 自检 TSF DLL:依赖不得含 libgcc/libwinpthread,须导出 4 个 Dll* 符号。
set -euo pipefail
cd "$(dirname "$0")/.."
DLL="${1:-target/x86_64-pc-windows-gnu/release/pyime_tsf.dll}"
test -f "$DLL" || { echo "找不到 $DLL"; exit 1; }
OUT="$(x86_64-w64-mingw32-objdump -p "$DLL")"
echo "$OUT" | grep 'DLL Name' || true
if echo "$OUT" | grep -E 'DLL Name:.*(libgcc|libwinpthread|libstdc)'; then
  echo "FAIL: 依赖了不应出现的运行库"; exit 1
fi
for s in DllGetClassObject DllCanUnloadNow DllRegisterServer DllUnregisterServer; do
  echo "$OUT" | grep -qw "$s" || { echo "FAIL: 缺少导出 $s"; exit 1; }
done
echo "OK: $DLL"
```

- [ ] **Step 4: 先在 Linux 上 check Windows 目标**

Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-tsf > /tmp/tsf-check.log 2>&1; tail -30 /tmp/tsf-check.log`
Expected: `Finished`。若报 windows 0.62 API 签名不符(如 `GetModuleFileNameW` 参数、`HSTRING::from_wide` 返回类型),按编译器提示最小修正后重跑,不改 spec 的行为约定。

- [ ] **Step 5: 构建并自检**

Run: `scripts/build-tsf.sh > /tmp/tsf-build.log 2>&1; tail -5 /tmp/tsf-build.log; scripts/check-tsf-dll.sh`
Expected: 末行 `OK: ...pyime_tsf.dll`。若 FAIL 是依赖 libgcc/winpthread:改用 `PYIME_TSF_RUSTFLAGS="-C panic=abort -C link-arg=-static-libgcc -C link-arg=-static" scripts/build-tsf.sh` 重试,把最终可行参数写回脚本默认值。

- [ ] **Step 6: Commit**

```bash
git add crates/pyime-tsf scripts/build-tsf.sh scripts/check-tsf-dll.sh
git commit -m "pyime-tsf:空壳 DLL(阶段 0)与构建自检脚本

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 3: 阶段 0 Windows 实机验收(用户操作)

**Files:** 无

- [ ] **Step 1: 把 `target/x86_64-pc-windows-gnu/release/pyime_tsf.dll` 传到 Windows 的 `C:\PYIME-spike\`**(传输方式由用户决定)。

- [ ] **Step 2: 管理员 PowerShell 中注册**

Run: `regsvr32 C:\PYIME-spike\pyime_tsf.dll`
Expected: 弹窗"DllRegisterServer 在 ... 已成功"。

- [ ] **Step 3: 验证并反注册**

Run: `reg query "HKCR\CLSID\{AFE649C4-87D0-4E83-8BB0-733256CAEADB}\InprocServer32"` 应显示 DLL 路径;再 `regsvr32 /u C:\PYIME-spike\pyime_tsf.dll`,再次 `reg query` 应报"找不到"。

- [ ] **Step 4: 判定**

通过 → 继续 Task 4。失败 → 记录弹窗错误码/`reg query` 输出,按 spec"判定"一节给用户出报告,**不继续**阶段 1。

---

## 阶段 1:最小 TSF 逻辑

### Task 4: 类厂 + TextService(Activate/Deactivate + 按键上屏)

**Files:**
- Create: `crates/pyime-tsf/src/com/factory.rs`, `crates/pyime-tsf/src/com/service.rs`
- Modify: `crates/pyime-tsf/src/com/mod.rs`(加 `mod factory; mod service;`,`DllGetClassObject` 返回类厂)

**Interfaces:**
- Consumes: Task 1 的 `keymap::is_letter_vk`;Task 2 的 `lock_module`/`unlock_module`/`CLSID_PYIME`。
- Produces: `factory::ClassFactory`(无状态,`#[implement(IClassFactory)]`)、`service::TextService::new() -> TextService`。

- [ ] **Step 1: 写 factory.rs**

```rust
use core::ffi::c_void;

use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, E_FAIL};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::core::{BOOL, GUID, IUnknown, Interface, Ref, Result, implement};

use super::service::TextService;

/// 造 [`TextService`] 的无状态类厂。
#[implement(IClassFactory)]
pub struct ClassFactory;

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if !punkouter.is_null() {
            return CLASS_E_NOAGGREGATION.ok();
        }
        if riid.is_null() || ppvobject.is_null() {
            return E_FAIL.ok();
        }
        let unknown: IUnknown = TextService::new().into();
        unsafe { unknown.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, flock: BOOL) -> Result<()> {
        if flock.as_bool() {
            super::lock_module();
        } else {
            super::unlock_module();
        }
        Ok(())
    }
}
```

- [ ] **Step 2: 写 service.rs**

```rust
//! 文本服务:激活时注册 key sink;a–z(无 Ctrl/Alt)经 edit session 上屏固定文字。

use std::cell::{Cell, RefCell};

use windows::Win32::Foundation::{FALSE, LPARAM, TRUE, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU};
use windows::Win32::UI::TextServices::{
    ITfContext, ITfEditSession, ITfEditSession_Impl, ITfInsertAtSelection, ITfKeyEventSink,
    ITfKeyEventSink_Impl, ITfKeystrokeMgr, ITfTextInputProcessor, ITfTextInputProcessor_Impl,
    ITfThreadMgr, INSERT_TEXT_AT_SELECTION_FLAGS, TF_ES_READWRITE, TF_ES_SYNC,
};
use windows::core::{implement, ComObjectInterface, Interface, Ref, Result, BOOL, GUID};

use crate::keymap::is_letter_vk;

const TEXT: &str = "测试";

#[implement(ITfTextInputProcessor, ITfKeyEventSink)]
pub struct TextService {
    thread_mgr: RefCell<Option<ITfThreadMgr>>,
    client_id: Cell<u32>,
}

impl TextService {
    pub fn new() -> Self {
        super::lock_module();
        Self { thread_mgr: RefCell::new(None), client_id: Cell::new(0) }
    }
}

impl Drop for TextService {
    fn drop(&mut self) {
        super::unlock_module();
    }
}

fn key_down(vk: i32) -> bool {
    (unsafe { GetKeyState(vk) } as u16 & 0x8000) != 0
}

/// 吃不吃这个键:只吃无 Ctrl/Alt 的 a–z。
fn would_eat(wparam: WPARAM) -> bool {
    is_letter_vk(wparam.0 as u32) && !key_down(VK_CONTROL.0 as i32) && !key_down(VK_MENU.0 as i32)
}

#[implement(ITfEditSession)]
struct InsertSession {
    context: ITfContext,
}

impl ITfEditSession_Impl for InsertSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        let insert: ITfInsertAtSelection = self.context.cast()?;
        let utf16: Vec<u16> = TEXT.encode_utf16().collect();
        unsafe { insert.InsertTextAtSelection(ec, INSERT_TEXT_AT_SELECTION_FLAGS(0), &utf16)? };
        Ok(())
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<ITfThreadMgr>, tid: u32) -> Result<()> {
        let thread_mgr = ptim.ok()?.clone();
        self.client_id.set(tid);
        let keystroke: ITfKeystrokeMgr = thread_mgr.cast()?;
        let sink: ITfKeyEventSink = self.to_interface();
        unsafe { keystroke.AdviseKeyEventSink(tid, &sink, true)? };
        *self.thread_mgr.borrow_mut() = Some(thread_mgr);
        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        if let Some(thread_mgr) = self.thread_mgr.borrow_mut().take() {
            if let Ok(keystroke) = thread_mgr.cast::<ITfKeystrokeMgr>() {
                let _ = unsafe { keystroke.UnadviseKeyEventSink(self.client_id.get()) };
            }
        }
        Ok(())
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    fn OnSetFocus(&self, _foreground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(&self, _pic: Ref<ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(if would_eat(wparam) { TRUE } else { FALSE })
    }

    fn OnKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        if !would_eat(wparam) {
            return Ok(FALSE);
        }
        // 取不到上下文或 edit session 失败(只读文档、密码框等)时放行按键。
        let Ok(context) = pic.ok().cloned() else { return Ok(FALSE) };
        let session: ITfEditSession = InsertSession { context: context.clone() }.into();
        let hr = unsafe { context.RequestEditSession(self.client_id.get(), &session, TF_ES_SYNC | TF_ES_READWRITE) };
        Ok(if matches!(hr, Ok(code) if code.is_ok()) { TRUE } else { FALSE })
    }

    fn OnTestKeyUp(&self, _pic: Ref<ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn OnKeyUp(&self, _pic: Ref<ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn OnPreservedKey(&self, _pic: Ref<ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        Ok(FALSE)
    }
}
```

- [ ] **Step 3: 改 com/mod.rs**

顶部加 `mod factory;` `mod service;`,`use windows::core::Interface;`、`use windows::Win32::System::Com::IClassFactory;`,把 `DllGetClassObject` 里 `CLASS_E_CLASSNOTAVAILABLE` 的返回替换为:

```rust
    if unsafe { *rclsid } != CLSID_PYIME {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = factory::ClassFactory.into();
    unsafe { factory.query(riid, ppv) }
```
同时删掉 `lock_module`/`unlock_module` 上的 `#[allow(dead_code)]`。

- [ ] **Step 4: check 并按编译器提示最小修正**

Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-tsf > /tmp/tsf-check.log 2>&1; tail -40 /tmp/tsf-check.log`
Expected: `Finished`。windows 0.62 的签名(如 `BOOL` 所在模块、`Ref::ok` 返回、`RequestEditSession` 返回类型、`GetKeyState` 参数类型)若不符,按提示改,保持上述行为不变。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-tsf
git commit -m "pyime-tsf:类厂与 TextService(key sink + 上屏测试)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 5: 输入法注册(profile + 8 个类别)

**Files:**
- Create: `crates/pyime-tsf/src/com/registry.rs`
- Modify: `crates/pyime-tsf/src/com/mod.rs`(加 `mod registry;`,`register()`/`unregister()` 末尾调用)

**Interfaces:**
- Consumes: Task 2 的 `CLSID_PYIME`、`GUID_PROFILE`、`LANGID_ZH_CN`、`SERVICE_DESCRIPTION`。
- Produces: `registry::register_profile() -> Result<()>`、`registry::unregister_profile()`。

- [ ] **Step 1: 写 registry.rs**

```rust
//! 经 ITfInputProcessorProfiles / ITfCategoryMgr 把本 DLL 登记成键盘类文本服务。

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles, GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
    GUID_TFCAT_TIP_KEYBOARD, GUID_TFCAT_TIPCAP_COMLESS, GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
    GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT, GUID_TFCAT_TIPCAP_SECUREMODE,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT, GUID_TFCAT_TIPCAP_UIELEMENTENABLED, ITfCategoryMgr,
    ITfInputProcessorProfiles,
};
use windows::core::{GUID, Result};

use super::{CLSID_PYIME, GUID_PROFILE, LANGID_ZH_CN, SERVICE_DESCRIPTION};

/// 只登记键盘类 TIP,Win10/11 的输入切换器会把它过滤掉;须同时声明沉浸式、托盘等能力。
const CATEGORIES: &[GUID] = &[
    GUID_TFCAT_TIP_KEYBOARD,
    GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
    GUID_TFCAT_TIPCAP_SECUREMODE,
    GUID_TFCAT_TIPCAP_COMLESS,
    GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT,
    GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
    GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
];

pub(crate) fn register_profile() -> Result<()> {
    com_scope(|| {
        let profiles: ITfInputProcessorProfiles = unsafe {
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?
        };
        // AddLanguageProfile 按 0 结尾读字符串,不补 0 会多读相邻内存。
        let description = wide_z(SERVICE_DESCRIPTION);
        unsafe {
            profiles.Register(&CLSID_PYIME)?;
            profiles.AddLanguageProfile(&CLSID_PYIME, LANGID_ZH_CN, &GUID_PROFILE, &description, &[0], 0)?;
        }
        let category: ITfCategoryMgr =
            unsafe { CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)? };
        for catid in CATEGORIES {
            unsafe { category.RegisterCategory(&CLSID_PYIME, catid, &CLSID_PYIME)? };
        }
        Ok(())
    })
}

/// 尽力而为的反注册。
pub(crate) fn unregister_profile() {
    let _ = com_scope(|| {
        if let Ok(category) = unsafe {
            CoCreateInstance::<_, ITfCategoryMgr>(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)
        } {
            for catid in CATEGORIES {
                let _ = unsafe { category.UnregisterCategory(&CLSID_PYIME, catid, &CLSID_PYIME) };
            }
        }
        if let Ok(profiles) = unsafe {
            CoCreateInstance::<_, ITfInputProcessorProfiles>(
                &CLSID_TF_InputProcessorProfiles,
                None,
                CLSCTX_INPROC_SERVER,
            )
        } {
            unsafe {
                let _ = profiles.RemoveLanguageProfile(&CLSID_PYIME, LANGID_ZH_CN, &GUID_PROFILE);
                let _ = profiles.Unregister(&CLSID_PYIME);
            }
        }
        Ok(())
    });
}

/// regsvr32 一般已初始化过 COM;已是别的套间模式就不配对反初始化。
fn com_scope<T>(body: impl FnOnce() -> Result<T>) -> Result<T> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let result = body();
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

fn wide_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
```

- [ ] **Step 2: 接入 com/mod.rs**

加 `mod registry;`;`register()` 的 `Ok(())` 前改为 `registry::register_profile()`;`unregister()` 开头加 `registry::unregister_profile();`。

- [ ] **Step 3: check**

Run: `cargo check --target x86_64-pc-windows-gnu -p pyime-tsf > /tmp/tsf-check.log 2>&1; tail -30 /tmp/tsf-check.log`
Expected: `Finished`;签名不符按提示最小修正。

- [ ] **Step 4: Commit**

```bash
git add crates/pyime-tsf
git commit -m "pyime-tsf:输入法 profile 与类别注册

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 6: 构建、自检、Linux 回归

**Files:** 无新增

- [ ] **Step 1: 构建并自检**

Run: `scripts/build-tsf.sh > /tmp/tsf-build.log 2>&1; tail -5 /tmp/tsf-build.log; scripts/check-tsf-dll.sh`
Expected: 末行 `OK: ...`,DLL Name 列表只有 Windows 系统 DLL(KERNEL32/ole32/OLEAUT32/api-ms-win-* 等)。

- [ ] **Step 2: Linux 回归**

Run: `cargo test --workspace 2>&1 | grep -E "test result|FAILED|error"; cargo clippy --workspace 2>&1 | tail -3`
Expected: 全部 ok,无新增 warning。

- [ ] **Step 3: 若 Step 1/2 有改动,Commit**

```bash
git add -A crates/pyime-tsf scripts
git commit -m "pyime-tsf:构建参数与自检修正

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

### Task 7: 阶段 1 Windows 实机验收(用户操作)

**Files:** 无

- [ ] **Step 1:** 先 `regsvr32 /u` 旧的空壳 DLL,再把新 `pyime_tsf.dll` 覆盖到 `C:\PYIME-spike\`。管理员 PowerShell:`regsvr32 C:\PYIME-spike\pyime_tsf.dll`,期望成功弹窗。
- [ ] **Step 2:** 设置 → 时间和语言 → 语言和区域 → 中文(简体)→ 键盘,应出现"PYIME 实验"。**若没出现,不要判定 gnu 失败**:记录并先查类别注册(`reg query` 与 Task 5 的类别表)。
- [ ] **Step 3:** 在记事本、Edge/Chrome 输入框、Windows Terminal 切换到"PYIME 实验",按 a–z:期望上屏"测试";按 Ctrl+C/Ctrl+A 等应正常(未被吃掉)。某宿主不上屏但按键正常放行,记录为"该宿主放行",不算失败。
- [ ] **Step 4:** 连续输入约 10 分钟,观察宿主不崩。若崩溃,事件查看器 → Windows 日志 → 应用程序,记录 Faulting module 是否为 `pyime_tsf.dll`。
- [ ] **Step 5:** `regsvr32 /u C:\PYIME-spike\pyime_tsf.dll`;`reg query` 确认 CLSID 项消失;注销重新登录后 DLL 文件可删;确认已装 PIME 与 PYIME v0.1.7 仍可用。

### Task 8: 记录结果并给出判定

**Files:**
- Modify: `docs/superpowers/specs/2026-10-14-tsf-gnu-spike-design.md`(末尾追加"结果"一节)

- [ ] **Step 1:** 追加"## 结果"一节:最终可用的 rustflags、objdump 的 DLL Name 列表、阶段 0/1 各验收项通过与否、遇到的问题与修法、宿主兼容记录。
- [ ] **Step 2:** 按 spec"判定"一节写明结论(进入后续里程碑:搬青简 `tsf/` 并新建 `pyime-protocol` / 改 MSVC / 其他),Commit:

```bash
git add docs/superpowers/specs/2026-10-14-tsf-gnu-spike-design.md
git commit -m "spec:TSF gnu spike 结果记录

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 3:** 向用户汇报结论;push / 合并 / 后续路线均先问用户。
