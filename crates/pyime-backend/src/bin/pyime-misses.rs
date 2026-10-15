//! 错例报告:读 `input-log.jsonl`,只用日志里当时显示的候选(不加载词典),找出首选不对的上屏,
//! 输出可贴进 `regression.rs` 的用例与"疑似找不到"清单。仅本机开发用,不进安装包。
//! 用法:pyime-misses <input-log.jsonl>
use serde_json::Value;

/// 纠错配对:撤回后多久内的重打算数(毫秒)。
const RETYPE_MS: u64 = 60_000;
/// 放弃输入后多久内的上屏算作改打(毫秒)。
const CANCEL_MS: u64 = 30_000;

/// 一次上屏:连续分段选词合成的组。
struct Group {
    /// 各段 typed 拼接。
    typed: String,
    text: String,
    first_input: String,
    first_top: Option<String>,
    first_index: u64,
    segs: usize,
    context: String,
    /// 首段时刻(毫秒)。
    ts: u64,
    /// 被撤回的误选组,不再计入 ② ③。
    wrong: bool,
    /// 已作为纠错期望累加过,记下该用例的 (拼音, 期望);不再计入 ② ③,被撤回时用来回收该用例。
    used: Option<(String, String)>,
    /// 部分选词后按 Esc 放弃,文字未上屏。
    dropped: bool,
}

impl Group {
    /// 输入被选完:没有被退格删掉的尾巴,分段链条也没有断。
    fn complete(&self) -> bool {
        self.typed == self.first_input
    }
}

enum Event {
    Group(usize),
    /// 撤回:取最后一项(分段上屏时为整句)。
    Retract { typed: String, text: String, ts: u64 },
    Cancel { input: String, top: Vec<String>, ts: u64 },
}

struct Case {
    /// 1 纠错、2 拆开选、3 选了非首位。
    kind: u8,
    input: String,
    expect: String,
    note: String,
    count: usize,
}

#[derive(Default)]
struct Report {
    lines: usize,
    versions: Vec<String>,
    cases: Vec<Case>,
    suspects: Vec<String>,
}

impl Report {
    /// 相同 (类型, 拼音, 期望) 合并计数,注释保留首次的。
    fn add(&mut self, kind: u8, input: &str, expect: &str, note: String) {
        if let Some(c) = self.cases.iter_mut().find(|c| c.kind == kind && c.input == input && c.expect == expect) {
            c.count += 1;
        } else {
            self.cases.push(Case { kind, input: input.to_string(), expect: expect.to_string(), note, count: 1 });
        }
    }

    /// 回收一次纠错用例的计数(该期望随后又被撤回),减到 0 则移除。
    fn retract_case(&mut self, input: &str, expect: &str) {
        if let Some(p) = self.cases.iter().position(|c| c.kind == 1 && c.input == input && c.expect == expect) {
            self.cases[p].count -= 1;
            if self.cases[p].count == 0 {
                self.cases.remove(p);
            }
        }
    }
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else { return Err("用法: pyime-misses <input-log.jsonl>".into()) };
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取 {path} 失败: {e}"))?;
    let lines: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    print!("{}", render(&analyze(&lines)));
    Ok(())
}

