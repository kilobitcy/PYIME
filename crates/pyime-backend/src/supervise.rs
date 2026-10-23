//! `--supervise` 监护进程:单实例,循环启动 `--worker` 子进程,退出后按策略重启;
//! 收到退出事件(`--quit`)时连同子进程退出。纯逻辑部分(重启策略)各平台可测。
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 监护进程单实例互斥量。
pub const MUTEX_NAME: &str = r"Local\PYIME-supervisor";
/// `--quit` 置位的退出事件。
pub const QUIT_EVENT: &str = r"Local\PYIME-quit";
/// worker 在此时间窗内退出达 `RESTART_MAX` 次即停止重启(词典损坏等致命错误时不无限刷日志)。
pub const RESTART_WINDOW: Duration = Duration::from_secs(60);
pub const RESTART_MAX: usize = 5;

pub struct RestartPolicy {
    window: Duration,
    max: usize,
    exits: VecDeque<Instant>,
}

impl RestartPolicy {
    pub fn new(window: Duration, max: usize) -> RestartPolicy {
        RestartPolicy { window, max, exits: VecDeque::new() }
    }

    /// 记一次 worker 退出;返回是否应重启。
    pub fn on_exit(&mut self, now: Instant) -> bool {
        while self.exits.front().is_some_and(|&t| now.duration_since(t) >= self.window) {
            self.exits.pop_front();
        }
        self.exits.push_back(now);
        self.exits.len() < self.max
    }
}

#[cfg(windows)]
pub fn run() {
    use crate::win::sys::*;
    use crate::{log, win};
    use std::os::windows::io::AsRawHandle;

    log::init();
    log::install_panic_hook();
    let Some(sec) = user_sid().and_then(|sid| Security::from_sddl(&win::object_sddl(&sid))) else {
        return log::line("监护进程:安全描述符无效,退出");
    };
    // SAFETY: 名字以 NUL 结尾(临时 Vec 活到语句结束);sec 在调用期间有效。
    let mutex = unsafe { CreateMutexW(&sec.attrs, 1, wide(MUTEX_NAME).as_ptr()) };
    if mutex == 0 || last_error() == ERROR_ALREADY_EXISTS {
        return; // 已有监护进程
    }
    // SAFETY: 同上;手动复位、初始未置位。
    let quit = unsafe { CreateEventW(&sec.attrs, 1, 0, wide(QUIT_EVENT).as_ptr()) };
    if quit == 0 {
        return log::line(&format!("监护进程:建退出事件失败: 错误码 {}", last_error()));
    }
    let Ok(exe) = std::env::current_exe() else { return log::line("监护进程:取不到自身路径,退出") };
    log::line(&format!("监护进程启动 pyime-server {}", env!("CARGO_PKG_VERSION")));
    let mut policy = RestartPolicy::new(RESTART_WINDOW, RESTART_MAX);
    loop {
        let mut child = match std::process::Command::new(&exe).arg("--worker").spawn() {
            Ok(c) => c,
            Err(e) => return log::line(&format!("监护进程:启动 worker 失败,退出: {e}")),
        };
        let handles = [child.as_raw_handle() as Handle, quit];
        // SAFETY: 两个句柄在等待期间都有效(child 未被 drop,quit 未关闭)。
        let r = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
        if r != WAIT_OBJECT_0 {
            log::line(if r == WAIT_OBJECT_0 + 1 { "监护进程:收到退出通知" } else { "监护进程:等待失败,退出" });
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        log::line(&format!("worker 退出: {:?}", child.wait()));
        if !policy.on_exit(Instant::now()) {
            return log::line("worker 60s 内退出 5 次,停止重启");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// `--quit`:通知监护进程连同 worker 退出,最多等 3s;监护进程不在时立即返回。
#[cfg(windows)]
pub fn quit() {
    use crate::win::sys::*;
    const EVENT_MODIFY_STATE: u32 = 0x0002;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    // SAFETY: 名字以 NUL 结尾;打开的句柄在本函数末尾关闭。
    unsafe {
        let event = OpenEventW(EVENT_MODIFY_STATE, 0, wide(QUIT_EVENT).as_ptr());
        if event == 0 {
            return;
        }
        let mutex = OpenMutexW(SYNCHRONIZE, 0, wide(MUTEX_NAME).as_ptr());
        SetEvent(event);
        if mutex != 0 {
            WaitForSingleObject(mutex, 3000);
            CloseHandle(mutex);
        }
        CloseHandle(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 窗口内前四次退出都重启_第五次停止() {
        let t0 = Instant::now();
        let mut p = RestartPolicy::new(RESTART_WINDOW, RESTART_MAX);
        for i in 0..4 {
            assert!(p.on_exit(t0 + Duration::from_secs(i)), "第 {} 次", i + 1);
        }
        assert!(!p.on_exit(t0 + Duration::from_secs(10)));
    }

    #[test]
    fn 间隔超过窗口的退出不累计() {
        let t0 = Instant::now();
        let mut p = RestartPolicy::new(RESTART_WINDOW, RESTART_MAX);
        for i in 0..20 {
            assert!(p.on_exit(t0 + Duration::from_secs(i * 61)), "第 {} 次", i + 1);
        }
    }

    #[test]
    fn 旧退出滑出窗口后重新计数() {
        let t0 = Instant::now();
        let mut p = RestartPolicy::new(RESTART_WINDOW, RESTART_MAX);
        for i in 0..4 {
            assert!(p.on_exit(t0 + Duration::from_secs(i)));
        }
        // 60s 后前 4 次都已滑出窗口
        assert!(p.on_exit(t0 + Duration::from_secs(64)));
    }
}
