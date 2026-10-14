//! 到 pyime-server 的 overlapped 命名管道客户端:读写共用 150 ms 总截止时间,绝不无限阻塞宿主。

use std::io;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use pyime_protocol::{
    current_user_sid, encode_frame, pipe_name, ClientMessage, ServerMessage, MAX_FRAME,
};
use serde_json::Value;
use windows::core::HSTRING;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_NONE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcessId, ResetEvent, WaitForSingleObject,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

use super::{launch, log};

const DEADLINE_MS: u64 = 150;
static NEXT_SEQ: AtomicU32 = AtomicU32::new(1);

/// 读/写恰好 buf.len() 字节,整个调用共用同一截止时间。
fn io_exact(
    handle: HANDLE,
    event: HANDLE,
    buf: &mut [u8],
    write: bool,
    deadline: Instant,
) -> io::Result<()> {
    let mut done = 0;
    while done < buf.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut ov = OVERLAPPED { hEvent: event, ..Default::default() };
        unsafe { ResetEvent(event).map_err(io::Error::other)? };
        let chunk = &mut buf[done..];
        let r = unsafe {
            if write {
                WriteFile(handle, Some(chunk), None, Some(&mut ov))
            } else {
                ReadFile(handle, Some(chunk), None, Some(&mut ov))
            }
        };
        if let Err(e) = r {
            if e.code() != ERROR_IO_PENDING.to_hresult() {
                return Err(io::Error::other(e));
            }
            if unsafe { WaitForSingleObject(event, left.as_millis() as u32) } != WAIT_OBJECT_0 {
                // 超时:取消并等取消完成(ov 在栈上,必须等内核不再引用它)。
                unsafe {
                    let _ = CancelIoEx(handle, Some(&ov));
                    let mut n = 0;
                    let _ = GetOverlappedResult(handle, &ov, &mut n, true);
                }
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
        let mut n = 0u32;
        unsafe { GetOverlappedResult(handle, &ov, &mut n, false) }.map_err(io::Error::other)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        done += n as usize;
    }
    Ok(())
}

pub(crate) struct Client {
    host: String,
    sid: u64,
    pipe: Option<HANDLE>,
    event: HANDLE,
}

impl Client {
    pub(crate) fn new(host: String) -> Client {
        let sid = ((unsafe { GetCurrentProcessId() } as u64) << 32)
            | NEXT_SEQ.fetch_add(1, Ordering::Relaxed) as u64;
        let event = unsafe { CreateEventW(None, true, false, None) }.unwrap_or_default();
        Client { host, sid, pipe: None, event }
    }

    pub(crate) fn sid(&self) -> u64 {
        self.sid
    }

    fn deadline() -> Instant {
        Instant::now() + Duration::from_millis(DEADLINE_MS)
    }

    fn drop_pipe(&mut self, why: &str) {
        if let Some(h) = self.pipe.take() {
            unsafe {
                let _ = CloseHandle(h);
            }
            log::write(&format!("管道断开:{why}"));
        }
    }

    fn ensure_connected(&mut self) -> bool {
        if self.pipe.is_some() {
            return true;
        }
        if self.event.is_invalid() {
            return false;
        }
        let Some(user) = current_user_sid() else { return false };
        let name = HSTRING::from(pipe_name(&user));
        let opened = unsafe {
            CreateFileW(
                &name,
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
        };
        let h = match opened {
            Ok(h) if h != INVALID_HANDLE_VALUE => h,
            _ => {
                launch::try_spawn_server();
                return false;
            }
        };
        self.pipe = Some(h);
        let open = ClientMessage::Open { sid: self.sid, host: self.host.clone() };
        if self.write_msg(&open, Self::deadline()).is_err() {
            self.drop_pipe("写 Open 失败");
            return false;
        }
        true
    }

    fn write_msg(&mut self, msg: &ClientMessage, deadline: Instant) -> io::Result<()> {
        let mut bytes = encode_frame(msg)?;
        let h = self.pipe.ok_or(io::ErrorKind::NotConnected)?;
        io_exact(h, self.event, &mut bytes, true, deadline)
    }

    fn read_resp(&mut self, deadline: Instant) -> io::Result<ServerMessage> {
        let h = self.pipe.ok_or(io::ErrorKind::NotConnected)?;
        let mut head = [0u8; 4];
        io_exact(h, self.event, &mut head, false, deadline)?;
        let n = u32::from_le_bytes(head) as usize;
        if n > MAX_FRAME {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "帧过大"));
        }
        let mut body = vec![0u8; n];
        io_exact(h, self.event, &mut body, false, deadline)?;
        serde_json::from_slice(&body).map_err(io::Error::other)
    }

    /// 发 `Req` 并读一帧 `Resp`,返回其 `json`;任何失败返回 `None`。
    pub(crate) fn call(&mut self, json: Value) -> Option<Value> {
        if !self.ensure_connected() {
            return None;
        }
        let deadline = Self::deadline();
        let req = ClientMessage::Req { sid: self.sid, json };
        let res = self.write_msg(&req, deadline).and_then(|_| self.read_resp(deadline));
        match res {
            Ok(ServerMessage::Resp { json, .. }) => Some(json),
            Err(e) => {
                self.drop_pipe(&e.to_string());
                None
            }
        }
    }

    /// 只写不读(`Rect`、`Close` 用);失败静默。
    pub(crate) fn send(&mut self, msg: ClientMessage) {
        if !self.ensure_connected() {
            return;
        }
        if let Err(e) = self.write_msg(&msg, Self::deadline()) {
            self.drop_pipe(&e.to_string());
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(h) = self.pipe.take() {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
        if !self.event.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.event);
            }
        }
    }
}
