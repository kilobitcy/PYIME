//! 本地调试日志,仅写本机 `%LOCALAPPDATA%\PYIME\log.txt`,不上传。
//! 0.x 阶段默认开启(为了让用户直接贴回);在同目录建空文件 `log.disabled` 即可关闭。
//! 记录请求/响应原文、会话状态、panic 与启动信息,便于把真实按键转成回归用例。
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 5 * 1024 * 1024;
static FILE: OnceLock<Mutex<File>> = OnceLock::new();

fn dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("PYIME"))
}

/// 打开日志(幂等)。被关闭或无法创建时静默返回,不影响输入。
pub fn init() {
    let Some(dir) = dir() else { return };
    if dir.join("log.disabled").exists() || fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("log.txt");
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(&path, dir.join("log.old.txt"));
    }
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = FILE.set(Mutex::new(f));
    }
}

/// 写一行,带毫秒时间戳与进程号。
pub fn line(text: &str) {
    let Some(file) = FILE.get() else { return };
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(f, "{ms} [{}] {text}", std::process::id());
    }
}

/// 把 panic 信息(含位置)写进日志,再交给默认处理。
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        line(&format!("PANIC: {info}"));
        default(info);
    }));
}
