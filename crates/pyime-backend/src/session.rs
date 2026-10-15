//! 单个前端连接的会话状态:拼音串、候选、分页与选词。
use crate::ime::Ime;
use pyime_engine::engine::Candidate;
use serde_json::{json, Value};
use crate::punct::{self, Mapped};
use std::rc::Rc;
use std::time::{Duration, Instant};

const VK_SHIFT_KEYS: [i64; 3] = [16, 160, 161];
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
const VK_LEFT: i64 = 37;
const VK_RIGHT: i64 = 39;
/// 模拟按键发出后这段时间内,收到的同一方向键视为自己注入的回声,不改变状态。
/// libIME 对同一次按键会先后调两次 filterKeyDown(OnTestKeyDown 与 OnKeyDown,后者防部分应用不触发前者),
/// 间隔微秒级;自动连发的退格间隔 ≥ 30ms。窗口内的重复退格视为同一次按键。
const BACK_DEDUP: Duration = Duration::from_millis(15);
const INJECT_ECHO: Duration = Duration::from_millis(300);
/// 候选横排,每页个数(与微信输入法一致)。
const PAGE_SIZE: usize = 6;
/// 候选窗字体与字号(PIME 只允许改这两项外观)。
const CAND_FONT: &str = "Microsoft YaHei UI";
const CAND_FONT_SIZE: i64 = 16;

/// 数字键 1-9(主键盘或小键盘)对应的页内下标。
fn digit_index(key: i64) -> Option<usize> {
    match key {
        49..=57 => Some((key - 49) as usize),
        97..=105 => Some((key - 97) as usize),
        _ => None,
    }
}

/// 取末尾至多 3 个字,作为下一个词的上文。
fn tail3(s: &str) -> String {
    let n = s.chars().count();
    s.chars().skip(n.saturating_sub(3)).collect()
}

/// 一次学习:(键入拼音, 词, 用到的模糊规则, 上文)。
type Learned = (String, String, u16, String);

/// 刚上屏的一次组词:记下它做过的学习。随后放行的退格删光它(含随之上屏的标点),
/// 或删过之后对同一串拼音改选了别的词,就把这些学习撤回。
struct Last {
    learned: Vec<Learned>,
    typed: String,
    /// 上屏的全部文字(组词结果 + 随后一起上屏的标点)。
    text: String,
    /// `text` 末尾的标点字符数;标点不在上文 `context` 里,删它时不回退上文。
    punct: usize,
    /// 上屏后放行的退格次数。
    backspaces: usize,
}

pub struct Session {
    ime: Rc<Ime>,
    /// 已选定、尚未上屏的文字(分段选词时累积)。
    pending: String,
    /// 尚未转换的拼音字母。
    input: String,
    cands: Vec<Candidate>,
    /// 本次组词累计键入并已选定的拼音,用于分段选完后学习整句。
    typed_all: String,
    /// 刚上屏文字的末尾几个字,作为下一次组词的上文(搭配加分用)。
    context: String,
    segments: usize,
    page: usize,
    /// 页内高亮下标。
    cursor: usize,
    /// 英文模式:所有按键放行给应用。
    english: bool,
    /// Shift 按下后尚未按过其他键,此时松开即视为"单击 Shift"。
    shift_pending: bool,
    /// 本次组词已做的学习。
    learned: Vec<Learned>,
    last: Option<Last>,
    /// 应用刚收到数字键,紧接着的 `.` `,` `:` 用半角。
    after_digit: bool,
    /// 双引号 / 单引号当前是否已开(下一次输出闭引号)。
    quote_open: [bool; 2],
    /// 自动补全、光标前方尚未越过的右符号(栈)。
    closers: Vec<char>,
    /// 最近一次自己注入方向键的时刻。
    injected: Option<Instant>,
    inject_echo: Duration,
    /// 上一次计数的放行退格时刻(抬键后清空),用于合并 libIME 的重复调用。
    back_counted: Option<Instant>,
    /// 本次组字中最长的拼音及其当时的候选前几名;放弃组字时写入日志。
    peak: Option<(String, Vec<String>)>,
}