/// 把日志行按顺序拆成上屏组与撤回/放弃事件,顺带统计条数与版本。
fn parse(lines: &[Value], report: &mut Report) -> (Vec<Group>, Vec<Event>) {
    let (mut groups, mut events) = (Vec::<Group>::new(), Vec::new());
    // 正在等待下一段的组及其剩余拼音
    let mut open: Option<(usize, String)> = None;
    for v in lines {
        report.lines += 1;
        let ver = v["v"].as_str().unwrap_or("旧版").to_string();
        if !report.versions.contains(&ver) {
            report.versions.push(ver);
        }
        let ts = v["ts"].as_u64().unwrap_or(0);
        if let Some(items) = v["retract"].as_array() {
            open = None;
            if let Some(last) = items.last() {
                events.push(Event::Retract { typed: str_of(&last[0]), text: str_of(&last[1]), ts });
            }
            continue;
        }
        if let Some(input) = v["cancel"].as_str() {
            if let Some((g, _)) = open {
                groups[g].dropped = true;
            }
            open = None;
            let top = v["top"].as_array().map(|a| a.iter().map(str_of).collect()).unwrap_or_default();
            events.push(Event::Cancel { input: input.to_string(), top, ts });
            continue;
        }
        let (Some(input), Some(typed), Some(text)) = (v["input"].as_str(), v["typed"].as_str(), v["text"].as_str()) else { continue };
        let idx = match &open {
            Some((g, rest)) if rest == input => {
                let g = *g;
                groups[g].typed.push_str(typed);
                groups[g].text.push_str(text);
                groups[g].segs += 1;
                g
            }
            _ => {
                groups.push(Group {
                    typed: typed.to_string(),
                    text: text.to_string(),
                    first_input: input.to_string(),
                    first_top: v["top"][0].as_str().map(String::from),
                    first_index: v["index"].as_u64().unwrap_or(0),
                    segs: 1,
                    context: str_of(&v["context"]),
                    ts,
                    wrong: false,
                    used: None,
                    dropped: false,
                });
                events.push(Event::Group(groups.len() - 1));
                groups.len() - 1
            }
        };
        open = input.strip_prefix(typed).filter(|r| !r.is_empty()).map(|r| (idx, r.to_string()));
    }
    (groups, events)
}

/// 撤回 (r, w):向前找误选组,再把其后的上屏(撤回后的只算 60 秒内)按序累加,
/// 拼音恰好凑成 r 时累加文字即期望;凑不上则列入疑似清单。
fn retract(report: &mut Report, groups: &mut [Group], events: &[Event], i: usize, r: &str, w: &str, ts: u64) {
    // 后端的撤回只针对最近一次上屏;整串重选时误选组是倒数第二个,故只看最近 2 个组
    let found = (0..i)
        .rev()
        .filter_map(|j| match events[j] {
            Event::Group(g) => Some((j, g)),
            _ => None,
        })
        .take(2)
        .find(|&(_, g)| !groups[g].wrong && groups[g].typed == r && groups[g].text == w);
    let Some((pos, wrong)) = found else { return };
    groups[wrong].wrong = true;
    if let Some((r0, e0)) = groups[wrong].used.take() {
        report.retract_case(&r0, &e0);
    }
    let (mut typed, mut text, mut used) = (String::new(), String::new(), Vec::new());
    let mut first: Option<usize> = None;
    for (j, e) in events.iter().enumerate().skip(pos + 1) {
        let Event::Group(g) = *e else { continue };
        if groups[g].dropped {
            continue;
        }
        if j > i && groups[g].ts.saturating_sub(ts) > RETYPE_MS {
            break;
        }
        first.get_or_insert(g);
        typed.push_str(&groups[g].typed);
        text.push_str(&groups[g].text);
        used.push(g);
        if !r.starts_with(typed.as_str()) {
            break;
        }
        if typed == r {
            if text != w {
                for g in used {
                    groups[g].used = Some((r.to_string(), text.clone()));
                }
                report.add(1, r, &text, format!("误选 {w}"));
            }
            return;
        }
    }
    let after = match first {
        Some(g) => format!("改打 {} 得 {}", groups[g].typed, groups[g].text),
        None => "之后 60 秒内没有重打".to_string(),
    };
    report.suspects.push(format!("{r} 误选 {w} → {after}"));
}

/// 放弃输入后 30 秒内的第一次上屏列为疑似找不到;其间又有有效放弃则以后者为准。
/// 拼音不足 2 个字母或没有候选的放弃视为误敲,忽略。
fn cancel(report: &mut Report, groups: &[Group], events: &[Event], i: usize, input: &str, top: &[String], ts: u64) {
    let valid = |s: &str, t: &[String]| s.len() >= 2 && !t.is_empty();
    if !valid(input, top) {
        return;
    }
    for e in &events[i + 1..] {
        match e {
            Event::Cancel { input: s, top: t, .. } if valid(s.as_str(), t.as_slice()) => return,
            Event::Group(g) => {
                let g = &groups[*g];
                if g.dropped {
                    continue;
                }
                if g.ts.saturating_sub(ts) <= CANCEL_MS {
                    report.suspects.push(format!("{input}(候选: {})→ 改打 {} 得 {}", top.join(" "), g.typed, g.text));
                }
                return;
            }
            _ => {}
        }
    }
}

