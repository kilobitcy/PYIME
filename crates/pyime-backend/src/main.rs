// GUI 子系统:由监护进程/Run 键启动时不弹控制台窗。
#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg_attr(not(windows), allow(dead_code))] // 只被 Windows 的 pipe.rs 使用;纯逻辑在 Linux 上单测
mod hub;
#[cfg_attr(not(windows), allow(dead_code))] // 加载入口只被 Windows 的 pipe.rs 调用
mod ime;
mod inputlog;
mod log;
#[cfg(windows)]
mod pipe;
mod punct;
mod session;
#[cfg_attr(not(windows), allow(dead_code))] // 监护进程只在 Windows 运行;纯逻辑在 Linux 上单测
mod supervise;
mod tray;
#[allow(dead_code)] // sys 里部分声明(CreateFileW 等)只给 pyime-pipe-cli 用;纯字符串函数在 Linux 上只被测试用
mod win;

/// 打开日志、输入日志与 panic 钩子,记启动信息。
#[cfg_attr(not(windows), allow(dead_code))]
fn start_logs(args: &[String]) {
    log::init();
    inputlog::init();
    log::install_panic_hook();
    log::line(&format!(
        "启动 pyime-server {} args={:?} exe={:?}",
        env!("CARGO_PKG_VERSION"),
        args,
        std::env::current_exe().ok()
    ));
}

/// Windows 专有模式:`--worker` 管道服务、`--supervise` 监护进程、`--quit` 通知退出。处理了返回 true。
#[cfg(windows)]
fn run_windows_mode(args: &[String]) -> bool {
    match args.get(1).map(String::as_str) {
        Some("--worker") => {
            start_logs(args);
            pipe::run();
        }
        Some("--supervise") => supervise::run(),
        Some("--quit") => supervise::quit(),
        _ => return false,
    }
    true
}

#[cfg(not(windows))]
fn run_windows_mode(_: &[String]) -> bool {
    false
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !run_windows_mode(&args) {
        eprintln!("用法: pyime-server.exe --supervise | --worker | --quit");
        std::process::exit(2);
    }
}
