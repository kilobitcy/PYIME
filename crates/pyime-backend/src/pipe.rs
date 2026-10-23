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
