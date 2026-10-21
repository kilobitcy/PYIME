//! 回放 `input-log.jsonl`:按日志顺序重放每次选词,统计选中项在候选里的名次,并边放边学习。
//! 用法:pyime-replay <pyime.dict> <input-log.jsonl> [--gram <pyime.gram>] [--en <pyime.en>] [--fuzzy-penalty <x>] [--episodes-out <修改过程.tsv>] [--ranks-out <逐条名次.tsv>] [-v]
use pyime_engine::dict::Dict;
use pyime_engine::english::{parse_compiled, English};
use pyime_engine::engine::{Engine, Lesson};
use pyime_engine::fuzzy::FuzzyConfig;
use pyime_engine::gram::Gram;
use serde_json::Value;
use std::collections::HashMap;

fn main() -> Result<(), String> {
    let mut pos = Vec::new();
    let (mut gram, mut en, mut verbose, mut penalty, mut episodes_out, mut ranks_out) = (None, None, false, None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--gram" => gram = args.next(),
            "--en" => en = args.next(),
            "--fuzzy-penalty" => penalty = args.next().and_then(|v| v.parse::<f64>().ok()),
            "--episodes-out" => episodes_out = args.next(),
            "--ranks-out" => ranks_out = args.next(),
            "-v" => verbose = true,
            _ => pos.push(a),
        }
    }
    let [dict, log] = pos.as_slice() else { return Err("用法: pyime-replay <pyime.dict> <input-log.jsonl> [--gram <pyime.gram>] [--en <pyime.en>] [--fuzzy-penalty <x>] [--episodes-out <修改过程.tsv>] [--ranks-out <逐条名次.tsv>] [-v]".into()) };
    let read = |p: &str| std::fs::read(p).map_err(|e| format!("读取 {p} 失败: {e}"));
    let mut engine = Engine::new(Dict::from_bytes(read(dict)?)?, FuzzyConfig::default());
    if let Some(g) = gram {
        engine.set_gram(Gram::from_bytes(read(&g)?)?);
    }
    if let Some(p) = penalty {
        engine.set_fuzzy_penalty(p);
    }
    if let Some(p) = en {
        let text = String::from_utf8(read(&p)?).map_err(|e| e.to_string())?;
        engine.set_english(English::new(parse_compiled(&text)));
    }
    let text = String::from_utf8(read(log)?).map_err(|e| e.to_string())?;
    let (mut total, mut hit1, mut hit3, mut hit5, mut missing) = (0usize, 0usize, 0usize, 0usize, 0usize);
    // 每次选词顺带学的 lessons,按 (typed, text, 上文) 记最近一次,供 retract 连带撤销(日志里不含 lessons)
    let mut taught: HashMap<(String, String, String), Vec<Lesson>> = HashMap::new();
    let mut episodes = Episodes::default();
    // 逐条名次(行号⇥拼音⇥上屏⇥名次,0 = 未出现),供调参前后逐条比对
    let mut ranks = String::new();
    for (no, line) in text.lines().enumerate() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let ts = v["ts"].as_u64().unwrap_or(0);
        if let Some(p) = v["edit"].as_str() {
            episodes.fail(p, ts, false);
            continue;
        }
        if let Some(p) = v["cancel"].as_str() {
            episodes.cancel(p, ts);
            continue;
        }
        if let Some(items) = v["retract"].as_array() {
            // 分段上屏会撤回多项,最长的拼音即整次组字
            if let Some(p) = items.iter().filter_map(|it| it[0].as_str()).max_by_key(|t| t.len()) {
                episodes.fail(p, ts, true);
            }
            for it in items {
                if let (Some(t), Some(w), Some(p), Some(c)) = (it[0].as_str(), it[1].as_str(), it[2].as_u64(), it[3].as_str()) {
                    engine.unlearn(t, w, p as u16, c);
                    if let Some(ls) = taught.remove(&(t.to_string(), w.to_string(), c.to_string())) {
                        engine.unlearn_lessons(&ls);
                    }
                }
            }
            continue;
        }
        if let Some(w) = v["forget"].as_str() {
            engine.forget(w);
            continue;
        }
        // 回车原样上屏:按同样条件重建英文回车学习
        if let Some(s) = v["enter_raw"].as_str() {
            engine.learn_raw(s);
            continue;
        }
        let (Some(input), Some(typed), Some(chosen)) = (v["input"].as_str(), v["typed"].as_str(), v["text"].as_str()) else { continue };
        let ctx = v["context"].as_str().unwrap_or("");
        episodes.select(&engine, input, typed, chosen, ctx, ts);
        let cands = engine.candidates_after(input, ctx);
        let rank = cands.iter().position(|c| c.text == chosen);
        total += 1;
        ranks += &format!("{}\t{input}\t{chosen}\t{}\n", no + 1, rank.map_or(0, |r| r + 1));
        match rank {
            Some(r) => {
                hit1 += usize::from(r < 1);
                hit3 += usize::from(r < 3);
                hit5 += usize::from(r < 5);
                if verbose && r >= 3 {
                    println!("{input} → {chosen}: 名次 {}", r + 1);
                }
            }
            None => {
                missing += 1;
                if verbose {
                    println!("{input} → {chosen}: 候选里没有");
                }
            }
        }
        engine.learn(typed, chosen);
        engine.learn_next(ctx, chosen);
        let lessons = match rank {
            Some(r) => {
                engine.learn_fuzzy(cands[r].pairs);
                cands[r].lessons(&input[..cands[r].consumed], ctx)
            }
            None => Vec::new(),
        };
        engine.learn_lessons(&lessons);
        taught.insert((typed.to_string(), chosen.to_string(), ctx.to_string()), lessons);
    }
    let pct = |n: usize| if total == 0 { 0.0 } else { n as f64 * 100.0 / total as f64 };
    episodes.close();
    println!("{}", episodes.report());
    if let Some(p) = episodes_out {
        std::fs::write(&p, episodes.rows.join("\n") + "\n").map_err(|e| format!("写入 {p} 失败: {e}"))?;
    }
    if let Some(p) = ranks_out {
        std::fs::write(&p, ranks).map_err(|e| format!("写入 {p} 失败: {e}"))?;
    }
    println!("共 {total} 条;首选 {hit1} ({:.1}%) 前3 {hit3} ({:.1}%) 前5 {hit5} ({:.1}%) 未出现 {missing}", pct(hit1), pct(hit3), pct(hit5));
    Ok(())
}

