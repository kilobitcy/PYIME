#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    pyime_server::win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("pyime-server 目前只支持 Windows");
}
