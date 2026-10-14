//! 组字串与上屏:把 `Action` 通过一次同步 edit session 应用到文档。
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::{FALSE, RECT};
use windows::Win32::UI::TextServices::{
    ITfComposition, ITfCompositionSink, ITfContext, ITfContextComposition, ITfEditSession,
    ITfEditSession_Impl, ITfInsertAtSelection, ITfRange, INSERT_TEXT_AT_SELECTION_FLAGS,
    TF_AE_NONE, TF_ANCHOR_END, TF_ANCHOR_START, TF_ES_READWRITE, TF_ES_SYNC, TF_IAS_QUERYONLY,
    TF_SELECTION, TF_SELECTIONSTYLE,
};
use windows::core::{implement, Interface, Result};

use super::log;
use crate::wire::{utf16_offset, Action};

/// 进行中的组字(无则 `None`)。
pub struct Composition {
    inner: RefCell<Option<ITfComposition>>,
}

impl Composition {
    pub fn new() -> Self {
        Self { inner: RefCell::new(None) }
    }

    pub fn is_active(&self) -> bool {
        self.inner.borrow().is_some()
    }

    /// 只丢引用,不碰文档;给 `OnCompositionTerminated` 用。
    pub fn clear(&self) {
        self.inner.borrow_mut().take();
    }
}

impl Default for Composition {
    fn default() -> Self {
        Self::new()
    }
}

#[implement(ITfEditSession)]
struct ApplySession {
    ctx: ITfContext,
    comp: Rc<Composition>,
    sink: ITfCompositionSink,
    action: Action,
    rect: Rc<RefCell<Option<RECT>>>,
}

impl ITfEditSession_Impl for ApplySession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        match run(&self.ctx, &self.comp, &self.sink, &self.action, ec) {
            Ok(rect) => {
                *self.rect.borrow_mut() = rect;
                Ok(())
            }
            Err(e) => {
                log::write(&format!("edit session 失败: {e}"));
                Err(e)
            }
        }
    }
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// 把选区设到 `range`(须已折叠)。
fn set_caret(ctx: &ITfContext, ec: u32, range: &ITfRange) -> Result<()> {
    let sel = TF_SELECTION {
        range: std::mem::ManuallyDrop::new(Some(range.clone())),
        style: TF_SELECTIONSTYLE { ase: TF_AE_NONE, fInterimChar: FALSE },
    };
    let r = unsafe { ctx.SetSelection(ec, &[sel.clone()]) };
    drop(std::mem::ManuallyDrop::into_inner(sel.range));
    r
}

/// 结束组字并清状态(无论文档操作是否成功,状态都清掉)。
fn end(comp: &Composition, ec: u32) -> Result<()> {
    let c = comp.inner.borrow_mut().take();
    match c {
        Some(c) => unsafe { c.EndComposition(ec) },
        None => Ok(()),
    }
}

fn run(
    ctx: &ITfContext,
    comp: &Rc<Composition>,
    sink: &ITfCompositionSink,
    action: &Action,
    ec: u32,
) -> Result<Option<RECT>> {
    if let Some(text) = &action.commit {
        let active = comp.inner.borrow().clone();
        if let Some(c) = active {
            let range = unsafe { c.GetRange()? };
            let set = unsafe { range.SetText(ec, 0, &utf16(text)) };
            let ended = end(comp, ec);
            set?;
            ended?;
            unsafe { range.Collapse(ec, TF_ANCHOR_END)? };
            set_caret(ctx, ec, &range)?;
        } else {
            let insert: ITfInsertAtSelection = ctx.cast()?;
            unsafe {
                insert.InsertTextAtSelection(ec, INSERT_TEXT_AT_SELECTION_FLAGS(0), &utf16(text))?
            };
        }
    }
    if let Some(s) = &action.composition {
        if s.is_empty() {
            let active = comp.inner.borrow().clone();
            if let Some(c) = active {
                let set = unsafe { c.GetRange().and_then(|r| r.SetText(ec, 0, &[])) };
                let ended = end(comp, ec);
                set?;
                ended?;
            }
        } else {
            if !comp.is_active() {
                let insert: ITfInsertAtSelection = ctx.cast()?;
                let at = unsafe { insert.InsertTextAtSelection(ec, TF_IAS_QUERYONLY, &[])? };
                let cc: ITfContextComposition = ctx.cast()?;
                let c = unsafe { cc.StartComposition(ec, &at, sink)? };
                *comp.inner.borrow_mut() = Some(c);
            }
            if let Err(e) = set_composition_text(ctx, comp, ec, s, action.cursor) {
                // 不残留半个组字:结束并清状态。
                let _ = end(comp, ec);
                return Err(e);
            }
        }
    }
    active_rect(ctx, comp, ec)
}

fn set_composition_text(
    ctx: &ITfContext,
    comp: &Composition,
    ec: u32,
    s: &str,
    cursor: Option<usize>,
) -> Result<()> {
    let c = comp.inner.borrow().clone().ok_or(windows::core::Error::from(
        windows::Win32::Foundation::E_UNEXPECTED,
    ))?;
    let range = unsafe { c.GetRange()? };
    unsafe { range.SetText(ec, 0, &utf16(s))? };
    if let Some(n) = cursor {
        let caret = unsafe { range.Clone()? };
        unsafe {
            caret.Collapse(ec, TF_ANCHOR_START)?;
            let mut shifted = 0i32;
            caret.ShiftEnd(ec, utf16_offset(s, n) as i32, &mut shifted, std::ptr::null())?;
            caret.Collapse(ec, TF_ANCHOR_END)?;
        }
        set_caret(ctx, ec, &caret)?;
    }
    Ok(())
}

fn active_rect(ctx: &ITfContext, comp: &Composition, ec: u32) -> Result<Option<RECT>> {
    let active = comp.inner.borrow().clone();
    let Some(c) = active else { return Ok(None) };
    let range = unsafe { c.GetRange()? };
    let view = unsafe { ctx.GetActiveView()? };
    let mut rect = RECT::default();
    let mut clipped = FALSE;
    unsafe { view.GetTextExt(ec, &range, &mut rect, &mut clipped)? };
    Ok(Some(rect))
}

/// 应用 `action`;返回编辑后组字范围的屏幕矩形,无组字或任何一步失败返回 `None`。
pub fn apply(
    comp: &Rc<Composition>,
    ctx: &ITfContext,
    client_id: u32,
    sink: &ITfCompositionSink,
    action: &Action,
) -> Option<RECT> {
    let rect = Rc::new(RefCell::new(None));
    let session: ITfEditSession = ApplySession {
        ctx: ctx.clone(),
        comp: comp.clone(),
        sink: sink.clone(),
        action: action.clone(),
        rect: rect.clone(),
    }
    .into();
    let hr = unsafe { ctx.RequestEditSession(client_id, &session, TF_ES_SYNC | TF_ES_READWRITE) };
    match hr {
        Ok(code) if code.is_ok() => rect.take(),
        Ok(code) => {
            log::write(&format!("RequestEditSession 返回 {code:?}"));
            None
        }
        Err(e) => {
            log::write(&format!("RequestEditSession 失败: {e}"));
            None
        }
    }
}
