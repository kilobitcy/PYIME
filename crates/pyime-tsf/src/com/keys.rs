//! 按键辅助:键态、字符码、放行判定(键盘被禁用的 compartment)。

use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardState, ToUnicode};
use windows::Win32::UI::TextServices::{
    ITfCompartmentMgr, ITfContext, ITfThreadMgr, GUID_COMPARTMENT_KEYBOARD_DISABLED,
};
use windows::core::Interface;

/// 取 256 项键态,失败则全零。
pub fn key_states() -> [u8; 256] {
    let mut states = [0u8; 256];
    if unsafe { GetKeyboardState(&mut states) }.is_err() {
        return [0u8; 256];
    }
    states
}

/// 按下键对应的字符码:仅单字符、且无 Ctrl/Alt 时有值,否则 0。
/// `ToUnicode` 的 flags 用 4,不改变键盘状态(不消耗死键)。
pub fn char_code(vk: u32, lparam: isize, states: &[u8; 256]) -> u32 {
    if states[0x11] & 0x80 != 0 || states[0x12] & 0x80 != 0 {
        return 0;
    }
    let scancode = ((lparam >> 16) & 0xFF) as u32;
    let mut buf = [0u16; 8];
    let n = unsafe { ToUnicode(vk, scancode, Some(states), &mut buf, 4) };
    if n == 1 {
        buf[0] as u32
    } else {
        0
    }
}

fn disabled<T: Interface>(obj: &T) -> bool {
    let Ok(mgr) = obj.cast::<ITfCompartmentMgr>() else { return false };
    let Ok(comp) = (unsafe { mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_DISABLED) }) else {
        return false;
    };
    let Ok(value) = (unsafe { comp.GetValue() }) else { return false };
    i32::try_from(&value).is_ok_and(|v| v != 0)
}

/// 线程级或上下文级键盘被禁用(如密码框)时放行。取值失败视为未禁用。
pub fn bypass(thread_mgr: &ITfThreadMgr, ctx: &ITfContext) -> bool {
    disabled(thread_mgr) || disabled(ctx)
}