fn analyze(lines: &[Value]) -> Report {
    let mut report = Report::default();
    let (mut groups, events) = parse(lines, &mut report);
    for (i, e) in events.iter().enumerate() {
        match e {
            Event::Retract { typed, text, ts } => retract(&mut report, &mut groups, &events, i, typed, text, *ts),
            Event::Cancel { input, top, ts } => cancel(&mut report, &groups, &events, i, input, top, *ts),
            Event::Group(_) => {}
        }
    }
    for g in groups.iter().filter(|g| !g.wrong && g.used.is_none() && g.complete()) {
        if g.segs > 1 && g.first_top.as_deref() != Some(g.text.as_str()) {
            report.add(2, &g.typed, &g.text, format!("{} 段", g.segs));
        } else if g.segs == 1 && g.first_index > 0 {
            report.add(3, &g.typed, &g.text, format!("上文: {}", g.context));
        }
    }
    // 稳定排序:同类内按次数降序,同次数保持首次出现顺序
    report.cases.sort_by_key(|c| (c.kind, std::cmp::Reverse(c.count)));
    report
}

/// 报告文本:统计行、三类用例(可直接贴进 regression.rs)、疑似清单。
fn render(r: &Report) -> String {
    let mut out = format!("共 {} 条日志;版本: {}\n", r.lines, r.versions.join(", "));
    for (kind, title) in [(1, "① 纠错"), (2, "② 拆开选"), (3, "③ 选了非首位")] {
        out.push_str(&format!("== {title} ==\n"));
        let cases: Vec<&Case> = r.cases.iter().filter(|c| c.kind == kind).collect();
        if cases.is_empty() {
            out.push_str("    (无)\n");
        }
        for c in cases {
            out.push_str(&format!("    ({:?}, {:?}, 1), // ×{} {}\n", c.input, c.expect, c.count, c.note));
        }
    }
    out.push_str("== 疑似找不到 ==\n");
    if r.suspects.is_empty() {
        out.push_str("    (无)\n");
    }
    for s in &r.suspects {
        out.push_str(&format!("    {s}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn sel(input: &str, typed: &str, text: &str, index: u64, top: &[&str], ts: u64) -> Value {
        json!({"v": "0.1.9", "ts": ts, "input": input, "context": "", "typed": typed, "text": text, "index": index, "top": top})
    }

    /// retract 行:items 依次为各段,分段上屏时末项为整句。
    fn ret(items: &[(&str, &str)], ts: u64) -> Value {
        let items: Vec<Value> = items.iter().map(|(t, w)| json!([t, w, 0, ""])).collect();
        json!({"v": "0.1.9", "ts": ts, "retract": items})
    }

    fn can(input: &str, top: &[&str], ts: u64) -> Value {
        json!({"v": "0.1.9", "ts": ts, "cancel": input, "top": top})
    }

    fn cases(r: &Report) -> Vec<(u8, &str, &str, usize)> {
        r.cases.iter().map(|c| (c.kind, c.input.as_str(), c.expect.as_str(), c.count)).collect()
    }

    #[test]
    fn 删光后分两次重打_得到纠错用例() {
        let r = analyze(&[
            sel("qinjixu", "qinjixu", "亲继续", 0, &["亲继续", "请继续"], 1000),
            ret(&[("qinjixu", "亲继续")], 2000),
            sel("qin", "qin", "请", 0, &["请", "亲"], 3000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 4000),
        ]);
        assert_eq!(cases(&r), [(1, "qinjixu", "请继续", 1)]);
        assert_eq!(r.cases[0].note, "误选 亲继续");
        assert!(r.suspects.is_empty());
    }

    #[test]
    fn 分段上屏后删光_以整句项为准() {
        let r = analyze(&[
            sel("qinjixu", "qin", "亲", 0, &["亲继续"], 1000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
            ret(&[("qin", "亲"), ("jixu", "继续"), ("qinjixu", "亲继续")], 2000),
            sel("qinjixu", "qinjixu", "请继续", 1, &["亲继续", "请继续"], 3000),
        ]);
        assert_eq!(cases(&r), [(1, "qinjixu", "请继续", 1)]);
    }

    #[test]
    fn 整串重选改词_撤回写在新选词之后() {
        let r = analyze(&[
            sel("nihao", "nihao", "你号", 0, &["你号", "你好"], 1000),
            sel("nihao", "nihao", "你好", 1, &["你号", "你好"], 3000),
            ret(&[("nihao", "你号")], 3000),
        ]);
        assert_eq!(cases(&r), [(1, "nihao", "你好", 1)]);
    }

    #[test]
    fn 删光后改打模糊拼音_进疑似清单() {
        let r = analyze(&[
            sel("qinjixu", "qinjixu", "亲继续", 0, &["亲继续"], 1000),
            ret(&[("qinjixu", "亲继续")], 2000),
            sel("qingjixu", "qingjixu", "请继续", 0, &["请继续"], 3000),
        ]);
        assert!(r.cases.is_empty());
        assert_eq!(r.suspects, ["qinjixu 误选 亲继续 → 改打 qingjixu 得 请继续"]);
    }

    #[test]
    fn 撤回后超过60秒才重打_不出用例() {
        let r = analyze(&[
            sel("qinjixu", "qinjixu", "亲继续", 0, &["亲继续"], 1000),
            ret(&[("qinjixu", "亲继续")], 2000),
            sel("qinjixu", "qinjixu", "请继续", 0, &["请继续"], 2000 + 60_001),
        ]);
        assert!(r.cases.is_empty());
        assert_eq!(r.suspects, ["qinjixu 误选 亲继续 → 之后 60 秒内没有重打"]);
    }

    #[test]
    fn 拆开选且首选不是整句() {
        let r = analyze(&[
            sel("qinjixu", "qin", "请", 1, &["亲继续", "请"], 1000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
        ]);
        assert_eq!(cases(&r), [(2, "qinjixu", "请继续", 1)]);
        assert_eq!(r.cases[0].note, "2 段");
    }

    #[test]
    fn 拆开选但首选就是整句_不出() {
        let r = analyze(&[
            sel("qinjixu", "qin", "请", 1, &["请继续", "请"], 1000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
        ]);
        assert!(r.cases.is_empty());
    }

    #[test]
    fn 末尾多余字母_不出拆开选() {
        let r = analyze(&[
            sel("nihaou", "ni", "你", 1, &["你好", "你"], 1000),
            sel("haou", "hao", "好", 0, &["好"], 1100),
        ]);
        assert!(r.cases.is_empty());
    }

    #[test]
    fn 链条断裂的首行_不出非首位() {
        let r = analyze(&[
            sel("qinji", "qin", "请", 2, &["亲吉", "亲", "请"], 1000),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
        ]);
        assert!(r.cases.is_empty());
    }

    #[test]
    fn 两段之间夹forget仍归一组() {
        let r = analyze(&[
            sel("qinjixu", "qin", "请", 1, &["亲继续", "请"], 1000),
            json!({"v": "0.1.9", "ts": 1050, "forget": "亲继续", "input": "jixu", "noop": false}),
            sel("jixu", "jixu", "继续", 0, &["继续"], 1100),
        ]);
        assert_eq!(cases(&r), [(2, "qinjixu", "请继续", 1)]);
    }

    #[test]
    fn 单段非首位() {
        let r = analyze(&[sel("zai", "zai", "在", 1, &["再", "在"], 1000)]);
        assert_eq!(cases(&r), [(3, "zai", "在", 1)]);
        assert_eq!(r.cases[0].note, "上文: ");
    }

    #[test]
    fn 被撤回的上屏不出非首位() {
        let r = analyze(&[
            sel("zai", "zai", "在", 1, &["再", "在"], 1000),
            ret(&[("zai", "在")], 2000),
        ]);
        assert!(r.cases.is_empty());
    }

    #[test]
    fn 相同用例合并计数() {
        let r = analyze(&[
            sel("zai", "zai", "在", 1, &["再", "在"], 1000),
            sel("zai", "zai", "在", 1, &["再", "在"], 2000),
        ]);
        assert_eq!(cases(&r), [(3, "zai", "在", 2)]);
    }

    #[test]
    fn 放弃后30秒内上屏_列入疑似() {
        let r = analyze(&[
            can("chelue", &["车路饿", "车"], 1000),
            sel("celve", "celve", "策略", 0, &["策略"], 5000),
        ]);
        assert_eq!(r.suspects, ["chelue(候选: 车路饿 车)→ 改打 celve 得 策略"]);
    }

    #[test]
    fn 放弃超过30秒_不配对() {
        let r = analyze(&[
            can("chelue", &["车路饿"], 1000),
            sel("celve", "celve", "策略", 0, &["策略"], 1000 + 30_001),
        ]);
        assert!(r.suspects.is_empty());
    }

    #[test]
    fn 单字母放弃视为误敲() {
        let r = analyze(&[
            can("chelue", &["车路饿"], 1000),
            can("c", &["才"], 2000),
            sel("celve", "celve", "策略", 0, &["策略"], 3000),
        ]);
        assert_eq!(r.suspects, ["chelue(候选: 车路饿)→ 改打 celve 得 策略"]);
    }

    #[test]
    fn 报告列出版本与用例() {
        let r = analyze(&[
            json!({"ts": 1, "input": "ni", "context": "", "typed": "ni", "text": "你", "index": 0, "top": ["你"]}),
            sel("zai", "zai", "在", 1, &["再", "在"], 1000),
        ]);
        assert_eq!(r.lines, 2);
        assert_eq!(r.versions, ["旧版", "0.1.9"]);
        let out = render(&r);
        assert!(out.starts_with("共 2 条日志;版本: 旧版, 0.1.9\n"), "{out}");
        assert!(out.contains("== ① 纠错 ==\n    (无)\n"), "{out}");
        assert!(out.contains("    (\"zai\", \"在\", 1), // ×1 上文: \n"), "{out}");
        assert!(out.contains("== 疑似找不到 ==\n    (无)\n"), "{out}");
    }

    #[test]
    fn 连续两次整串重选_以最后一次为准() {
        let r = analyze(&[
            sel("nihao", "nihao", "你号", 0, &["你号", "泥好", "你好"], 1000),
            sel("nihao", "nihao", "泥好", 1, &["你号", "泥好", "你好"], 2000),
            ret(&[("nihao", "你号")], 2000),
            sel("nihao", "nihao", "你好", 2, &["你号", "泥好", "你好"], 3000),
            ret(&[("nihao", "泥好")], 3000),
        ]);
        assert_eq!(cases(&r), [(1, "nihao", "你好", 1)]);
        assert_eq!(r.cases[0].note, "误选 泥好");
        assert!(r.suspects.is_empty());
    }

    #[test]
    fn 撤回只匹配最近的上屏_不回溯旧组() {
        let r = analyze(&[
            sel("nihao", "nihao", "你号", 0, &["你号"], 1000),
            sel("wo", "wo", "我", 0, &["我"], 2000),
            sel("zai", "zai", "在", 0, &["在"], 3000),
            ret(&[("nihao", "你号")], 4000),
        ]);
        assert!(r.cases.is_empty());
        assert!(r.suspects.is_empty());
    }

    #[test]
    fn esc丢弃的部分选词不参与纠错累加() {
        let r = analyze(&[
            sel("qinjixu", "qinjixu", "亲继续", 0, &["亲继续"], 1000),
            ret(&[("qinjixu", "亲继续")], 2000),
            sel("qinjixu", "qin", "请", 1, &["亲继续", "请"], 3000),
            can("qinjixu", &["亲继续", "请"], 3500),
            sel("jixu", "jixu", "继续", 0, &["继续"], 4000),
        ]);
        assert!(r.cases.is_empty());
        assert_eq!(r.suspects, ["qinjixu 误选 亲继续 → 改打 jixu 得 继续", "qinjixu(候选: 亲继续 请)→ 改打 jixu 得 继续"]);
    }

    #[test]
    fn esc丢弃的部分选词不参与放弃配对() {
        let r = analyze(&[
            can("chelue", &["车路饿"], 1000),
            sel("celve", "ce", "策", 0, &["策略", "策"], 2000),
            can("celve", &["策略", "策"], 2500),
            sel("celve", "celve", "策略", 0, &["策略"], 3000),
        ]);
        assert_eq!(r.suspects, ["celve(候选: 策略 策)→ 改打 celve 得 策略"]);
    }
}