/// 上屏后多久没再撤回,即认为修改过程结束(毫秒)。
const EPISODE_GAP: u64 = 15_000;
const CLASSES: [&str; 3] = ["改选", "改拼音", "改主意"];

/// 正在拼的一次组字:完整拼音、上文、已选文字、剩余拼音、首段选词前在首次尝试拼音下的候选。
struct Comp {
    input: String,
    ctx: String,
    text: String,
    rest: String,
    cands: Vec<String>,
}

/// 修改过程(见 2026-10-20 规范 3.2):从首次失败的尝试(edit/cancel/被撤回的上屏)起,
/// 到某次上屏后 15 秒内无撤回为止;最终上屏文字视为用户意图。
/// 失败后分几次上屏的(删回 `qin` 选"请"、再打 `queren`),累积到拼音不短于首次尝试为止。
#[derive(Default)]
struct Episodes {
    /// 首次尝试的拼音与最近一次失败的时刻。
    first: Option<(String, u64)>,
    comp: Option<Comp>,
    /// 失败后累积的上屏(待定是否最终)与最近一次上屏的时刻。
    done: Option<(Comp, u64)>,
    counts: [usize; 3],
    hit1: usize,
    hit3: usize,
    /// 前两类导出行:首次拼音⇥最终文字⇥名次(0 = 未召回)⇥类别⇥上文。
    rows: Vec<String>,
}

impl Episodes {
    /// 一次失败的尝试:`retract` 为真表示撤回了刚才的上屏,否则为组字中删改或放弃。
    fn fail(&mut self, pinyin: &str, ts: u64, retract: bool) {
        if retract {
            self.done = None;
        } else if self.complete() {
            self.close();
        }
        match &mut self.first {
            Some((_, t)) => *t = ts,
            None => self.first = Some((pinyin.to_string(), ts)),
        }
    }

    /// 放弃组字:当前组字作废。
    fn cancel(&mut self, pinyin: &str, ts: u64) {
        self.comp = None;
        self.fail(pinyin, ts, false);
    }

