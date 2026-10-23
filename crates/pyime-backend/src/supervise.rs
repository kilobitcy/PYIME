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
