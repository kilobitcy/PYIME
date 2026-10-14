//! 候选窗(GDI):横排、序号、当前项高亮;不抢焦点,不进任务栏。

use crate::ui::{CandView, Rect, UiCmd};
use pyime_backend::log;
use std::cell::RefCell;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect,
    GetDC, GetMonitorInfoW, GetTextExtentPoint32W, InvalidateRect, MonitorFromPoint, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, FW_NORMAL, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
    TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, SetWindowPos, ShowWindow, HWND_TOPMOST,
    MA_NOACTIVATE, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_MOUSEACTIVATE,
    WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{w, PCWSTR};

const PAD_X: i32 = 8;
const PAD_Y: i32 = 5;
const MARGIN: i32 = 4;
const GAP: i32 = 4;

thread_local! {
    static VIEW: RefCell<Option<CandView>> = const { RefCell::new(None) };
}

pub struct CandWindow {
    hwnd: HWND,
    visible: bool,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn make_font(view: &CandView, dpi: u32) -> HFONT {
    let height = -(view.font_size * dpi as i32 / 72);
    let face = wide(&view.font);
    unsafe {
        CreateFontW(height, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, DEFAULT_CHARSET, FONT_OUTPUT_PRECISION(0), FONT_CLIP_PRECISION(0),
            CLEARTYPE_QUALITY, 0, PCWSTR(face.as_ptr()))
    }
}

/// 每项的横向区间 [left,right) 与整窗尺寸。
fn layout(view: &CandView, dpi: u32) -> (Vec<(i32, i32)>, SIZE) {
    let hdc = unsafe { GetDC(None) };
    let font = make_font(view, dpi);
    let old = unsafe { SelectObject(hdc, font.into()) };
    let (mut x, mut text_h, mut spans) = (MARGIN, 0, Vec::new());
    for label in &view.labels {
        let text = wide(label);
        let mut size = SIZE::default();
        let _ = unsafe { GetTextExtentPoint32W(hdc, &text[..text.len() - 1], &mut size) };
        text_h = text_h.max(size.cy);
        spans.push((x, x + size.cx + 2 * PAD_X));
        x += size.cx + 2 * PAD_X + GAP;
    }
    unsafe {
        SelectObject(hdc, old);
        let _ = DeleteObject(font.into());
        ReleaseDC(None, hdc);
    }
    (spans, SIZE { cx: x - GAP + MARGIN, cy: text_h + 2 * PAD_Y + 2 * MARGIN })
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    VIEW.with(|v| {
        let Some(view) = v.borrow().clone() else { return };
        let dpi = unsafe { GetDPIForWindowOr96(hwnd) };
        let (spans, size) = layout(&view, dpi);
        let whole = RECT { left: 0, top: 0, right: size.cx, bottom: size.cy };
        unsafe {
            let white = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00FF_FFFF));
            let border = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00B0_B0B0));
            let hi = CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00FF_E4CC)); // BGR:浅蓝
            FillRect(hdc, &whole, white);
            FrameRect(hdc, &whole, border);
            let font = make_font(&view, dpi);
            let old = SelectObject(hdc, font.into());
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, windows::Win32::Foundation::COLORREF(0));
            for (i, label) in view.labels.iter().enumerate() {
                let (l, r) = spans[i];
                let mut item = RECT { left: l, top: MARGIN, right: r, bottom: size.cy - MARGIN };
                if i == view.cursor {
                    FillRect(hdc, &item, hi);
                }
                let mut text = wide(label);
                let n = text.len() - 1;
                DrawTextW(hdc, &mut text[..n], &mut item, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
            }
            SelectObject(hdc, old);
            for obj in [font.into(), white.into(), border.into(), hi.into()] {
                let _ = DeleteObject(obj);
            }
        }
    });
    let _ = unsafe { EndPaint(hwnd, &ps) };
}

#[allow(non_snake_case)]
unsafe fn GetDPIForWindowOr96(hwnd: HWND) -> u32 {
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => 96,
        d => d,
    }
}

impl CandWindow {
    pub fn new() -> CandWindow {
        let hinst = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: w!("PYIMECandidate"),
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("PYIMECandidate"),
                w!(""),
                WS_POPUP,
                0, 0, 0, 0,
                None, None, Some(hinst.into()), None,
            )
        }
        .unwrap_or_else(|e| {
            log::line(&format!("创建候选窗失败: {e}"));
            HWND::default()
        });
        CandWindow { hwnd, visible: false }
    }

    pub fn apply(&mut self, cmd: UiCmd) {
        if self.hwnd.0.is_null() {
            return;
        }
        match cmd {
            UiCmd::Hide => {
                let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
                self.visible = false;
            }
            UiCmd::Show(view) => {
                let rect = view.rect;
                VIEW.with(|v| *v.borrow_mut() = Some(view));
                self.place(rect);
                let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
                let _ = unsafe { InvalidateRect(Some(self.hwnd), None, true) };
                self.visible = true;
            }
            UiCmd::Move(rect) if self.visible => self.place(rect),
            UiCmd::Move(_) => {}
        }
    }

    /// 放在光标矩形下方;越出所在显示器工作区则翻到上方/左移。
    fn place(&self, rect: Rect) {
        let Some(view) = VIEW.with(|v| v.borrow().clone()) else { return };
        let dpi = unsafe { GetDPIForWindowOr96(self.hwnd) };
        let (_, size) = layout(&view, dpi);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let monitor = unsafe { MonitorFromPoint(POINT { x: rect.left, y: rect.bottom }, MONITOR_DEFAULTTONEAREST) };
        let work = if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            info.rcWork
        } else {
            RECT { left: 0, top: 0, right: i32::MAX / 2, bottom: i32::MAX / 2 }
        };
        let mut x = rect.left;
        let mut y = rect.bottom + 4;
        if x + size.cx > work.right {
            x = work.right - size.cx;
        }
        if y + size.cy > work.bottom {
            y = rect.top - size.cy - 4;
        }
        x = x.max(work.left);
        y = y.max(work.top);
        let _ = unsafe { SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, size.cx, size.cy, SWP_NOACTIVATE) };
    }
}
