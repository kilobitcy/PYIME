//! 单个前端连接的会话状态。当前为验证用的最小实现:固定候选,验证 PIME 链路能出字。
use serde_json::{json, Value};

const VK_BACK: i64 = 8;
const VK_RETURN: i64 = 13;
const VK_ESCAPE: i64 = 27;
const VK_SPACE: i64 = 32;
const VK_UP: i64 = 38;
const VK_DOWN: i64 = 40;
const CANDIDATES: [&str; 3] = ["测试", "候选", "拼音"];

/// 数字键 1-9(主键盘或小键盘)对应的候选下标。
fn digit_index(key: i64) -> Option<usize> {
    match key {
        49..=57 => Some((key - 49) as usize),
        97..=105 => Some((key - 97) as usize),
        _ => None,
    }
}

#[derive(Default)]
pub struct Session {
    composition: String,
    /// 当前高亮的候选下标。
    cursor: usize,
}

impl Session {
    /// 该键是否由输入法接管(有拼音串时,空格/回车/Esc/退格也接管)。
    fn wants_key(&self, key: i64) -> bool {
        let letter = (65..=90).contains(&key);
        let editing = matches!(key, VK_BACK | VK_RETURN | VK_ESCAPE | VK_SPACE | VK_UP | VK_DOWN)
            || digit_index(key).is_some_and(|i| i < CANDIDATES.len());
        letter || (editing && !self.composition.is_empty())
    }

    /// 供日志使用的当前状态摘要。
    pub fn debug_state(&self) -> String {
        format!("composition={:?} cursor={}", self.composition, self.cursor)
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
                resp["commitString"] = json!(CANDIDATES[self.cursor]);
                self.composition.clear();
            }
            VK_RETURN => {
                resp["commitString"] = json!(self.composition);
                self.composition.clear();
            }
            VK_ESCAPE => self.composition.clear(),
            VK_UP => self.cursor = self.cursor.saturating_sub(1),
            VK_DOWN => self.cursor = (self.cursor + 1).min(CANDIDATES.len() - 1),
            _ if digit_index(key).is_some() => {
                resp["commitString"] = json!(CANDIDATES[digit_index(key).unwrap()]);
                self.composition.clear();
            }
            VK_BACK => {
                self.composition.pop();
            }
            _ => self.composition.push((key as u8 as char).to_ascii_lowercase()),
        }
        // 输入内容变化、上屏或清空后,高亮回到第一个;上下键只移动高亮
        if !matches!(key, VK_UP | VK_DOWN) {
            self.cursor = 0;
        }
        resp["compositionString"] = json!(self.composition);
        resp["compositionCursor"] = json!(self.composition.chars().count());
        resp["showCandidates"] = json!(!self.composition.is_empty());
        if !self.composition.is_empty() {
            resp["candidateList"] = json!(CANDIDATES);
            resp["candidateCursor"] = json!(self.cursor);
            resp["customizeUI"] = json!({ "candUseCursor": true });
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
    fn 数字键选择对应候选上屏() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        assert_eq!(key(&mut s, "filterKeyDown", 50)["return"], true);
        let r = key(&mut s, "onKeyDown", 50);
        assert_eq!(r["commitString"], "候选");
        assert_eq!(r["compositionString"], "");
    }

    #[test]
    fn 超出候选数的数字键不接管() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        assert_eq!(key(&mut s, "filterKeyDown", 57)["return"], false);
    }

    #[test]
    fn 回车上屏拼音串() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        key(&mut s, "onKeyDown", 73);
        let r = key(&mut s, "onKeyDown", VK_RETURN);
        assert_eq!(r["commitString"], "ni");
        assert_eq!(r["compositionString"], "");
    }

    #[test]
    fn 下键移动高亮且不上屏() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        assert_eq!(key(&mut s, "filterKeyDown", VK_DOWN)["return"], true);
        let r = key(&mut s, "onKeyDown", VK_DOWN);
        assert_eq!(r["candidateCursor"], 1);
        assert_eq!(r["compositionString"], "n");
        assert!(r.get("commitString").is_none());
    }

    #[test]
    fn 空格上屏当前高亮候选() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        key(&mut s, "onKeyDown", VK_DOWN);
        assert_eq!(key(&mut s, "onKeyDown", VK_SPACE)["commitString"], "候选");
    }

    #[test]
    fn 上键不越界且输入后高亮归零() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        assert_eq!(key(&mut s, "onKeyDown", VK_UP)["candidateCursor"], 0);
        key(&mut s, "onKeyDown", VK_DOWN);
        assert_eq!(key(&mut s, "onKeyDown", 73)["candidateCursor"], 0);
    }

    #[test]
    fn 无拼音串时方向键不接管() {
        let mut s = Session::default();
        assert_eq!(key(&mut s, "filterKeyDown", VK_DOWN)["return"], false);
    }

    #[test]
    fn 退格删一个字符() {
        let mut s = Session::default();
        key(&mut s, "onKeyDown", 78);
        key(&mut s, "onKeyDown", 73);
        assert_eq!(key(&mut s, "onKeyDown", VK_BACK)["compositionString"], "n");
    }
}
