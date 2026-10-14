//! 本机日志:追加到 `%LOCALAPPDATA%\PYIME\tsf.log`。任何失败静默忽略,不得影响宿主。
#![allow(dead_code)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("PYIME"))
}

/// 追加一行 `<秒级时间戳> [pid] msg`;`log.disabled` 存在则不写。
pub(crate) fn write(msg: &str) {
    let Some(dir) = dir() else { return };
    if dir.join("log.disabled").exists() {
        return;
    }
    let _ = fs::create_dir_all(&dir);
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(dir.join("tsf.log")) else {
        return;
    };
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let _ = writeln!(f, "{secs} [{}] {msg}", std::process::id());
}
