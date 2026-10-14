//! 分发核心:管道消息 → Hub → 响应与候选窗指令。纯逻辑,不依赖 Windows,Linux 可测。

use crate::ui::{ui_from_resp, Rect, UiCmd};
use pyime_backend::hub::Hub;
use pyime_protocol::{ClientMessage, ServerMessage};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};

pub struct Dispatcher {
    hub: Hub,
    rects: HashMap<u64, Rect>,
    /// 当前正显示候选窗的会话。
    shown_by: Option<u64>,
}

pub struct Outcome {
    pub reply: Option<ServerMessage>,
    pub ui: Option<UiCmd>,
}

pub struct Job {
    pub msg: ClientMessage,
    pub reply: Sender<ServerMessage>,
}

impl Dispatcher {
    pub fn new(hub: Hub) -> Dispatcher {
        Dispatcher { hub, rects: HashMap::new(), shown_by: None }
    }

    pub fn handle(&mut self, msg: ClientMessage) -> Outcome {
        match msg {
            ClientMessage::Open { sid, .. } => {
                self.hub.open(sid);
                Outcome { reply: None, ui: None }
            }
            ClientMessage::Req { sid, json } => {
                let resp = self.hub.handle(sid, &json);
                let ui = ui_from_resp(&resp, self.rects.get(&sid).copied());
                let ui = self.gate(sid, ui);
                Outcome { reply: Some(ServerMessage::Resp { sid, json: resp }), ui }
            }
            ClientMessage::Rect { sid, left, top, right, bottom } => {
                let rect = Rect { left, top, right, bottom };
                self.rects.insert(sid, rect);
                Outcome { reply: None, ui: self.gate(sid, Some(UiCmd::Move(rect))) }
            }
            ClientMessage::Close { sid } => {
                self.hub.close(sid);
                self.rects.remove(&sid);
                Outcome { reply: None, ui: self.gate(sid, Some(UiCmd::Hide)) }
            }
        }
    }

    /// 按候选窗归属过滤指令:Show 夺取所有权;Hide 仅所有者生效并释放所有权;Move 仅所有者生效。
    fn gate(&mut self, sid: u64, ui: Option<UiCmd>) -> Option<UiCmd> {
        match ui? {
            show @ UiCmd::Show(_) => {
                self.shown_by = Some(sid);
                Some(show)
            }
            UiCmd::Hide if self.shown_by == Some(sid) => {
                self.shown_by = None;
                Some(UiCmd::Hide)
            }
            mv @ UiCmd::Move(_) if self.shown_by == Some(sid) => Some(mv),
            _ => None,
        }
    }
}

