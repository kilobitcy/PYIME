//! 文本服务:激活时注册 key sink;a–z(无 Ctrl/Alt)经 edit session 上屏固定文字。

use std::cell::{Cell, RefCell};

use windows::Win32::Foundation::{FALSE, LPARAM, TRUE, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU};
use windows::Win32::UI::TextServices::{
    ITfContext, ITfEditSession, ITfEditSession_Impl, ITfInsertAtSelection, ITfKeyEventSink,
    ITfKeyEventSink_Impl, ITfKeystrokeMgr, ITfTextInputProcessor, ITfTextInputProcessor_Impl,
    ITfThreadMgr, INSERT_TEXT_AT_SELECTION_FLAGS, TF_ES_READWRITE, TF_ES_SYNC,
};
use windows::core::{implement, IUnknownImpl, Interface, Ref, Result, BOOL, GUID};

use crate::keymap::is_letter_vk;

const TEXT: &str = "测试";

#[implement(ITfTextInputProcessor, ITfKeyEventSink)]
pub struct TextService {
    thread_mgr: RefCell<Option<ITfThreadMgr>>,
    client_id: Cell<u32>,
}

impl TextService {
    pub fn new() -> Self {
        super::lock_module();
        Self { thread_mgr: RefCell::new(None), client_id: Cell::new(0) }
    }
}

impl Drop for TextService {
    fn drop(&mut self) {
        super::unlock_module();
    }
}

fn key_down(vk: i32) -> bool {
    (unsafe { GetKeyState(vk) } as u16 & 0x8000) != 0
}

/// 吃不吃这个键:只吃无 Ctrl/Alt 的 a–z。
fn would_eat(wparam: WPARAM) -> bool {
    is_letter_vk(wparam.0 as u32) && !key_down(VK_CONTROL.0 as i32) && !key_down(VK_MENU.0 as i32)
}

#[implement(ITfEditSession)]
struct InsertSession {
    context: ITfContext,
}

impl ITfEditSession_Impl for InsertSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        let insert: ITfInsertAtSelection = self.context.cast()?;
        let utf16: Vec<u16> = TEXT.encode_utf16().collect();
        unsafe { insert.InsertTextAtSelection(ec, INSERT_TEXT_AT_SELECTION_FLAGS(0), &utf16)? };
        Ok(())
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<ITfThreadMgr>, tid: u32) -> Result<()> {
        let thread_mgr = ptim.ok()?.clone();
        self.client_id.set(tid);
        let keystroke: ITfKeystrokeMgr = thread_mgr.cast()?;
        let sink: ITfKeyEventSink = self.to_interface();
        unsafe { keystroke.AdviseKeyEventSink(tid, &sink, true)? };
        *self.thread_mgr.borrow_mut() = Some(thread_mgr);
        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        if let Some(thread_mgr) = self.thread_mgr.borrow_mut().take() {
            if let Ok(keystroke) = thread_mgr.cast::<ITfKeystrokeMgr>() {
                let _ = unsafe { keystroke.UnadviseKeyEventSink(self.client_id.get()) };
            }
        }
        Ok(())
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    fn OnSetFocus(&self, _foreground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(&self, _pic: Ref<ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(if would_eat(wparam) { TRUE } else { FALSE })
    }

    fn OnKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        if !would_eat(wparam) {
            return Ok(FALSE);
        }
        // 取不到上下文或 edit session 失败(只读文档、密码框等)时放行按键。
        let Ok(context) = pic.ok().cloned() else { return Ok(FALSE) };
        let session: ITfEditSession = InsertSession { context: context.clone() }.into();
        let hr = unsafe { context.RequestEditSession(self.client_id.get(), &session, TF_ES_SYNC | TF_ES_READWRITE) };
        Ok(if matches!(hr, Ok(code) if code.is_ok()) { TRUE } else { FALSE })
    }

    fn OnTestKeyUp(&self, _pic: Ref<ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn OnKeyUp(&self, _pic: Ref<ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(FALSE)
    }

    fn OnPreservedKey(&self, _pic: Ref<ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        Ok(FALSE)
    }
}
