//! Win32 小工具:管道名、安全描述符 SDDL(纯字符串,各平台可测),以及 Windows 下的
//! `extern "system"` 声明与消息模式管道收发。沿用 main.rs 手写声明的写法,不引入 windows crate。
//! `src/bin/pyime-pipe-cli.rs` 经 `#[path]` 复用本文件。

/// 管道路径 `\\.\pipe\<用户名>\PYIME`:带用户名,多用户不串。
pub fn pipe_path(user: &str) -> String {
    format!(r"\\.\pipe\{user}\PYIME")
}

/// 管道安全描述符(同 PIME Launcher,`PipeSecurity.cpp:123-149`):拒绝网络访问;允许 SYSTEM、
/// Administrators、ALL APPLICATION PACKAGES 与当前用户;低完整性标签,让 AppContainer/低完整性宿主里的 DLL 能连上。
/// AC 的访问掩码 0x12019B = FILE_GENERIC_READ|FILE_GENERIC_WRITE 去掉 FILE_APPEND_DATA(即
/// FILE_CREATE_PIPE_INSTANCE):防商店应用另开同名管道实例截获按键。客户端因此须以
/// GENERIC_READ|FILE_WRITE_DATA|FILE_WRITE_ATTRIBUTES 打开管道。
pub fn pipe_sddl(user_sid: &str) -> String {
    format!("D:(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12019B;;;AC)(A;;GA;;;{user_sid})S:(ML;;NWNR;;;LW)")
}

/// 退出事件与互斥量的安全描述符:只允许 SYSTEM、Administrators(提权安装器)与当前用户,不对应用容器开放。
pub fn object_sddl(user_sid: &str) -> String {
    format!("D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{user_sid})")
}

#[cfg(windows)]
pub mod sys {
    use std::ffi::c_void;

    pub type Handle = isize;
    pub const INVALID_HANDLE_VALUE: Handle = -1;
    pub const ERROR_MORE_DATA: u32 = 234;
    pub const ERROR_PIPE_CONNECTED: u32 = 535;
    pub const ERROR_ALREADY_EXISTS: u32 = 183;
    pub const INFINITE: u32 = 0xFFFF_FFFF;
    pub const WAIT_OBJECT_0: u32 = 0;

    #[repr(C)]
    pub struct SecurityAttributes {
        pub length: u32,
        pub descriptor: *mut c_void,
        pub inherit: i32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateNamedPipeW(name: *const u16, open_mode: u32, pipe_mode: u32, max_instances: u32, out_size: u32, in_size: u32, timeout_ms: u32, sa: *const SecurityAttributes) -> Handle;
        pub fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
        pub fn DisconnectNamedPipe(pipe: Handle) -> i32;
        pub fn CreateFileW(name: *const u16, access: u32, share: u32, sa: *const SecurityAttributes, disposition: u32, flags: u32, template: Handle) -> Handle;
        pub fn SetNamedPipeHandleState(pipe: Handle, mode: *const u32, max_count: *const u32, timeout: *const u32) -> i32;
        pub fn ReadFile(h: Handle, buf: *mut u8, len: u32, read: *mut u32, overlapped: *mut c_void) -> i32;
        pub fn WriteFile(h: Handle, buf: *const u8, len: u32, written: *mut u32, overlapped: *mut c_void) -> i32;
        pub fn CloseHandle(h: Handle) -> i32;
        pub fn GetLastError() -> u32;
        pub fn LocalFree(mem: *mut c_void) -> *mut c_void;
        pub fn GetCurrentProcess() -> Handle;
        pub fn CreateMutexW(sa: *const SecurityAttributes, initial_owner: i32, name: *const u16) -> Handle;
        pub fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> Handle;
        pub fn CreateEventW(sa: *const SecurityAttributes, manual_reset: i32, initial_state: i32, name: *const u16) -> Handle;
        pub fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> Handle;
        pub fn SetEvent(h: Handle) -> i32;
        pub fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
        pub fn WaitForMultipleObjects(count: u32, handles: *const Handle, wait_all: i32, ms: u32) -> u32;
    }

