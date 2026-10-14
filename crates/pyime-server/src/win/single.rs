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
