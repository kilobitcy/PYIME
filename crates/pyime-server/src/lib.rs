//! pyime-server:独立输入法服务进程。纯逻辑跨平台,Windows 专有部分在 `win`。

pub mod core;
pub mod ui;

#[cfg(windows)]
pub mod win;
