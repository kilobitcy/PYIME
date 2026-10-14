//! 候选窗。本版本是日志桩,Task 6 替换为 GDI 实现(签名不变)。

use crate::ui::UiCmd;
use pyime_backend::log;

pub struct CandWindow;

impl CandWindow {
    pub fn new() -> CandWindow {
        CandWindow
    }

    pub fn apply(&mut self, cmd: UiCmd) {
        log::line(&format!("ui: {cmd:?}"));
    }
}