    /// 一次选词(须在回放学习之前调用,候选才不含本次学习)。
    fn select(&mut self, engine: &Engine, input: &str, typed: &str, text: &str, ctx: &str, ts: u64) {
        if self.done.as_ref().is_some_and(|d| ts > d.1 + EPISODE_GAP) {
            self.close();
        }
        if self.comp.as_ref().is_none_or(|c| c.rest != input) {
            if self.complete() {
                self.close();
            }
            if self.first.as_ref().is_some_and(|f| ts > f.1 + EPISODE_GAP) {
                self.first = None; // 失败后久未上屏:放弃了,不与之后的输入相连
            }
            let cands = self.first.as_ref().map_or(Vec::new(), |(p, _)| engine.candidates_after(p, ctx).into_iter().map(|c| c.text).collect());
            self.comp = Some(Comp { input: input.to_string(), ctx: ctx.to_string(), text: String::new(), rest: input.to_string(), cands });
        }
        let Some(c) = &mut self.comp else { return };
        c.text.push_str(text);
        c.rest = input.get(typed.len()..).unwrap_or("").to_string();
        if !c.rest.is_empty() {
            return;
        }
        let c = self.comp.take().unwrap();
        match &mut self.done {
            Some((d, t)) if self.first.is_some() => {
                d.input.push_str(&c.input);
                d.text.push_str(&c.text);
                *t = ts;
            }
            _ => self.done = Some((c, ts)),
        }
    }

    /// 累积的上屏已不短于首次尝试(或并无首次尝试):再有新输入即可结束本过程。
    fn complete(&self) -> bool {
        self.done.as_ref().is_some_and(|(d, _)| self.first.as_ref().is_none_or(|(p, _)| d.input.len() >= p.len()))
    }

    /// 结束进行中的过程:有首次尝试且有最终上屏时分类计数。
    fn close(&mut self) {
        let Some((c, _)) = self.done.take() else { return };
        let Some((first, _)) = self.first.take() else { return };
        let rank = c.cands.iter().position(|t| *t == c.text);
        let class = if first == c.input { 0 } else if rank.is_some() { 1 } else { 2 };
        self.counts[class] += 1;
        if class < 2 {
            self.hit1 += usize::from(rank == Some(0));
            self.hit3 += usize::from(rank.is_some_and(|r| r < 3));
            self.rows.push(format!("{first}\t{}\t{}\t{}\t{}", c.text, rank.map_or(0, |r| r + 1), CLASSES[class], c.ctx));
        }
    }

    fn report(&self) -> String {
        let n = self.counts[0] + self.counts[1];
        let pct = |k: usize| if n == 0 { 0.0 } else { k as f64 * 100.0 / n as f64 };
        let parts: Vec<String> = CLASSES.iter().zip(self.counts).map(|(c, k)| format!("{c} {k}")).collect();
        format!("修改过程 {} 个:{};前两类首选 {} ({:.1}%) 前3 {} ({:.1}%)", self.counts.iter().sum::<usize>(), parts.join(" "), self.hit1, pct(self.hit1), self.hit3, pct(self.hit3))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyime_engine::dict::DictBuilder;

    fn engine() -> Engine {
        let mut b = DictBuilder::default();
        for (k, w, n) in [("qin", "亲", 5000), ("qing", "请", 4000), ("que ren", "确认", 3000), ("zai", "在", 5000), ("zai", "再", 4000), ("hou xuan", "候选", 3000)] {
            b.add(k, w, n);
        }
        Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default())
    }

    #[test]
    fn 撤回后同拼音改选为改选_记名次() {
        let (e, mut ep) = (engine(), Episodes::default());
        ep.select(&e, "zai", "zai", "在", "", 0);
        ep.fail("zai", 1000, true);
        ep.select(&e, "zai", "zai", "再", "", 2000);
        ep.close();
        assert_eq!(ep.counts, [1, 0, 0]);
        assert_eq!(ep.rows, ["zai\t再\t2\t改选\t"]);
    }

    #[test]
    fn 删回前段分次上屏_累积到不短于首次拼音() {
        let (e, mut ep) = (engine(), Episodes::default());
        ep.fail("qinqueren", 0, false);
        ep.select(&e, "qin", "qin", "请", "", 1000);
        ep.select(&e, "queren", "queren", "确认", "请", 2000);
        ep.close();
        assert_eq!(ep.counts, [1, 0, 0]);
        assert!(ep.rows[0].starts_with("qinqueren\t请确认\t"), "{:?}", ep.rows);
    }

    #[test]
    fn 撤回后改打无关内容为改主意_不导出() {
        let (e, mut ep) = (engine(), Episodes::default());
        ep.select(&e, "zai", "zai", "在", "", 0);
        ep.fail("zai", 1000, true);
        ep.select(&e, "houxuan", "houxuan", "候选", "", 2000);
        ep.close();
        assert_eq!(ep.counts, [0, 0, 1]);
        assert!(ep.rows.is_empty());
    }

    #[test]
    fn 失败后久未上屏不与之后的输入相连() {
        let (e, mut ep) = (engine(), Episodes::default());
        ep.cancel("qinqueren", 0);
        ep.select(&e, "zai", "zai", "在", "", EPISODE_GAP + 1);
        ep.close();
        assert_eq!(ep.counts, [0, 0, 0]);
    }
}
