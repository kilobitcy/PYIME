//! 文本服务:把按键经管道交给 server,按响应维护组字串、上屏与光标矩形。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pyime_protocol::ClientMessage;
use windows::Win32::Foundation::{FALSE, LPARAM, TRUE, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::UI::TextServices::{
    ITfCompositionSink, ITfCompositionSink_Impl, ITfContext, ITfComposition, ITfKeyEventSink,
    ITfKeyEventSink_Impl, ITfKeystrokeMgr, ITfTextInputProcessor, ITfTextInputProcessor_Impl,
    ITfThreadMgr,
};
use windows::core::{implement, IUnknownImpl, Interface, Ref, Result, BOOL, GUID};

use super::composition::{self, Composition};
use super::keys;
use super::log;
use super::pipe::Client;
use crate::wire::{key_request, wants_key_up, Action};

const VK_ESCAPE: u32 = 27;

#[implement(ITfTextInputProcessor, ITfKeyEventSink, ITfCompositionSink)]
pub struct TextService {
    thread_mgr: RefCell<Option<ITfThreadMgr>>,
    client_id: Cell<u32>,
    client: RefCell<Option<Client>>,
    comp: Rc<Composition>,
    last_up: RefCell<Option<(u32, Action)>>,
}

impl TextService {
    pub fn new() -> Self {
        super::lock_module();
        Self {
            thread_mgr: RefCell::new(None),
            client_id: Cell::new(0),
            client: RefCell::new(None),
            comp: Rc::new(Composition::new()),
            last_up: RefCell::new(None),
        }
    }
}

impl Drop for TextService {
    fn drop(&mut self) {
        super::unlock_module();
    }
}

/// 宿主 exe 的文件名部分(取不到则为空串)。
fn host_name() -> String {
    let mut buf = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(None, &mut buf) } as usize;
    if len == 0 || len >= buf.len() {
        return String::new();
    }
    let path = String::from_utf16_lossy(&buf[..len]);
    path.rsplit(['\\', '/']).next().unwrap_or_default().to_string()
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<ITfThreadMgr>, tid: u32) -> Result<()> {
        let thread_mgr = ptim.ok()?.clone();
        self.client_id.set(tid);
        let keystroke: ITfKeystrokeMgr = thread_mgr.cast()?;
        let sink: ITfKeyEventSink = self.to_interface();
        unsafe { keystroke.AdviseKeyEventSink(tid, &sink, true)? };
        *self.thread_mgr.borrow_mut() = Some(thread_mgr);
        // 连接推迟到第一次按键,避免激活变慢。
        *self.client.borrow_mut() = Some(Client::new(host_name()));
        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        if let Some(mut client) = self.client.borrow_mut().take() {
            let sid = client.sid();
            client.send(ClientMessage::Close { sid });
        }
        self.comp.clear();
        self.last_up.borrow_mut().take();
        if let Some(thread_mgr) = self.thread_mgr.borrow_mut().take() {
            if let Ok(keystroke) = thread_mgr.cast::<ITfKeystrokeMgr>() {
                let _ = unsafe { keystroke.UnadviseKeyEventSink(self.client_id.get()) };
            }
        }
        Ok(())
    }
}

impl ITfCompositionSink_Impl for TextService_Impl {
    fn OnCompositionTerminated(&self, _ecwrite: u32, _pcomposition: Ref<ITfComposition>) -> Result<()> {
        // 可能在 edit session 内部同步回调:只清引用并通知 server,不再请求 edit session。
        self.comp.clear();
        let _ = self.call("onKeyDown", VK_ESCAPE, 0, &[0u8; 256]);
        Ok(())
    }
}

impl TextService_Impl {
    /// 发一次按键请求;`client` 只在此调用期间借用,返回后即释放。
    fn call(&self, method: &str, vk: u32, code: u32, states: &[u8]) -> Option<Action> {
        let req = key_request(method, vk, code, states);
        let mut guard = self.client.try_borrow_mut().ok()?;
        let resp = guard.as_mut()?.call(req)?;
        Some(Action::from_resp(&resp))
    }

    fn bypassed(&self, ctx: &ITfContext) -> bool {
        match self.thread_mgr.try_borrow() {
            Ok(tm) => tm.as_ref().is_none_or(|tm| keys::bypass(tm, ctx)),
            Err(_) => true,
        }
    }

    fn filter_down(&self, method: &str, pic: &Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Option<Action> {
        let ctx = pic.as_ref()?;
        if self.bypassed(ctx) {
            return None;
        }
        let vk = wparam.0 as u32;
        let states = keys::key_states();
        let code = keys::char_code(vk, lparam.0, &states);
        self.call(method, vk, code, &states)
    }

    /// 应用编辑并回报光标矩形。返回 edit session 是否成功(无需编辑时视为成功)。
    fn apply_action(&self, ctx: &ITfContext, action: &Action) -> bool {
        if !action.needs_edit() {
            return true;
        }
        let sink: ITfCompositionSink = self.to_interface();
        let rect = composition::apply(&self.comp, ctx, self.client_id.get(), &sink, action);
        // 有组字却拿不到矩形,视为 edit session 失败。
        let Some(r) = rect else { return action.composition.is_none() };
        if let Ok(mut guard) = self.client.try_borrow_mut() {
            if let Some(client) = guard.as_mut() {
                let sid = client.sid();
                client.send(ClientMessage::Rect { sid, left: r.left, top: r.top, right: r.right, bottom: r.bottom });
            }
        }
        true
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    fn OnSetFocus(&self, _foreground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let eat = self.filter_down("filterKeyDown", &pic, wparam, lparam).is_some_and(|a| a.eat);
        Ok(if eat { TRUE } else { FALSE })
    }

    fn OnKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let Some(action) = self.filter_down("onKeyDown", &pic, wparam, lparam) else { return Ok(FALSE) };
        if let Some(ctx) = pic.as_ref() {
            if !self.apply_action(ctx, &action) && action.eat {
                log::write("edit session 失败,按键仍被吃掉以保持状态一致");
            }
        }
        Ok(if action.eat { TRUE } else { FALSE })
    }

    fn OnTestKeyUp(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        let vk = wparam.0 as u32;
        if !wants_key_up(vk) {
            return Ok(FALSE);
        }
        let _ = lparam;
        let Some(ctx) = pic.as_ref() else { return Ok(FALSE) };
        if self.bypassed(ctx) {
            return Ok(FALSE);
        }
        let Some(action) = self.call("filterKeyUp", vk, 0, &keys::key_states()) else { return Ok(FALSE) };
        let edit = action.needs_edit();
        *self.last_up.borrow_mut() = Some((vk, action));
        Ok(if edit { TRUE } else { FALSE })
    }

    fn OnKeyUp(&self, pic: Ref<ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        let vk = wparam.0 as u32;
        let cached = self.last_up.borrow_mut().take();
        let Some((cvk, action)) = cached else { return Ok(FALSE) };
        if cvk != vk {
            return Ok(FALSE);
        }
        if let Some(ctx) = pic.as_ref() {
            self.apply_action(ctx, &action);
        }
        Ok(if action.needs_edit() { TRUE } else { FALSE })
    }

    fn OnPreservedKey(&self, _pic: Ref<ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        Ok(FALSE)
    }
}
