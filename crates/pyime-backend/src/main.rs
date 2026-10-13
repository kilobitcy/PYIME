mod log;
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
    let mut log = log::Log::open();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        log.line(&format!("> {line}"));
        let Some((id, req)) = protocol::parse_request(&line) else {
            eprintln!("忽略无法解析的请求: {line}");
            continue;
        };
        // close 无需回复,只清理该连接的状态
        if req["method"] == "close" {
            sessions.remove(id);
            continue;
        }
        let resp = sessions.entry(id.to_string()).or_default().handle(&req);
        let out = protocol::format_response(id, &resp);
        log.line(&format!("< {out}"));
        let _ = writeln!(stdout, "{out}");
        let _ = stdout.flush();
    }
}
