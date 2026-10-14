//! COM 入口:模块句柄、引用计数、4 个 Dll* 导出与 DllMain。

mod factory;
mod launch;
mod log;
mod pipe;
mod registry;
mod service;

use core::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};

use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, E_FAIL, HINSTANCE, HMODULE, S_FALSE, S_OK,
};
use windows::Win32::System::Com::IClassFactory;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::core::{BOOL, GUID, HRESULT, HSTRING, Interface};
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

pub(crate) fn lock_module() {
    DLL_REFERENCES.fetch_add(1, Ordering::SeqCst);
}

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
    registry::register_profile()
}

fn unregister() {
    registry::unregister_profile();
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
    if unsafe { *rclsid } != CLSID_PYIME {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = factory::ClassFactory.into();
    unsafe { factory.query(riid, ppv) }
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
