//! PIME 后端行协议:请求 `<client_id>|<JSON>`,响应 `PIME_MSG|<client_id>|<JSON>`。
use serde_json::Value;

/// 解析一行请求,返回 (client_id, JSON)。格式不合法返回 None。
pub fn parse_request(line: &str) -> Option<(&str, Value)> {
    let (id, body) = line.split_once('|')?;
    let json = serde_json::from_str(body).ok()?;
    Some((id, json))
}

/// 组装一行响应(不含换行)。
pub fn format_response(client_id: &str, body: &Value) -> String {
    format!("PIME_MSG|{}|{}", client_id, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 解析合法请求() {
        let (id, v) = parse_request(r#"c1|{"method":"init","seqNum":1}"#).unwrap();
        assert_eq!(id, "c1");
        assert_eq!(v["method"], "init");
    }

    #[test]
    fn 非法请求返回none() {
        assert!(parse_request("没有分隔符").is_none());
        assert!(parse_request("c1|不是json").is_none());
    }

    #[test]
    fn 响应带前缀() {
        let s = format_response("c1", &json!({"success": true}));
        assert_eq!(s, r#"PIME_MSG|c1|{"success":true}"#);
    }
}
