//! TSF DLL 与 pyime-server 之间的管道协议:信封类型与长度前缀帧。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Read, Write};

/// 单帧正文上限(字节)。
pub const MAX_FRAME: usize = 1 << 20;

/// DLL → server。`Req.json` 是 `Session::handle` 的 PIME 风格请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Open { sid: u64, host: String },
    Req { sid: u64, json: Value },
    Rect { sid: u64, left: i32, top: i32, right: i32, bottom: i32 },
    Close { sid: u64 },
}

/// server → DLL。`Resp.json` 是 `Session::handle` 的 PIME 风格响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Resp { sid: u64, json: Value },
}

/// 每用户一个管道,避免多用户/RDP 下互相连错。
pub fn pipe_name(user: &str) -> String {
    format!(r"\\.\pipe\pyime-{user}")
}

pub fn encode_frame<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
    let body = serde_json::to_vec(msg).map_err(io::Error::other)?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "帧过大"));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}

pub fn try_decode_frame<T: DeserializeOwned>(buf: &[u8]) -> io::Result<Option<(T, usize)>> {
    let Some(head) = buf.get(..4) else { return Ok(None) };
    let n = u32::from_le_bytes(head.try_into().unwrap()) as usize;
    if n > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "帧过大"));
    }
    let Some(body) = buf.get(4..4 + n) else { return Ok(None) };
    let msg = serde_json::from_slice(body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some((msg, 4 + n)))
}

pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    w.write_all(&encode_frame(msg)?)?;
    w.flush()
}

pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<T> {
    let mut head = [0u8; 4];
    r.read_exact(&mut head)?;
    let n = u32::from_le_bytes(head) as usize;
    if n > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "帧过大"));
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    fn samples() -> Vec<ClientMessage> {
        vec![
            ClientMessage::Open { sid: 1, host: "notepad.exe".into() },
            ClientMessage::Req { sid: 2, json: json!({"method": "onKeyDown", "keyCode": 78}) },
            ClientMessage::Rect { sid: 3, left: -10, top: 20, right: 30, bottom: 44 },
            ClientMessage::Close { sid: 4 },
        ]
    }

    #[test]
    fn 管道名带用户标识() {
        assert_eq!(pipe_name("S-1-5-21-1"), r"\\.\pipe\pyime-S-1-5-21-1");
    }

    #[test]
    fn 信封往返() {
        for m in samples() {
            let bytes = encode_frame(&m).unwrap();
            let (back, used): (ClientMessage, usize) = try_decode_frame(&bytes).unwrap().unwrap();
            assert_eq!(back, m);
            assert_eq!(used, bytes.len());
        }
        let r = ServerMessage::Resp { sid: 9, json: json!({"return": true, "compositionString": ""}) };
        let bytes = encode_frame(&r).unwrap();
        let (back, _): (ServerMessage, usize) = try_decode_frame(&bytes).unwrap().unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn 帧头是小端长度() {
        let bytes = encode_frame(&ClientMessage::Close { sid: 4 }).unwrap();
        let n = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert_eq!(n, bytes.len() - 4);
        assert!(bytes[4..].starts_with(b"{"));
    }

    #[test]
    fn 缓冲不足返回none() {
        let bytes = encode_frame(&ClientMessage::Close { sid: 4 }).unwrap();
        assert!(try_decode_frame::<ClientMessage>(&bytes[..2]).unwrap().is_none());
        assert!(try_decode_frame::<ClientMessage>(&bytes[..bytes.len() - 1]).unwrap().is_none());
    }

    #[test]
    fn 连续两帧按消耗字节数切分() {
        let mut buf = encode_frame(&ClientMessage::Close { sid: 1 }).unwrap();
        buf.extend(encode_frame(&ClientMessage::Close { sid: 2 }).unwrap());
        let (a, used): (ClientMessage, usize) = try_decode_frame(&buf).unwrap().unwrap();
        assert_eq!(a, ClientMessage::Close { sid: 1 });
        let (b, _): (ClientMessage, usize) = try_decode_frame(&buf[used..]).unwrap().unwrap();
        assert_eq!(b, ClientMessage::Close { sid: 2 });
    }

    #[test]
    fn 超长帧被拒绝() {
        let mut buf = ((MAX_FRAME + 1) as u32).to_le_bytes().to_vec();
        buf.extend([0u8; 8]);
        let e = try_decode_frame::<ClientMessage>(&buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn 非法json报invaliddata() {
        let mut buf = 3u32.to_le_bytes().to_vec();
        buf.extend(b"{{{");
        let e = try_decode_frame::<ClientMessage>(&buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn 读写封装往返并在截断时报eof() {
        let mut out = Vec::new();
        for m in samples() {
            write_frame(&mut out, &m).unwrap();
        }
        let mut cur = Cursor::new(out.clone());
        for m in samples() {
            let back: ClientMessage = read_frame(&mut cur).unwrap();
            assert_eq!(back, m);
        }
        let mut cut = Cursor::new(out[..out.len() - 1].to_vec());
        let mut last = Ok(ClientMessage::Close { sid: 0 });
        for _ in 0..samples().len() {
            last = read_frame(&mut cut);
        }
        assert_eq!(last.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
