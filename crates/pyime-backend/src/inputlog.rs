//! 结构化输入日志:每个事件(选词、撤回、删除学习、放弃输入、组字中删改)一行 JSON,带时间戳与版本号,写 `%LOCALAPPDATA%\PYIME\input-log.jsonl`,仅本机。
//! 供 `pyime-replay` 离线回放,量化排序改动;与 `log.disabled` 共用同一关闭开关。
use crate::log;
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// 每条记录保存的候选前几名。
pub(crate) const TOP: usize = 5;
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

/// 写一行:补上时间戳与后端版本号。测试下同时存入本线程的捕获列表,供断言。
fn write(mut line: Value) {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    line["ts"] = json!(ms);
    line["v"] = json!(env!("CARGO_PKG_VERSION"));
    #[cfg(test)]
    CAPTURED.with(|c| c.borrow_mut().push(line.clone()));
    let Some(file) = FILE.get() else { return };
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(f, "{line}");
    }
}

/// 一次选词:`input` 为选词时的完整拼音串,`typed` 为本次消耗的部分,`index` 为选中项在全部候选中的下标。
pub fn select(input: &str, context: &str, typed: &str, text: &str, index: usize, top: &[&str]) {
    let top: Vec<&str> = top.iter().take(TOP).copied().collect();
    write(json!({ "input": input, "context": context, "typed": typed, "text": text, "index": index, "top": top }));
}

/// 撤销一次组词的学习:`items` 为 (typed, text, pairs, prev)。
pub fn retract(items: &[(String, String, u16, String)]) {
    write(json!({ "retract": items }));
}

/// 用户删除了某候选的个人学习:`input` 为当时的拼音串,`noop` 表示删除后候选顺序没有变化。
pub fn forget(input: &str, text: &str, noop: bool) {
    write(json!({ "forget": text, "input": input, "noop": noop }));
}

/// 放弃一次组字(Esc,或退格删光且无上屏):`input` 为本次组字中最长的拼音,`top` 为其当时的候选前几名。
pub fn cancel(input: &str, top: &[String]) {
    write(json!({ "cancel": input, "top": top }));
}

/// 组字中拼音由增长转为删改(本轮第一次退格):`input` 为删前的拼音,`top` 为其当时的候选前几名。
pub fn edit(input: &str, top: &[String]) {
    write(json!({ "edit": input, "top": top }));
}

/// 回车原样上屏拼音串(本次组字未分段选过词):`counted` 表示是否计入英文回车学习。
pub fn enter_raw(input: &str, counted: bool) {
    write(json!({ "enter_raw": input, "counted": counted }));
}

#[cfg(test)]
thread_local! {
    static CAPTURED: std::cell::RefCell<Vec<Value>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// 取出并清空本线程已写的日志行(仅测试)。
#[cfg(test)]
pub fn take_captured() -> Vec<Value> {
    CAPTURED.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 每行带版本与时间戳() {
        take_captured();
        select("nihao", "", "nihao", "你好", 0, &["你好", "你", "泥", "尼", "拟", "逆"]);
        let lines = take_captured();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["v"], env!("CARGO_PKG_VERSION"));
        assert!(lines[0]["ts"].as_u64().unwrap() > 0);
        assert_eq!(lines[0]["text"], "你好");
        assert_eq!(lines[0]["top"].as_array().unwrap().len(), TOP);
    }

    #[test]
    fn 放弃输入记拼音与候选() {
        take_captured();
        cancel("chelue", &["车".to_string(), "撤".to_string()]);
        let lines = take_captured();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["cancel"], "chelue");
        assert_eq!(lines[0]["top"], json!(["车", "撤"]));
        assert_eq!(lines[0]["v"], env!("CARGO_PKG_VERSION"));
    }
}