    #[link(name = "advapi32")]
    extern "system" {
        pub fn GetUserNameW(buf: *mut u16, len: *mut u32) -> i32;
        pub fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        pub fn GetTokenInformation(token: Handle, class: u32, info: *mut c_void, len: u32, ret_len: *mut u32) -> i32;
        pub fn ConvertSidToStringSidW(sid: *mut c_void, out: *mut *mut u16) -> i32;
        pub fn ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl: *const u16, revision: u32, sd: *mut *mut c_void, size: *mut u32) -> i32;
    }

    /// 以 NUL 结尾的 UTF-16。
    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub fn last_error() -> u32 {
        // SAFETY: 无参数,只读线程的最近错误码。
        unsafe { GetLastError() }
    }

    /// 当前进程的用户名(管道名用)。
    pub fn user_name() -> Option<String> {
        let mut buf = [0u16; 257];
        let mut len = buf.len() as u32;
        // SAFETY: buf 与 len 是本地变量,长度如实传入。
        if unsafe { GetUserNameW(buf.as_mut_ptr(), &mut len) } == 0 {
            return None;
        }
        // 成功时 len 含结尾 NUL
        Some(String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]))
    }

    /// 当前进程用户的 SID 字符串(如 `S-1-5-21-…`)。
    pub fn user_sid() -> Option<String> {
        const TOKEN_QUERY: u32 = 0x0008;
        const TOKEN_USER: u32 = 1;
        let mut token: Handle = 0;
        // u64 数组保证 TOKEN_USER 结构里指针字段对齐
        let mut buf = [0u64; 64];
        let mut len = 0u32;
        // SAFETY: token/buf/len 均为本地变量;TOKEN_USER 的首字段是指向 buf 内部的 SID 指针,
        // ConvertSidToStringSidW 分配的字符串读完后 LocalFree;该函数返回 NUL 结尾的字符串,
        // 下面的 take_while 依赖这一点定位结尾。
        unsafe {
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return None;
            }
            let ok = GetTokenInformation(token, TOKEN_USER, buf.as_mut_ptr().cast(), std::mem::size_of_val(&buf) as u32, &mut len);
            CloseHandle(token);
            if ok == 0 {
                return None;
            }
            let sid = *(buf.as_ptr() as *const *mut c_void);
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(sid, &mut s) == 0 {
                return None;
            }
            let n = (0..).take_while(|&i| *s.add(i) != 0).count();
            let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s.cast());
            Some(out)
        }
    }

    /// 由 SDDL 生成的安全属性,析构时释放描述符。内含裸指针,因此是 !Send,需在使用它的线程内构造。
    pub struct Security {
        pub attrs: SecurityAttributes,
    }

    impl Security {
        pub fn from_sddl(sddl: &str) -> Option<Security> {
            let w = wide(sddl);
            let mut sd: *mut c_void = std::ptr::null_mut();
            // SAFETY: w 以 NUL 结尾;sd 由系统分配,Drop 时 LocalFree。
            if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(w.as_ptr(), 1, &mut sd, std::ptr::null_mut()) } == 0 {
                return None;
            }
            let length = std::mem::size_of::<SecurityAttributes>() as u32;
            Some(Security { attrs: SecurityAttributes { length, descriptor: sd, inherit: 0 } })
        }
    }

    impl Drop for Security {
        fn drop(&mut self) {
            // SAFETY: descriptor 来自 ConvertStringSecurityDescriptorToSecurityDescriptorW,只释放一次。
            unsafe { LocalFree(self.attrs.descriptor) };
        }
    }

    /// 单条消息上限:防能连管道的进程用超大消息耗尽 worker 内存。
    pub const MAX_MESSAGE: usize = 1 << 20;

    /// 读一条完整消息(消息模式;超过缓冲区时循环处理 ERROR_MORE_DATA)。对端断开或出错返回 None;
    /// 超过 MAX_MESSAGE 时把该条读完丢弃,返回 None。`h` 须为非重叠(同步)消息模式管道句柄。
    pub fn read_message(h: Handle) -> Option<Vec<u8>> {
        let mut msg = Vec::new();
        let mut too_big = false;
        let mut buf = [0u8; 4096];
        loop {
            let mut n = 0u32;
            // SAFETY: buf 为本地缓冲区,长度如实传入;同步句柄,不传 OVERLAPPED。
            let ok = unsafe { ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) };
            // 先取错误码,免得后面的扩容等操作覆盖它
            let err = if ok == 0 { last_error() } else { 0 };
            if msg.len() + n as usize > MAX_MESSAGE {
                too_big = true;
            } else if !too_big {
                msg.extend_from_slice(&buf[..n as usize]);
            }
            if ok != 0 {
                return if too_big { None } else { Some(msg) };
            }
            if err != ERROR_MORE_DATA {
                return None;
            }
        }
    }

    /// 写一条完整消息。`h` 须为非重叠(同步)消息模式管道句柄。
    pub fn write_message(h: Handle, data: &[u8]) -> bool {
        let mut n = 0u32;
        // SAFETY: data 在调用期间有效,长度如实传入;同步句柄。
        let ok = unsafe { WriteFile(h, data.as_ptr(), data.len() as u32, &mut n, std::ptr::null_mut()) };
        ok != 0 && n as usize == data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 管道名带用户名() {
        assert_eq!(pipe_path("chenyu"), r"\\.\pipe\chenyu\PYIME");
    }

    #[test]
    fn 管道sddl同_pime_launcher() {
        assert_eq!(
            pipe_sddl("S-1-5-21-1-2-3-1001"),
            "D:(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12019B;;;AC)(A;;GA;;;S-1-5-21-1-2-3-1001)S:(ML;;NWNR;;;LW)"
        );
    }

    #[test]
    fn 内核对象sddl不授予应用容器() {
        let s = object_sddl("S-1-5-21-1-2-3-1001");
        assert_eq!(s, "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;S-1-5-21-1-2-3-1001)");
        assert!(!s.contains(";AC)"));
    }
}
