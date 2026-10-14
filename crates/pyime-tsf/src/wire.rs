//! 与 Windows 无关的线路逻辑:请求构造、响应解析、限流。Linux 上可单测。

use serde_json::{json, Value};

/// 构造 `Session::handle` 能读的 PIME 风格按键请求。`key_states` 不足 256 项时补零。
pub fn key_request(method: &str, vk: u32, char_code: u32, key_states: &[u8]) -> Value {
    let mut states = vec![0u8; 256];
    let n = key_states.len().min(256);
    states[..n].copy_from_slice(&key_states[..n]);
    json!({ "method": method, "keyCode": vk, "charCode": char_code, "keyStates": states })
}

/// 响应里 TSF 关心的部分。字段缺省 = 不变(`None`);`composition == Some("")` = 清空组字串。
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Action {
    pub eat: bool,
    pub commit: Option<String>,
    pub composition: Option<String>,
    /// 组字串内光标位置,单位为字符(非 UTF-16)。
    pub cursor: Option<usize>,
}

impl Action {
    pub fn from_resp(resp: &Value) -> Action {
        Action {
            eat: resp["return"].as_bool().unwrap_or(false),
            commit: resp["commitString"].as_str().filter(|s| !s.is_empty()).map(String::from),
            composition: resp["compositionString"].as_str().map(String::from),
            cursor: resp["compositionCursor"].as_u64().map(|n| n as usize),
        }
    }

    pub fn needs_edit(&self) -> bool {
        self.commit.is_some() || self.composition.is_some()
    }
}

/// 前 `chars` 个字符占多少个 UTF-16 单元(超出取整串)。
pub fn utf16_offset(s: &str, chars: usize) -> usize {
    s.chars().take(chars).map(char::len_utf16).sum()
}

/// `Session` 只在 Shift 键上使用 keyUp(单击 Shift 切换中英文),其余键不必付出管道往返。
pub fn wants_key_up(vk: u32) -> bool {
    matches!(vk, 16 | 160 | 161)
}

/// 最小间隔限流:用于限制拉起 server 的频率。
pub struct Throttle {
    interval_ms: u64,
    last: Option<u64>,
}

impl Throttle {
    pub fn new(interval_ms: u64) -> Throttle {
        Throttle { interval_ms, last: None }
    }

    pub fn allow(&mut self, now_ms: u64) -> bool {
        match self.last {
            Some(t) if now_ms.saturating_sub(t) < self.interval_ms => false,
            _ => {
                self.last = Some(now_ms);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 请求含方法键码字符与256项键态() {
        let mut states = [0u8; 256];
        states[16] = 0x80;
        let r = key_request("filterKeyDown", 0x4E, 'n' as u32, &states);
        assert_eq!(r["method"], "filterKeyDown");
        assert_eq!(r["keyCode"], 0x4E);
        assert_eq!(r["charCode"], 110);
        assert_eq!(r["keyStates"].as_array().unwrap().len(), 256);
        assert_eq!(r["keyStates"][16], 0x80);
        assert_eq!(r["keyStates"][17], 0);
    }

    #[test]
    fn 键态不足256项时补零() {
        let r = key_request("onKeyUp", 16, 0, &[1, 2, 3]);
        let a = r["keyStates"].as_array().unwrap();
        assert_eq!(a.len(), 256);
        assert_eq!(a[2], 3);
        assert_eq!(a[3], 0);
    }

    #[test]
    fn 解析上屏与组字() {
        let a = Action::from_resp(&json!({"return": true, "commitString": "你", "compositionString": "", "compositionCursor": 0}));
        assert_eq!(a, Action { eat: true, commit: Some("你".into()), composition: Some("".into()), cursor: Some(0) });
        assert!(a.needs_edit());
    }

    #[test]
    fn 缺省字段为空_空上屏串视为无() {
        let a = Action::from_resp(&json!({"return": false, "commitString": ""}));
        assert_eq!(a, Action::default());
        assert!(!a.needs_edit());
        assert_eq!(Action::from_resp(&json!({})), Action::default());
    }

    #[test]
    fn utf16偏移按字符数换算() {
        assert_eq!(utf16_offset("ni好", 3), 3);
        assert_eq!(utf16_offset("a𠀀b", 2), 3);
        assert_eq!(utf16_offset("ab", 9), 2);
        assert_eq!(utf16_offset("ab", 0), 0);
    }

    #[test]
    fn 只有shift需要发keyup() {
        for vk in [16, 160, 161] {
            assert!(wants_key_up(vk));
        }
        assert!(!wants_key_up(0x41));
        assert!(!wants_key_up(17));
    }

    #[test]
    fn 限流首次放行_间隔内拒绝_到点放行() {
        let mut t = Throttle::new(2000);
        assert!(t.allow(5000));
        assert!(!t.allow(6999));
        assert!(t.allow(7000));
        assert!(!t.allow(8999));
    }
}
