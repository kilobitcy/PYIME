//! 候选窗的显示模型:把 Session 的响应 JSON 翻成"显示/隐藏/移动"指令。纯逻辑,不依赖 Windows。

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandView {
    pub labels: Vec<String>,
    pub cursor: usize,
    pub font: String,
    pub font_size: i32,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiCmd {
    Hide,
    Show(CandView),
    Move(Rect),
}

pub const DEFAULT_FONT: &str = "Microsoft YaHei UI";
pub const DEFAULT_FONT_SIZE: i32 = 16;

pub fn ui_from_resp(resp: &Value, rect: Option<Rect>) -> Option<UiCmd> {
    if !resp["showCandidates"].as_bool()? {
        return Some(UiCmd::Hide);
    }
    let list = resp["candidateList"].as_array()?;
    if list.is_empty() {
        return Some(UiCmd::Hide);
    }
    let labels: Vec<String> = list
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{} {}", i + 1, t.as_str().unwrap_or("")))
        .collect();
    let cursor = (resp["candidateCursor"].as_u64().unwrap_or(0) as usize).min(labels.len() - 1);
    let ui = &resp["customizeUI"];
    Some(UiCmd::Show(CandView {
        labels,
        cursor,
        font: ui["candFontName"].as_str().unwrap_or(DEFAULT_FONT).to_string(),
        font_size: ui["candFontSize"].as_i64().map_or(DEFAULT_FONT_SIZE, |n| n as i32),
        rect: rect.unwrap_or_default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rect() -> Rect {
        Rect { left: 100, top: 200, right: 110, bottom: 220 }
    }

    #[test]
    fn 隐藏() {
        assert_eq!(ui_from_resp(&json!({"showCandidates": false}), None), Some(UiCmd::Hide));
    }

    #[test]
    fn 显示带序号标签与默认字体() {
        let r = json!({"showCandidates": true, "candidateList": ["你", "泥"], "candidateCursor": 1});
        let Some(UiCmd::Show(v)) = ui_from_resp(&r, Some(rect())) else { panic!("应为 Show") };
        assert_eq!(v.labels, vec!["1 你", "2 泥"]);
        assert_eq!(v.cursor, 1);
        assert_eq!(v.font, DEFAULT_FONT);
        assert_eq!(v.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(v.rect, rect());
    }

    #[test]
    fn 字体取自customize_ui() {
        let r = json!({"showCandidates": true, "candidateList": ["你"],
            "customizeUI": {"candFontName": "SimSun", "candFontSize": 20}});
        let Some(UiCmd::Show(v)) = ui_from_resp(&r, None) else { panic!("应为 Show") };
        assert_eq!(v.font, "SimSun");
        assert_eq!(v.font_size, 20);
        assert_eq!(v.rect, Rect::default());
    }

    #[test]
    fn 高亮下标越界时收敛到最后一项() {
        let r = json!({"showCandidates": true, "candidateList": ["你"], "candidateCursor": 5});
        let Some(UiCmd::Show(v)) = ui_from_resp(&r, None) else { panic!("应为 Show") };
        assert_eq!(v.cursor, 0);
    }

    #[test]
    fn 无字段保持不变() {
        assert_eq!(ui_from_resp(&json!({"return": true}), None), None);
    }

    #[test]
    fn 显示但无列表保持不变_空列表则隐藏() {
        assert_eq!(ui_from_resp(&json!({"showCandidates": true}), None), None);
        assert_eq!(
            ui_from_resp(&json!({"showCandidates": true, "candidateList": []}), None),
            Some(UiCmd::Hide)
        );
    }
}