impl Session {
    pub fn new(ime: Rc<Ime>) -> Session {
        Session { ime, pending: String::new(), input: String::new(), cands: Vec::new(), typed_all: String::new(), context: String::new(), segments: 0, page: 0, cursor: 0, english: false, shift_pending: false, learned: Vec::new(), last: None, after_digit: false, quote_open: [false; 2], closers: Vec::new(), injected: None, inject_echo: INJECT_ECHO, back_counted: None, peak: None }
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

    /// 该键是否由输入法接管(拼音串存在时,编辑/翻页/数字键也接管;Ctrl+数字不选词,Ctrl+Shift+数字删除学习)。
    fn wants_key(&self, key: i64, shift_down: bool, ctrl_down: bool) -> bool {
        if self.english {
            return false;
        }
        if (65..=90).contains(&key) {
            return self.composing() || !shift_down; // 未组词时 Shift+字母是大写,放行
        }
        let editing = matches!(
            key,
            VK_BACK | VK_RETURN | VK_ESCAPE | VK_SPACE | VK_UP | VK_DOWN | VK_PRIOR | VK_NEXT
                | VK_EQUALS | VK_MINUS | VK_LBRACKET | VK_RBRACKET
        ) || digit_index(key).is_some_and(|i| i < self.page_items().len() && (!ctrl_down || shift_down));
        editing && self.composing()
    }

    /// 供日志使用的当前状态摘要。
    pub fn debug_state(&self) -> String {
        format!(
            "english={} pending={:?} input={:?} cands={} page={} cursor={}",
            self.english, self.pending, self.input, self.cands.len(), self.page, self.cursor
        )
    }

    pub fn handle(&mut self, req: &Value) -> Value {
        let key = req["keyCode"].as_i64().unwrap_or(0);
        let shift_down = req["keyStates"][16].as_u64().unwrap_or(0) & 0x80 != 0;
        let ctrl_down = req["keyStates"][17].as_u64().unwrap_or(0) & 0x80 != 0;
        let mut resp = json!({ "success": true, "seqNum": req["seqNum"] });
        let is_shift = VK_SHIFT_KEYS.contains(&key);
        match req["method"].as_str().unwrap_or("") {
            m @ ("filterKeyDown" | "onKeyDown") => {
                self.shift_pending = is_shift;
                let mapped = self.punct_for(req, key);
                let wants = mapped.is_some() || self.wants_key(key, shift_down, ctrl_down);
                if !wants {
                    // 同一次按键可能先后以 filterKeyDown、onKeyDown 到达,退格只在前者计数
                    self.note_passthrough(key, shift_down, m == "filterKeyDown");
                }
                resp["return"] = json!(wants);
                if wants && m == "onKeyDown" {
                    self.on_key_down(key, mapped, ctrl_down, &mut resp);
                }
            }
            "filterKeyUp" | "onKeyUp" => {
                resp["return"] = json!(false);
                if key == VK_BACK {
                    self.back_counted = None;
                }
                if is_shift && std::mem::take(&mut self.shift_pending) && req["method"] == "filterKeyUp" {
                    self.toggle_english(&mut resp);
                }
            }
            _ => {}
        }
        resp
    }

    /// 单击 Shift:切换中/英文;切到英文时把未上屏的内容按拼音原样上屏。
    fn toggle_english(&mut self, resp: &mut Value) {
        self.english = !self.english;
        if self.english && self.composing() {
            let text = self.flush_raw();
            resp["commitString"] = json!(text);
            resp["compositionString"] = json!("");
            resp["compositionCursor"] = json!(0);
            resp["showCandidates"] = json!(false);
        }
    }

    /// 下一个词的上文:已选定未上屏的文字接在已上屏上文之后。
    fn prev(&self) -> String {
        tail3(&format!("{}{}", self.context, self.pending))
    }

    /// 应用收到的按键(输入法未接管)对"刚上屏"状态的影响:退格记为可能在撤销,
    /// Shift/Ctrl/Alt 与字母不影响,其他键说明用户接受了上屏结果。`count_back` 为假时退格不计数。
    fn note_passthrough(&mut self, key: i64, shift_down: bool, count_back: bool) {
        if matches!(key, VK_LEFT | VK_RIGHT) && self.injected.is_some_and(|t| t.elapsed() < self.inject_echo) {
            return;
        }
        let modifier = VK_SHIFT_KEYS.contains(&key) || matches!(key, 17 | 18 | 20);
        if !modifier {
            self.after_digit = !shift_down && matches!(key, 48..=57 | 96..=105);
        }
        // 光标被移动或内容被删除后,不再认为光标前方是自动补全的右符号
        if matches!(key, VK_BACK | VK_RETURN | VK_ESCAPE | 9 | 33..=40 | 46) {
            self.closers.clear();
        }
        if key == VK_BACK {
            if count_back && !self.back_counted.is_some_and(|t| t.elapsed() < BACK_DEDUP) {
                self.back_counted = Some(Instant::now());
                self.back_after_commit();
            }
        } else if !VK_SHIFT_KEYS.contains(&key) && !matches!(key, 17 | 18 | 65..=90) {
            self.last = None;
        }
    }

    /// 放行的一次退格:删到组词文字时同步回退上文;删光整次上屏则撤回其全部学习并清空上文。
    fn back_after_commit(&mut self) {
        let Some(l) = &mut self.last else { return };
        l.backspaces += 1;
        if l.backspaces > l.punct {
            self.context.pop();
        }
        if l.backspaces < l.text.chars().count() {
            return;
        }
        let Some(l) = self.last.take() else { return };
        crate::inputlog::retract(&l.learned);
        for (t, w, pairs, ctx) in &l.learned {
            self.ime.unlearn(t, w, *pairs, ctx);
        }
        self.context.clear();
    }

    /// 一次组词整句上屏:若是对刚被退格删掉的同一串拼音改选了别的词,撤回上次的学习;再记下这次。
    fn finish_commit(&mut self, typed: String, text: &str) {
        if let Some(prev) = self.last.take() {
            // 与整串重选比较时不含末尾标点
            let prev_text: String = {
                let n = prev.text.chars().count() - prev.punct;
                prev.text.chars().take(n).collect()
            };
            if prev.backspaces > 0 && prev.typed == typed && prev_text != text {
                crate::inputlog::retract(&prev.learned);
                for (t, w, pairs, ctx) in &prev.learned {
                    self.ime.unlearn(t, w, *pairs, ctx);
                }
            }
        }
        self.last = Some(Last { learned: std::mem::take(&mut self.learned), typed, text: text.to_string(), punct: 0, backspaces: 0 });
    }

    /// 组字串里未转换的拼音:按当前高亮候选的切分加 `'`(`de'dao'zhe`),没有候选时原样。
    fn display_input(&self) -> String {
        let Some(c) = self.page_items().get(self.cursor) else { return self.input.clone() };
        let mut cuts = c.cuts.clone();
        if c.consumed < self.input.len() {
            cuts.push(c.consumed);
        }
        let mut out = String::new();
        for (i, ch) in self.input.char_indices() {
            if i > 0 && cuts.contains(&i) {
                out.push('\'');
            }
            out.push(ch);
        }
        out
    }

    fn refresh(&mut self) {
        self.cands = if self.input.is_empty() { Vec::new() } else { self.ime.candidates(&self.input, &self.prev()) };
        self.page = 0;
        self.cursor = 0;
        if self.peak.as_ref().map_or(0, |p| p.0.len()) < self.input.len() {
            let top = self.cands.iter().take(crate::inputlog::TOP).map(|c| c.text.clone()).collect();
            self.peak = Some((self.input.clone(), top));
        }
    }

    /// 放弃组字(Esc,或退格删光且无上屏):把本次最长的拼音记为一次放弃。
    fn log_cancel(&mut self) {
        if let Some((input, top)) = self.peak.take() {
            crate::inputlog::cancel(&input, &top);
        }
    }

    /// 删除页内第 `idx` 个候选的个人学习,保持输入并刷新候选(页码仍有效则留在当前页)。
    fn forget(&mut self, idx: usize) {
        let Some(text) = self.page_items().get(idx).map(|c| c.text.clone()) else { return };
        let before: Vec<String> = self.cands.iter().map(|c| c.text.clone()).collect();
        self.ime.forget(&text);
        let page = self.page;
        self.refresh();
        self.page = page.min(self.page_count().saturating_sub(1));
        let noop = self.cands.iter().map(|c| &c.text).eq(before.iter());
        crate::inputlog::forget(&self.input, &text, noop);
    }

    /// 选中页内第 `idx` 个候选;输入耗尽时返回应上屏的文字。
    fn select(&mut self, idx: usize) -> Option<String> {
        let c = self.page_items().get(idx)?.clone();
        let tops: Vec<&str> = self.cands.iter().map(|c| c.text.as_str()).collect();
        let (input, ctx) = (self.input.clone(), self.prev());
        let index = self.page * PAGE_SIZE + idx;
        let typed: String = self.input.drain(..c.consumed).collect();
        crate::inputlog::select(&input, &ctx, &typed, &c.text, index, &tops);
        self.ime.learn(&typed, &c.text, c.pairs, &ctx);
        self.learned.push((typed.clone(), c.text.clone(), c.pairs, ctx));
        self.pending.push_str(&c.text);
        self.typed_all.push_str(&typed);
        self.segments += 1;
        if self.input.is_empty() {
            self.cands.clear();
            self.peak = None;
            let text = std::mem::take(&mut self.pending);
            if self.segments > 1 {
                self.ime.learn(&self.typed_all, &text, 0, ""); // 分段选完的整句也学
                self.learned.push((self.typed_all.clone(), text.clone(), 0, String::new()));
            }
            let typed_all = std::mem::take(&mut self.typed_all);
            self.finish_commit(typed_all, &text);
            self.segments = 0;
            self.context = tail3(&format!("{}{}", self.context, text));
            return Some(text);
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
        self.typed_all.clear();
        self.learned.clear();
        self.last = None;
        self.segments = 0;
        self.context.clear();
        self.peak = None;
        text
    }

    /// 中文状态下该键若是要转换的标点,返回其映射。组字时翻页用的 `[` `]` `-` `=` 不转换。
    fn punct_for(&self, req: &Value, key: i64) -> Option<Mapped> {
        let down = |i: usize| req["keyStates"][i].as_u64().unwrap_or(0) & 0x80 != 0;
        if self.english || down(17) || down(18) || (65..=90).contains(&key) {
            return None;
        }
        if self.composing() && matches!(key, VK_MINUS | VK_EQUALS | VK_LBRACKET | VK_RBRACKET) {
            return None;
        }
        punct::map(punct::typed_char(req)?)
    }

    /// 把当前组字整体上屏:逐段选高亮候选直到输入耗尽,没有候选则原样上屏拼音。
    fn commit_all(&mut self) -> String {
        if !self.composing() {
            return String::new();
        }
        let mut idx = self.cursor;
        for _ in 0..64 {
            if self.cands.is_empty() {
                break;
            }
            if let Some(t) = self.select(idx) {
                return t;
            }
            idx = 0;
        }
        self.flush_raw()
    }

    fn inject(&mut self, vk: i64) {
        self.ime.press(vk as u16);
        self.injected = Some(Instant::now());
    }

    /// 输出标点(组字时先上屏高亮候选);成对符号注入方向键让光标回到中间或跳过右符号。
    /// 同时上屏了组词时继续跟踪退格(标点计入删光阈值);注入了方向键(成对符号、跳过右符号)时光标已移动,不再跟踪。
    fn punctuate(&mut self, m: Mapped, was_digit: bool) -> Option<String> {
        let mut out = self.commit_all();
        let committed = out.chars().count();
        let mut moved = false;
        match m {
            Mapped::Fixed(c) => out.push(c),
            Mapped::Number { half, full } => out.push(if was_digit && out.is_empty() { half } else { full }),
            Mapped::Quote(double) => {
                let i = usize::from(!double);
                let (open, close) = if double { ('\u{201C}', '\u{201D}') } else { ('\u{2018}', '\u{2019}') };
                out.push(if self.quote_open[i] { close } else { open });
                self.quote_open[i] = !self.quote_open[i];
            }
            Mapped::Open { open, close } => {
                out.push(open);
                out.push(close);
                self.closers.push(close);
                self.inject(VK_LEFT);
                moved = true;
            }
            Mapped::Close(c) if self.closers.last() == Some(&c) => {
                self.closers.pop();
                self.inject(VK_RIGHT);
                moved = true;
            }
            Mapped::Close(c) => out.push(c),
        }
        let keep = committed > 0 && !moved;
        if !keep {
            self.last = None;
        } else if let Some(l) = &mut self.last {
            l.punct = out.chars().count() - committed;
            l.text = out.clone();
        }
        Some(out)
    }

    fn on_key_down(&mut self, key: i64, mapped: Option<Mapped>, ctrl_down: bool, resp: &mut Value) {
        let was_digit = std::mem::take(&mut self.after_digit);
        let mut commit: Option<String> = None;
        match key {
            _ if mapped.is_some() => commit = self.punctuate(mapped.unwrap(), was_digit),
            VK_SPACE if self.cands.is_empty() => commit = Some(self.flush_raw()),
            VK_SPACE => commit = self.select(self.cursor),
            VK_RETURN => commit = Some(self.flush_raw()),
            VK_ESCAPE => {
                self.log_cancel();
                self.flush_raw();
            }
            VK_UP => self.move_cursor(false),
            VK_DOWN => self.move_cursor(true),
            VK_PRIOR | VK_MINUS | VK_LBRACKET => self.turn_page(false),
            VK_NEXT | VK_EQUALS | VK_RBRACKET => self.turn_page(true),
            VK_BACK => {
                self.input.pop();
                if self.input.is_empty() {
                    if self.pending.is_empty() {
                        self.log_cancel();
                    }
                    commit = Some(self.flush_raw()).filter(|t| !t.is_empty());
                } else {
                    self.refresh();
                }
            }
            _ if ctrl_down && digit_index(key).is_some() => self.forget(digit_index(key).unwrap()),
            _ if digit_index(key).is_some() => commit = self.select(digit_index(key).unwrap()),
            _ => {
                self.input.push((key as u8 as char).to_ascii_lowercase());
                self.refresh();
            }
        }
        if let Some(text) = commit.filter(|t| !t.is_empty()) {
            resp["commitString"] = json!(text);
        }
        let shown = format!("{}{}", self.pending, self.display_input());
        resp["compositionString"] = json!(shown);
        resp["compositionCursor"] = json!(shown.chars().count());
        resp["showCandidates"] = json!(!self.cands.is_empty());
        if !self.cands.is_empty() {
            let texts: Vec<&str> = self.page_items().iter().map(|c| c.text.as_str()).collect();
            resp["candidateList"] = json!(texts);
            resp["candidateCursor"] = json!(self.cursor);
            resp["customizeUI"] = json!({ "candUseCursor": true, "candPerRow": PAGE_SIZE, "candFontName": CAND_FONT, "candFontSize": CAND_FONT_SIZE });
        }
    }
}

#[cfg(test)]
mod tests;
