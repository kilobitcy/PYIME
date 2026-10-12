mod protocol;
mod session;

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut sessions: HashMap<String, session::Session> = HashMap::new();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
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
        let _ = writeln!(stdout, "{}", protocol::format_response(id, &resp));
        let _ = stdout.flush();
    }
}
