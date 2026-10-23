// GUI 子系统:由监护进程/Run 键/PIMELauncher 启动时不弹控制台窗;stdin/stdout 由启动方经 STARTUPINFO 传入,照常可用。
#![cfg_attr(windows, windows_subsystem = "windows")]
mod ime;
mod inputlog;
mod log;
mod punct;
mod protocol;
mod register;
mod session;
mod tray;

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

/// 安装器入口:`--register <PIME目录>` / `--unregister <PIME目录>`,修改其中的 backends.json。
fn edit_backends(dir: &str, edit: fn(&str) -> Result<String, String>) -> Result<(), String> {
    let path = std::path::Path::new(dir).join("backends.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("读取 {path:?} 失败: {e}"))?;
    let new = edit(text.trim_start_matches('\u{feff}'))?;
    std::fs::write(&path, new).map_err(|e| format!("写入 {path:?} 失败: {e}"))
}

/// 去掉 stdin/stdout/stderr 句柄的继承标志。PIMELauncher 以可继承方式创建这些管道,
/// 而 std 起子进程时 bInheritHandles=TRUE,`Stdio::null()` 挡不住其他可继承句柄:
/// 菜单打开的记事本会持有 stdout 写端,server.exe 崩溃后 Launcher 读不到 EOF、不重启后端。
#[cfg(windows)]
fn disinherit_std_handles() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(which: u32) -> isize;
        fn SetHandleInformation(handle: isize, mask: u32, flags: u32) -> i32;
    }
    const HANDLE_FLAG_INHERIT: u32 = 1;
    const INVALID_HANDLE_VALUE: isize = -1;
    for (which, name) in [(-10i32 as u32, "stdin"), (-11i32 as u32, "stdout"), (-12i32 as u32, "stderr")] {
        // SAFETY: 两个 Win32 函数只读写句柄属性,参数均为值类型。
        let h = unsafe { GetStdHandle(which) };
        if h == 0 || h == INVALID_HANDLE_VALUE {
            continue;
        }
        if unsafe { SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0) } == 0 {
            log::line(&format!("取消 {name} 句柄继承失败: {}", io::Error::last_os_error()));
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, dir] = args.as_slice() {
        let edit = match flag.as_str() {
            "--register" => register::add_backend,
            "--unregister" => register::remove_backend,
            _ => return eprintln!("未知参数: {flag}"),
        };
        if let Err(e) = edit_backends(dir, edit) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut sessions: HashMap<String, session::Session> = HashMap::new();
    log::init();
    inputlog::init();
    log::install_panic_hook();
    #[cfg(windows)]
    disinherit_std_handles();
    let ime = ime::Ime::load();
    log::line(&format!(
        "启动 pyime-server {} args={:?} exe={:?}",
        env!("CARGO_PKG_VERSION"),
        args,
        std::env::current_exe().ok()
    ));

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                log::line(&format!("读取 stdin 失败,退出: {e}"));
                break;
            }
        };
        if line.trim().is_empty() {
            continue; // PIME 会在请求之间多发空行,直接跳过,不写日志
        }
        log::line(&format!("> {line}"));
        let Some((id, req)) = protocol::parse_request(&line) else {
            log::line("忽略:请求格式不合法");
            eprintln!("忽略无法解析的请求: {line}");
            continue;
        };
        // close 无需回复,只清理该连接的状态
        if req["method"] == "close" {
            sessions.remove(id);
            log::line(&format!("close: 清理连接 {id},剩余 {} 个", sessions.len()));
            continue;
        }
        let sess = sessions.entry(id.to_string()).or_insert_with(|| session::Session::new(ime.clone()));
        let t = std::time::Instant::now();
        let resp = sess.handle(&req);
        let out = protocol::format_response(id, &resp);
        log::line(&format!("< {out}"));
        log::line(&format!("  状态: {} 耗时 {:?}", sess.debug_state(), t.elapsed()));
        let _ = writeln!(stdout, "{out}");
        let _ = stdout.flush();
    }
    log::line("stdin 已关闭,退出");
}
