//! 在 PIME 的 backends.json 中登记/移除本后端(幂等),供安装器调用。
use serde_json::{json, Value};

pub const BACKEND_NAME: &str = "pyime";

fn entry() -> Value {
    json!({
        "name": BACKEND_NAME,
        "command": "pyime\\server.exe",
        "workingDir": "pyime",
        "params": ""
    })
}

fn parse_list(text: &str) -> Result<Vec<Value>, String> {
    match serde_json::from_str::<Value>(text).map_err(|e| e.to_string())? {
        Value::Array(list) => Ok(list),
        _ => Err("backends.json 顶层不是数组".into()),
    }
}

fn is_ours(v: &Value) -> bool {
    v["name"] == BACKEND_NAME
}

/// 返回加入本后端后的 backends.json 文本;已存在则先替换,不会重复。
pub fn add_backend(text: &str) -> Result<String, String> {
    let mut list = parse_list(text)?;
    list.retain(|v| !is_ours(v));
    list.push(entry());
    serde_json::to_string_pretty(&list).map_err(|e| e.to_string())
}

/// 返回移除本后端后的 backends.json 文本。
pub fn remove_backend(text: &str) -> Result<String, String> {
    let mut list = parse_list(text)?;
    list.retain(|v| !is_ours(v));
    serde_json::to_string_pretty(&list).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"[{"name":"python","command":"p","workingDir":"p","params":""}]"#;

    fn names(text: &str) -> Vec<String> {
        parse_list(text).unwrap().iter().map(|v| v["name"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn 添加保留已有后端() {
        assert_eq!(names(&add_backend(BASE).unwrap()), ["python", "pyime"]);
    }

    #[test]
    fn 重复添加不产生重复项() {
        let once = add_backend(BASE).unwrap();
        assert_eq!(names(&add_backend(&once).unwrap()), ["python", "pyime"]);
    }

    #[test]
    fn 移除只删本后端() {
        let once = add_backend(BASE).unwrap();
        assert_eq!(names(&remove_backend(&once).unwrap()), ["python"]);
    }

    #[test]
    fn 非数组报错() {
        assert!(add_backend("{}").is_err());
    }
}
