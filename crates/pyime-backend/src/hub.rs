//! 管道连接与会话的分发:所有请求经 mpsc 送到唯一的工作线程,它持有 `Rc<Ime>` 与每个连接的
//! `Session`(session/ime 保持单线程)。连接断开即结束该会话,等价于原 PIME 的 `close`。
use crate::ime::Ime;
use crate::log;
use crate::session::Session;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

/// 单条请求处理超过这么久即视为卡死,看门狗结束 worker 进程,由监护进程重启。
pub const WATCHDOG_LIMIT_MS: u64 = 5000;

pub enum Msg {
    /// 一条请求(管道消息原文)及回传回复的通道。
    Request { conn: u64, body: Vec<u8>, reply: Sender<Vec<u8>> },
    /// 连接断开。
    Closed { conn: u64 },
}

struct Hub {
    ime: Rc<Ime>,
    sessions: HashMap<u64, Session>,
}

/// 工作线程"当前请求开始时刻"(UNIX 毫秒,0 = 空闲),看门狗线程据此判断是否卡死。
#[derive(Default)]
pub struct Busy(AtomicU64);

impl Hub {
    fn new(ime: Rc<Ime>) -> Hub {
        Hub { ime, sessions: HashMap::new() }
    }

    /// 处理一条请求;不是 UTF-8 JSON 时返回 None(不回复)。日志格式同 stdin 模式,前缀带连接号。
    fn request(&mut self, conn: u64, body: &[u8]) -> Option<Vec<u8>> {
        let Ok(text) = std::str::from_utf8(body) else {
            log::line(&format!("忽略 [{conn}]:请求不是 UTF-8"));
            return None;
        };
        let text = text.trim();
        log::line(&format!("> [{conn}] {text}"));
        let Ok(req) = serde_json::from_str::<Value>(text) else {
            log::line("忽略:请求格式不合法");
            return None;
        };
        let ime = &self.ime;
        let sess = self.sessions.entry(conn).or_insert_with(|| Session::new(ime.clone()));
        let t = std::time::Instant::now();
        let out = sess.handle(&req).to_string();
        log::line(&format!("< [{conn}] {out}"));
        log::line(&format!("  状态: {} 耗时 {:?}", sess.debug_state(), t.elapsed()));
        Some(out.into_bytes())
    }

    fn close(&mut self, conn: u64) {
        if self.sessions.remove(&conn).is_some() {
            log::line(&format!("连接 {conn} 断开,剩余 {} 个会话", self.sessions.len()));
        }
    }

    #[cfg(test)]
    fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

/// 工作线程主循环,直到所有发送端关闭。处理每条请求期间在 `busy` 上记开始时刻供看门狗检查。
pub fn run(rx: Receiver<Msg>, ime: Rc<Ime>, busy: &Busy) {
    let mut hub = Hub::new(ime);
    for msg in rx {
        match msg {
            Msg::Request { conn, body, reply } => {
                busy.begin();
                let out = hub.request(conn, &body);
                busy.end();
                // 无法解析时丢弃 reply,I/O 线程收不到回复即断开该连接
                if let Some(out) = out {
                    let _ = reply.send(out);
                }
            }
            Msg::Closed { conn } => hub.close(conn),
        }
    }
}

impl Busy {
    pub fn begin(&self) {
        self.0.store(now_ms().max(1), Ordering::SeqCst);
    }

    pub fn end(&self) {
        self.0.store(0, Ordering::SeqCst);
    }

    /// 正在处理的请求已超过 `limit_ms` 毫秒。
    pub fn stuck(&self, now: u64, limit_ms: u64) -> bool {
        let start = self.0.load(Ordering::SeqCst);
        start != 0 && now.saturating_sub(start) > limit_ms
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyime_engine::dict::{Dict, DictBuilder};
    use pyime_engine::engine::Engine;
    use pyime_engine::fuzzy::FuzzyConfig;
    use serde_json::json;
    use std::sync::{mpsc, Arc};

    const N: i64 = 78;
    const I: i64 = 73;

    fn engine() -> Engine {
        let mut b = DictBuilder::default();
        b.add("ni", "你", 9000);
        Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default())
    }

    fn req(seq: u64, code: i64) -> Vec<u8> {
        json!({"method": "onKeyDown", "seqNum": seq, "keyCode": code}).to_string().into_bytes()
    }

    fn parse(out: &[u8]) -> Value {
        serde_json::from_slice(out).unwrap()
    }

    #[test]
    fn 不同连接各自独立会话() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        assert_eq!(parse(&hub.request(1, &req(1, N)).unwrap())["compositionString"], "n");
        assert_eq!(parse(&hub.request(2, &req(1, N)).unwrap())["compositionString"], "n");
        assert_eq!(parse(&hub.request(1, &req(2, I)).unwrap())["compositionString"], "ni");
        assert_eq!(hub.session_count(), 2);
    }

    #[test]
    fn 断开即结束会话_同编号重连从空开始() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        hub.request(1, &req(1, N)).unwrap();
        hub.close(1);
        assert_eq!(hub.session_count(), 0);
        assert_eq!(parse(&hub.request(1, &req(2, N)).unwrap())["compositionString"], "n");
    }

    #[test]
    fn 回显序号_结尾换行可解析() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        let mut body = req(42, N);
        body.push(b'\n');
        let r = parse(&hub.request(1, &body).unwrap());
        assert_eq!(r["seqNum"], 42);
        assert_eq!(r["success"], true);
    }

    #[test]
    fn 非法请求不回复() {
        let mut hub = Hub::new(Ime::with_engine(engine()));
        assert!(hub.request(1, b"not json").is_none());
        assert!(hub.request(1, &[0xff, 0xfe]).is_none());
    }

    #[test]
    fn 工作线程按顺序回复_非法请求丢弃回复通道() {
        let (tx, rx) = mpsc::channel();
        let busy = Arc::new(Busy::default());
        let b = busy.clone();
        let worker = std::thread::spawn(move || run(rx, Ime::with_engine(engine()), &b));
        let (rtx, rrx) = mpsc::channel();
        for seq in 1..=3 {
            tx.send(Msg::Request { conn: 1, body: req(seq, N), reply: rtx.clone() }).unwrap();
        }
        let seqs: Vec<u64> = (0..3).map(|_| parse(&rrx.recv().unwrap())["seqNum"].as_u64().unwrap()).collect();
        assert_eq!(seqs, [1, 2, 3]);
        let (btx, brx) = mpsc::channel();
        tx.send(Msg::Request { conn: 1, body: b"xx".to_vec(), reply: btx }).unwrap();
        assert!(brx.recv().is_err());
        tx.send(Msg::Closed { conn: 1 }).unwrap();
        drop(tx);
        worker.join().unwrap();
        assert!(!busy.stuck(now_ms() + 60_000, WATCHDOG_LIMIT_MS));
    }

    #[test]
    fn 看门狗只在处理中且超时才触发() {
        let busy = Busy::default();
        assert!(!busy.stuck(now_ms() + 60_000, WATCHDOG_LIMIT_MS));
        busy.begin();
        let now = now_ms();
        assert!(!busy.stuck(now + 1000, WATCHDOG_LIMIT_MS));
        assert!(busy.stuck(now + 6000, WATCHDOG_LIMIT_MS));
        busy.end();
        assert!(!busy.stuck(now + 6000, WATCHDOG_LIMIT_MS));
    }
}
