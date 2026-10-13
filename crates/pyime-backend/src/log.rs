//! 本地调试日志,默认关闭。存在 `%LOCALAPPDATA%\PYIME\log.enabled` 时才写 `log.txt`。
//! 只写在本机,不上传;记录请求与响应原文,便于把真实按键转成回归用例。
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub struct Log(Option<File>);

fn dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("PYIME"))
}

impl Log {
    pub fn open() -> Log {
        let file = dir().filter(|d| d.join("log.enabled").exists()).and_then(|d| {
            OpenOptions::new().create(true).append(true).open(d.join("log.txt")).ok()
        });
        Log(file)
    }

    pub fn line(&mut self, text: &str) {
        if let Some(f) = self.0.as_mut() {
            let _ = writeln!(f, "{text}");
        }
    }
}
