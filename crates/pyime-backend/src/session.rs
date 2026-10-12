//! 单个前端连接的会话状态。当前为验证用的最小实现:固定候选,验证 PIME 链路能出字。
use serde_json::{json, Value};

const VK_BACK: i64 = 8;
const VK_RETURN: i64 = 13;
const VK_ESCAPE: i64 = 27;
const VK_SPACE: i64 = 32;
const CANDIDATES: [&str; 3] = ["测试", "候选", "拼音"];

#[derive(Default)]
pub struct Session {
    composition: String,
}

impl Session {
    /// 该键是否由输入法接管(有拼音串时,空格/回车/Esc/退格也接管)。
    fn wants_key(&self, key: i64) -> bool {
        let letter = (65..=90).contains(&key);
        let editing = matches!(key, VK_BACK | VK_RETURN | VK_ESCAPE | VK_SPACE);
        letter || (editing && !self.composition.is_empty())
    }

    pub fn handle(&mut self, req: &Value) -> Value {
        let key = req["keyCode"].as_i64().unwrap_or(0);
        let mut resp = json!({ "success": true, "seqNum": req["seqNum"] });
        match req["method"].as_str().unwrap_or("") {
            "filterKeyDown" => resp["return"] = json!(self.wants_key(key)),
            "filterKeyUp" => resp["return"] = json!(false),
            "onKeyDown" => {
                resp["return"] = json!(self.wants_key(key));
                self.on_key_down(key, &mut resp);
            }
            _ => {}
        }
        resp
    }

    fn on_key_down(&mut self, key: i64, resp: &mut Value) {
        if !self.wants_key(key) {
            return;
        }
        match key {
            VK_SPACE => {
                resp["commitString"] = json!(CANDIDATES[0]);
                self.composition.clear();
            }
            VK_RETURN => {
                resp["commitString"] = json!(self.composition);
                self.composition.clear();
            }
            VK_ESCAPE => self.composition.clear(),
            VK_BACK => {
                self.composition.pop();
            }
            _ => self.composition.push((key as u8 as char).to_ascii_lowercase()),
        }
        resp["compositionString"] = json!(self.composition);
        resp["compositionCursor"] = json!(self.composition.chars().count());
        resp["showCandidates"] = json!(!self.composition.is_empty());
        if !self.composition.is_empty() {
            resp["candidateList"] = json!(CANDIDATES);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &mut Session, method: &str, code: i64) -> Value {
        s.handle(&json!({"method": method, "seqNum": 7, "keyCode": code}))
    }

    #[test]
    fn 字母键被接管并进入拼音串() {
        let mut s = Session::default();
        assert_eq!(key(&mut s, "filterKeyDown", 65)["return"], true);
        let r = key(&mut s, "onKeyDown", 65);
        assert_eq!(r["compositionString"], "a");
        assert_eq!(r["seqNum"], 7);
        assert_eq!(r["candidateList"][0], "测试");
    }

    #[test]
    fn 无拼音串时空格不接管() {
        let mut s = Session::default();
        assert_eq!(key(&mut s, "filterKeyDown", VK_SPACE)["return"], false);
    }

    #[test]
    fn 空格上屏首选并清空() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        let r = key(&mut s, "onKeyDown", VK_SPACE);
        assert_eq!(r["commitString"], "测试");
        assert_eq!(r["compositionString"], "");
        assert_eq!(r["showCandidates"], false);
    }

    #[test]
    fn 退格删一个字符() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        key(&mut s, "onKeyDown", 73);
        assert_eq!(key(&mut s, "onKeyDown", VK_BACK)["compositionString"], "n");
    }
}
