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