pub fn run_pending(rx: &Receiver<Job>, d: &mut Dispatcher, mut on_ui: impl FnMut(UiCmd)) -> usize {
    let mut n = 0;
    while let Ok(job) = rx.try_recv() {
        n += 1;
        let out = d.handle(job.msg);
        if let Some(reply) = out.reply {
            let _ = job.reply.send(reply); // I/O 线程已退出时发送失败,忽略
        }
        if let Some(ui) = out.ui {
            on_ui(ui);
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyime_backend::ime::Ime;
    use pyime_engine::dict::{Dict, DictBuilder};
    use pyime_engine::engine::Engine;
    use pyime_engine::fuzzy::FuzzyConfig;
    use serde_json::{json, Value};
    use std::sync::mpsc::channel;

    const N: i64 = 78;
    const I: i64 = 73;
    const Q: i64 = 81;
    const VK_ESCAPE: i64 = 27;

    fn dispatcher() -> Dispatcher {
        let mut b = DictBuilder::default();
        for (i, w) in "你泥尼拟逆腻匿溺".chars().enumerate() {
            b.add("ni", &w.to_string(), 9000 - i as u32 * 100);
        }
        let engine = Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default());
        Dispatcher::new(Hub::new(Ime::with_engine(engine)))
    }

    fn req(sid: u64, code: i64) -> ClientMessage {
        ClientMessage::Req { sid, json: json!({"method": "onKeyDown", "seqNum": 1, "keyCode": code}) }
    }

    fn rect_msg(sid: u64, left: i32) -> ClientMessage {
        ClientMessage::Rect { sid, left, top: 2, right: left + 10, bottom: 22 }
    }

    fn resp_json(o: &Outcome) -> &Value {
        let Some(ServerMessage::Resp { json, .. }) = &o.reply else { panic!("应有响应") };
        json
    }

    #[test]
    fn open无回复无ui() {
        let mut d = dispatcher();
        let o = d.handle(ClientMessage::Open { sid: 1, host: "notepad.exe".into() });
        assert!(o.reply.is_none() && o.ui.is_none());
    }

    #[test]
    fn 按键产生响应与显示指令且用已知光标矩形() {
        let mut d = dispatcher();
        d.handle(rect_msg(1, 100));
        d.handle(req(1, N));
        let o = d.handle(req(1, I));
        assert_eq!(resp_json(&o)["compositionString"], "ni");
        let Some(UiCmd::Show(v)) = o.ui else { panic!("应为 Show") };
        assert_eq!(v.labels[0], "1 你");
        assert_eq!(v.rect.left, 100);
    }

    #[test]
    fn 只有所有者的rect才移动窗口() {
        let mut d = dispatcher();
        d.handle(req(1, N));
        d.handle(req(1, I)); // 会话 1 显示候选窗,成为所有者
        assert_eq!(d.handle(rect_msg(1, 50)).ui, Some(UiCmd::Move(Rect { left: 50, top: 2, right: 60, bottom: 22 })));
        assert_eq!(d.handle(rect_msg(2, 70)).ui, None);
    }

    #[test]
    fn 只有所有者的隐藏生效() {
        let mut d = dispatcher();
        d.handle(req(1, N));
        d.handle(req(1, I)); // 会话 1 显示候选窗
        // 会话 2 输入词典里没有的 q(无候选 → showCandidates=false),它不是所有者,指令被丢弃
        assert_eq!(d.handle(req(2, Q)).ui, None);
        assert_eq!(d.handle(req(2, VK_ESCAPE)).ui, None);
        assert_eq!(d.handle(req(1, VK_ESCAPE)).ui, Some(UiCmd::Hide));
        // 所有者已清空,再来一次隐藏不再产生指令
        assert_eq!(d.handle(req(1, VK_ESCAPE)).ui, None);
    }

    #[test]
    fn 另一会话显示后所有权转移() {
        let mut d = dispatcher();
        d.handle(req(1, N));
        d.handle(req(1, I));
        d.handle(req(2, N));
        let o = d.handle(req(2, I));
        assert!(matches!(o.ui, Some(UiCmd::Show(_))));
        assert_eq!(d.handle(req(1, VK_ESCAPE)).ui, None); // 旧会话收尾不关新窗口
        assert_eq!(d.handle(req(2, VK_ESCAPE)).ui, Some(UiCmd::Hide));
    }

    #[test]
    fn 关闭所有者会话隐藏_关闭其他会话不动() {
        let mut d = dispatcher();
        d.handle(req(1, N));
        d.handle(req(1, I));
        assert_eq!(d.handle(ClientMessage::Close { sid: 2 }).ui, None);
        assert_eq!(d.handle(ClientMessage::Close { sid: 1 }).ui, Some(UiCmd::Hide));
    }

    #[test]
    fn run_pending按序处理并回复() {
        let mut d = dispatcher();
        let (tx, rx) = channel::<Job>();
        let (rtx, rrx) = channel::<ServerMessage>();
        tx.send(Job { msg: req(1, N), reply: rtx.clone() }).unwrap();
        tx.send(Job { msg: req(1, I), reply: rtx.clone() }).unwrap();
        tx.send(Job { msg: ClientMessage::Close { sid: 1 }, reply: rtx }).unwrap();
        let mut cmds = Vec::new();
        assert_eq!(run_pending(&rx, &mut d, |c| cmds.push(c)), 3);
        assert_eq!(run_pending(&rx, &mut d, |_| panic!("队列已空")), 0);
        let replies: Vec<_> = rrx.try_iter().collect();
        assert_eq!(replies.len(), 2); // Close 无回复
        // 只输入 n 时 Session 已有候选(n 可补全为 ni),故 n、ni 各产生一次 Show
        assert!(matches!(cmds.as_slice(), [UiCmd::Show(_), UiCmd::Show(_), UiCmd::Hide]), "{:?}", cmds);
    }
}
