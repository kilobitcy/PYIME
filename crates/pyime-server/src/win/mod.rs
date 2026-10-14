//! Windows 专有部分:单实例、管道服务、候选窗、消息循环。

pub mod cand;
pub mod pipe;
pub mod single;

use crate::core::{run_pending, Dispatcher, Job};
use cand::CandWindow;
use pyime_backend::hub::Hub;
use pyime_backend::ime::Ime;
use pyime_backend::log;
use pipe::WakeFn;
use pyime_protocol::{current_user_sid, pipe_name};
use std::cell::RefCell;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, RegisterClassW,
    TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WNDCLASSW,
};
use windows::core::w;

/// 主线程独占的状态:Hub(Rc,不可跨线程)、候选窗都在这里。
struct State {
    rx: Receiver<Job>,
    disp: Dispatcher,
    cand: CandWindow,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_APP {
        STATE.with(|s| {
            if let Some(State { rx, disp, cand }) = s.borrow_mut().as_mut() {
                run_pending(rx, disp, |cmd| cand.apply(cmd));
            }
        });
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub fn run() {
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    log::init();
    log::install_panic_hook();
    let Some(_single) = single::acquire() else {
        log::line("已有 pyime-server 在运行,退出");
        return;
    };

    // 仅用于被唤醒的消息窗口
    let hinst = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    let class = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        lpszClassName: w!("PYIMEServerMsg"),
        ..Default::default()
    };
    unsafe { RegisterClassW(&class) };
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("PYIMEServerMsg"),
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst.into()),
            None,
        )
    };
    let Ok(hwnd) = hwnd else {
        log::line("创建消息窗口失败,退出");
        return;
    };
    let hwnd_raw = hwnd.0 as isize;
    let post = move || {
        let _ = unsafe { PostMessageW(Some(HWND(hwnd_raw as *mut _)), WM_APP, WPARAM(0), LPARAM(0)) };
    };
    let wake: WakeFn = Arc::new(post);

    let (tx, rx) = channel::<Job>();
    let name = pipe_name(&current_user_sid().unwrap_or_default());
    if let Err(e) = pipe::serve(&name, tx, wake) {
        log::line(&format!("创建管道 {name} 失败(可能已有实例): {e}"));
        return;
    }
    let ime = Ime::load();
    STATE.with(|s| {
        *s.borrow_mut() = Some(State { rx, disp: Dispatcher::new(Hub::new(ime)), cand: CandWindow::new() });
    });
    // STATE 就绪前到达的唤醒会被忽略,这里补一次,排空期间积压的请求。
    let _ = unsafe { PostMessageW(Some(hwnd), WM_APP, WPARAM(0), LPARAM(0)) };
    log::line(&format!("pyime-server 就绪,管道 {name}"));

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
