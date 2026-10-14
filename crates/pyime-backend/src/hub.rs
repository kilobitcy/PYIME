//! 多会话中枢:按会话号持有 Session,供独立 server 把管道请求分发到各自的组词状态。

use crate::ime::Ime;
use crate::session::Session;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Hub {
    ime: Rc<Ime>,
    sessions: HashMap<u64, Session>,
}

impl Hub {
    pub fn new(ime: Rc<Ime>) -> Hub {
        Hub { ime, sessions: HashMap::new() }
    }

    pub fn open(&mut self, sid: u64) {
        self.sessions.insert(sid, Session::new(self.ime.clone()));
    }

    pub fn close(&mut self, sid: u64) {
        self.sessions.remove(&sid);
    }

    pub fn handle(&mut self, sid: u64, req: &Value) -> Value {
        let ime = &self.ime;
        self.sessions.entry(sid).or_insert_with(|| Session::new(ime.clone())).handle(req)
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyime_engine::dict::{Dict, DictBuilder};
    use pyime_engine::engine::Engine;
    use pyime_engine::fuzzy::FuzzyConfig;
    use serde_json::json;

    const N: i64 = 78;
    const I: i64 = 73;
    const W: i64 = 87;
    const O: i64 = 79;
    const VK_ESCAPE: i64 = 27;
    const KEY_1: i64 = 49;

    fn hub() -> Hub {
        let mut b = DictBuilder::default();
        for (i, w) in "你泥尼拟逆腻匿溺".chars().enumerate() {
            b.add("ni", &w.to_string(), 9000 - i as u32 * 100);
        }
        b.add("wo", "我", 8000);
        let engine = Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default());
        Hub::new(Ime::with_engine(engine))
    }

    fn key(h: &mut Hub, sid: u64, method: &str, code: i64) -> Value {
        h.handle(sid, &json!({"method": method, "seqNum": 1, "keyCode": code}))
    }

    #[test]
    fn 按需创建并可关闭会话() {
        let mut h = hub();
        assert_eq!(h.session_count(), 0);
        h.open(1);
        assert_eq!(h.session_count(), 1);
        key(&mut h, 2, "onKeyDown", N); // 未 open 的会话号自动创建
        assert_eq!(h.session_count(), 2);
        h.close(1);
        h.close(99); // 关闭不存在的会话不报错
        assert_eq!(h.session_count(), 1);
    }

    #[test]
    fn 会话之间组词状态互不干扰() {
        let mut h = hub();
        key(&mut h, 1, "onKeyDown", N);
        key(&mut h, 2, "onKeyDown", W);
        key(&mut h, 1, "onKeyDown", I);
        key(&mut h, 2, "onKeyDown", O);
        assert_eq!(key(&mut h, 1, "onKeyDown", KEY_1)["commitString"], "你");
        assert_eq!(key(&mut h, 2, "onKeyDown", KEY_1)["commitString"], "我");
    }

    #[test]
    fn esc清空组字且保留英文状态() {
        let mut h = hub();
        // 单击 Shift 切到英文:字母放行
        key(&mut h, 1, "filterKeyDown", 16);
        key(&mut h, 1, "filterKeyUp", 16);
        assert_eq!(key(&mut h, 1, "filterKeyDown", N)["return"], false);
        key(&mut h, 1, "onKeyDown", VK_ESCAPE);
        assert_eq!(key(&mut h, 1, "filterKeyDown", N)["return"], false);
    }

    #[test]
    fn esc清空中文组字() {
        let mut h = hub();
        key(&mut h, 1, "onKeyDown", N);
        key(&mut h, 1, "onKeyDown", I);
        let r = key(&mut h, 1, "onKeyDown", VK_ESCAPE);
        assert_eq!(r["compositionString"], "");
    }
}
