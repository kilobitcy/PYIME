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
