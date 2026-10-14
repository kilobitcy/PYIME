mod ime;
mod inputlog;
mod log;
mod punct;
mod protocol;
mod register;
mod session;

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

/// 安装器入口:`--register <PIME目录>` / `--unregister <PIME目录>`,修改其中的 backends.json。
fn edit_backends(dir: &str, edit: fn(&str) -> Result<String, String>) -> Result<(), String> {
    let path = std::path::Path::new(dir).join("backends.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("读取 {path:?} 失败: {e}"))?;
    let new = edit(text.trim_start_matches('\u{feff}'))?;
    std::fs::write(&path, new).map_err(|e| format!("写入 {path:?} 失败: {e}"))
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
    let ime = ime::Ime::load();
    log::line(&format!(
        "启动 pyime-backend {} args={:?} exe={:?}",
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
