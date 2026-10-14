//! 拉起 `pyime-server.exe`(与本 DLL 同目录),带限流。

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use windows::Win32::System::SystemInformation::GetTickCount64;

use super::log;
use crate::wire::Throttle;

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static THROTTLE: Mutex<Option<Throttle>> = Mutex::new(None);

fn server_path() -> Option<PathBuf> {
    let dll = super::module_path().ok()?;
    let dll = PathBuf::from(dll.to_string());
    Some(dll.parent()?.join("pyime-server.exe"))
}

/// 限流(2 秒内至多一次)后启动 server,不等待;失败只写日志。
pub(crate) fn try_spawn_server() {
    let allowed = {
        let mut g = THROTTLE.lock().unwrap_or_else(|e| e.into_inner());
        let t = g.get_or_insert_with(|| Throttle::new(2000));
        t.allow(unsafe { GetTickCount64() })
    };
    if !allowed {
        return;
    }
    let Some(path) = server_path() else {
        log::write("拉起 server:取不到 DLL 路径");
        return;
    };
    if !path.exists() {
        log::write(&format!("拉起 server:{} 不存在", path.display()));
        return;
    }
    match Command::new(&path).creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW).spawn() {
        Ok(_) => log::write("已拉起 pyime-server"),
        Err(e) => log::write(&format!("拉起 server 失败:{e}")),
    }
}
