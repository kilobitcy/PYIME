//! 单个前端连接的会话状态:拼音串、候选、分页与选词。
use crate::ime::Ime;
use pyime_engine::engine::Candidate;
use serde_json::{json, Value};
use std::rc::Rc;

const VK_BACK: i64 = 8;
const VK_RETURN: i64 = 13;
const VK_ESCAPE: i64 = 27;
const VK_SPACE: i64 = 32;
const VK_PRIOR: i64 = 33;
const VK_NEXT: i64 = 34;
const VK_UP: i64 = 38;
const VK_DOWN: i64 = 40;
const VK_EQUALS: i64 = 187;
const VK_MINUS: i64 = 189;
const VK_LBRACKET: i64 = 219;
const VK_RBRACKET: i64 = 221;
/// 候选横排,每页个数(与微信输入法一致)。
const PAGE_SIZE: usize = 5;

/// 数字键 1-9(主键盘或小键盘)对应的页内下标。
fn digit_index(key: i64) -> Option<usize> {
    match key {
        49..=57 => Some((key - 49) as usize),
        97..=105 => Some((key - 97) as usize),
        _ => None,
    }
}

pub struct Session {
    ime: Rc<Ime>,
    /// 已选定、尚未上屏的文字(分段选词时累积)。
    pending: String,
    /// 尚未转换的拼音字母。
    input: String,
    cands: Vec<Candidate>,
    page: usize,
    /// 页内高亮下标。
    cursor: usize,
}

impl Session {
    pub fn new(ime: Rc<Ime>) -> Session {
        Session { ime, pending: String::new(), input: String::new(), cands: Vec::new(), page: 0, cursor: 0 }
    }

    fn composing(&self) -> bool {
        !self.input.is_empty() || !self.pending.is_empty()
    }

    fn page_items(&self) -> &[Candidate] {
        let start = (self.page * PAGE_SIZE).min(self.cands.len());
        &self.cands[start..(start + PAGE_SIZE).min(self.cands.len())]
    }

    fn page_count(&self) -> usize {
        self.cands.len().div_ceil(PAGE_SIZE)
    }

    /// 该键是否由输入法接管(拼音串存在时,编辑/翻页/数字键也接管)。
    fn wants_key(&self, key: i64) -> bool {
        if (65..=90).contains(&key) {
            return true;
        }
        let editing = matches!(
            key,
            VK_BACK | VK_RETURN | VK_ESCAPE | VK_SPACE | VK_UP | VK_DOWN | VK_PRIOR | VK_NEXT
                | VK_EQUALS | VK_MINUS | VK_LBRACKET | VK_RBRACKET
        ) || digit_index(key).is_some_and(|i| i < self.page_items().len());
        editing && self.composing()
    }

    /// 供日志使用的当前状态摘要。
    pub fn debug_state(&self) -> String {
        format!(
            "pending={:?} input={:?} cands={} page={} cursor={}",
            self.pending, self.input, self.cands.len(), self.page, self.cursor
        )
    }

    pub fn handle(&mut self, req: &Value) -> Value {
        let key = req["keyCode"].as_i64().unwrap_or(0);
        let mut resp = json!({ "success": true, "seqNum": req["seqNum"] });
        match req["method"].as_str().unwrap_or("") {
            "filterKeyDown" => resp["return"] = json!(self.wants_key(key)),
            "filterKeyUp" => resp["return"] = json!(false),
            "onKeyDown" => {
                resp["return"] = json!(self.wants_key(key));
                if self.wants_key(key) {
                    self.on_key_down(key, &mut resp);
                }
            }
            _ => {}
        }
        resp
    }

    fn refresh(&mut self) {
        self.cands = if self.input.is_empty() { Vec::new() } else { self.ime.candidates(&self.input) };
        self.page = 0;
        self.cursor = 0;
    }

    /// 选中页内第 `idx` 个候选;输入耗尽时返回应上屏的文字。
    fn select(&mut self, idx: usize) -> Option<String> {
        let c = self.page_items().get(idx)?.clone();
        let typed: String = self.input.drain(..c.consumed).collect();
        self.ime.learn(&typed, &c.text);
        self.pending.push_str(&c.text);
        if self.input.is_empty() {
            self.cands.clear();
            return Some(std::mem::take(&mut self.pending));
        }
        self.refresh();
        None
    }

    fn turn_page(&mut self, forward: bool) {
        let last = self.page_count().saturating_sub(1);
        self.page = if forward { (self.page + 1).min(last) } else { self.page.saturating_sub(1) };
        self.cursor = 0;
    }

    fn move_cursor(&mut self, down: bool) {
        let len = self.page_items().len();
        if down {
            if self.cursor + 1 < len {
                self.cursor += 1;
            } else if self.page + 1 < self.page_count() {
                self.turn_page(true);
            }
        } else if self.cursor > 0 {
            self.cursor -= 1;
        } else if self.page > 0 {
            self.turn_page(false);
            self.cursor = self.page_items().len().saturating_sub(1);
        }
    }

    fn flush_raw(&mut self) -> String {
        self.cands.clear();
        let text = format!("{}{}", self.pending, self.input);
        self.pending.clear();
        self.input.clear();
        text
    }

    fn on_key_down(&mut self, key: i64, resp: &mut Value) {
        let mut commit: Option<String> = None;
        match key {
            VK_SPACE if self.cands.is_empty() => commit = Some(self.flush_raw()),
            VK_SPACE => commit = self.select(self.cursor),
            VK_RETURN => commit = Some(self.flush_raw()),
            VK_ESCAPE => {
                self.flush_raw();
            }
            VK_UP => self.move_cursor(false),
            VK_DOWN => self.move_cursor(true),
            VK_PRIOR | VK_MINUS | VK_LBRACKET => self.turn_page(false),
            VK_NEXT | VK_EQUALS | VK_RBRACKET => self.turn_page(true),
            VK_BACK => {
                self.input.pop();
                if self.input.is_empty() {
                    commit = Some(self.flush_raw()).filter(|t| !t.is_empty());
                } else {
                    self.refresh();
                }
            }
            _ if digit_index(key).is_some() => commit = self.select(digit_index(key).unwrap()),
            _ => {
                self.input.push((key as u8 as char).to_ascii_lowercase());
                self.refresh();
            }
        }
        if let Some(text) = commit.filter(|t| !t.is_empty()) {
            resp["commitString"] = json!(text);
        }
        let shown = format!("{}{}", self.pending, self.input);
        resp["compositionString"] = json!(shown);
        resp["compositionCursor"] = json!(shown.chars().count());
        resp["showCandidates"] = json!(!self.cands.is_empty());
        if !self.cands.is_empty() {
            let texts: Vec<&str> = self.page_items().iter().map(|c| c.text.as_str()).collect();
            resp["candidateList"] = json!(texts);
            resp["candidateCursor"] = json!(self.cursor);
            resp["customizeUI"] = json!({ "candUseCursor": true, "candPerRow": PAGE_SIZE });
        }
    }
}

#[cfg(test)]
mod tests;
