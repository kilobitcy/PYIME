//! 探针:模拟 TSF 客户端连 pyime-server 的管道,发会话与按键并打印响应。仅用于实机验证。
//! 用法:pyime-probe <按键序列,如 ni1> [停留秒数,默认 5]

use pyime_protocol::{pipe_name, read_frame, write_frame, ClientMessage, ServerMessage};
use serde_json::json;
use std::fs::OpenOptions;
use std::time::Duration;

fn vk(c: char) -> Option<i64> {
    match c {
        'a'..='z' => Some(c.to_ascii_uppercase() as i64),
        '0'..='9' => Some(c as i64),
        ' ' => Some(32),
        '<' => Some(27),
        '-' => Some(189),
        '=' => Some(187),
        _ => None,
    }
}

#[cfg(windows)]
fn user() -> String {
    pyime_protocol::current_user_sid().unwrap_or_default()
}

#[cfg(not(windows))]
fn user() -> String {
    String::new()
}

fn main() -> std::io::Result<()> {
    let keys = std::env::args().nth(1).unwrap_or_else(|| "ni".into());
    let hold: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let name = pipe_name(&user());
    let mut pipe = OpenOptions::new().read(true).write(true).open(&name)?;
    write_frame(&mut pipe, &ClientMessage::Open { sid: 1, host: "probe.exe".into() })?;
    write_frame(&mut pipe, &ClientMessage::Rect { sid: 1, left: 400, top: 300, right: 410, bottom: 322 })?;
    for c in keys.chars() {
        let Some(code) = vk(c) else {
            eprintln!("忽略按键 {c:?}");
            continue;
        };
        let json = json!({"method": "onKeyDown", "seqNum": 1, "keyCode": code});
        write_frame(&mut pipe, &ClientMessage::Req { sid: 1, json })?;
        let ServerMessage::Resp { json, .. } = read_frame(&mut pipe)?;
        println!("{c} -> {json}");
    }
    std::thread::sleep(Duration::from_secs(hold));
    write_frame(&mut pipe, &ClientMessage::Close { sid: 1 })?;
    Ok(())
}
