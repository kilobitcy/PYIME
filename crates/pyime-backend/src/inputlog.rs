//! 结构化输入日志:每次选词一行 JSON,写 `%LOCALAPPDATA%\PYIME\input-log.jsonl`,仅本机。
//! 供 `pyime-replay` 离线回放,量化排序改动;与 `log.disabled` 共用同一关闭开关。
use crate::log;
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// 每条记录保存的候选前几名。
const TOP: usize = 5;
static FILE: OnceLock<Mutex<File>> = OnceLock::new();

/// 打开日志(幂等)。被关闭或无法创建时静默返回。
pub fn init() {
    let Some(dir) = log::dir() else { return };
    if dir.join("log.disabled").exists() || fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("input-log.jsonl");
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(&path, dir.join("input-log.old.jsonl"));
    }
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = FILE.set(Mutex::new(f));
    }
}

/// 一次选词:`input` 为选词时的完整拼音串,`typed` 为本次消耗的部分,`index` 为选中项在全部候选中的下标。
pub fn select(input: &str, context: &str, typed: &str, text: &str, index: usize, top: &[&str]) {
    let Some(file) = FILE.get() else { return };
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let top: Vec<&str> = top.iter().take(TOP).copied().collect();
    let line = json!({ "ts": ms, "input": input, "context": context, "typed": typed, "text": text, "index": index, "top": top });
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(f, "{line}");
    }
}

/// 撤销一次组词的学习:`items` 为 (typed, text, pairs, prev)。
pub fn retract(items: &[(String, String, u16, String)]) {
    let Some(file) = FILE.get() else { return };
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let line = json!({ "ts": ms, "retract": items });
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(f, "{line}");
    }
}
