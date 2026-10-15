//! 回放 `input-log.jsonl`:按日志顺序重放每次选词,统计选中项在候选里的名次,并边放边学习。
//! 用法:pyime-replay <pyime.dict> <input-log.jsonl> [--gram <pyime.gram>] [--fuzzy-penalty <x>] [-v]
use pyime_engine::dict::Dict;
use pyime_engine::engine::Engine;
use pyime_engine::fuzzy::FuzzyConfig;
use pyime_engine::gram::Gram;
use serde_json::Value;

fn main() -> Result<(), String> {
    let mut pos = Vec::new();
    let (mut gram, mut verbose, mut penalty) = (None, false, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--gram" => gram = args.next(),
            "--fuzzy-penalty" => penalty = args.next().and_then(|v| v.parse::<f64>().ok()),
            "-v" => verbose = true,
            _ => pos.push(a),
        }
    }
    let [dict, log] = pos.as_slice() else { return Err("用法: pyime-replay <pyime.dict> <input-log.jsonl> [--gram <pyime.gram>] [--fuzzy-penalty <x>] [-v]".into()) };
    let read = |p: &str| std::fs::read(p).map_err(|e| format!("读取 {p} 失败: {e}"));
    let mut engine = Engine::new(Dict::from_bytes(read(dict)?)?, FuzzyConfig::default());
    if let Some(g) = gram {
        engine.set_gram(Gram::from_bytes(read(&g)?)?);
    }
    if let Some(p) = penalty {
        engine.set_fuzzy_penalty(p);
    }
    let text = String::from_utf8(read(log)?).map_err(|e| e.to_string())?;
    let (mut total, mut hit1, mut hit3, mut hit5, mut missing) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(items) = v["retract"].as_array() {
            for it in items {
                if let (Some(t), Some(w), Some(p), Some(c)) = (it[0].as_str(), it[1].as_str(), it[2].as_u64(), it[3].as_str()) {
                    engine.unlearn(t, w, p as u16, c);
                }
            }
            continue;
        }
        if let Some(w) = v["forget"].as_str() {
            engine.forget(w);
            continue;
        }
        let (Some(input), Some(typed), Some(chosen)) = (v["input"].as_str(), v["typed"].as_str(), v["text"].as_str()) else { continue };
        let ctx = v["context"].as_str().unwrap_or("");
        let cands = engine.candidates_after(input, ctx);
        let rank = cands.iter().position(|c| c.text == chosen);
        total += 1;
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
        if let Some(r) = rank {
            engine.learn_fuzzy(cands[r].pairs);
        }
    }
    let pct = |n: usize| if total == 0 { 0.0 } else { n as f64 * 100.0 / total as f64 };
    println!("共 {total} 条;首选 {hit1} ({:.1}%) 前3 {hit3} ({:.1}%) 前5 {hit5} ({:.1}%) 未出现 {missing}", pct(hit1), pct(hit3), pct(hit5));
    Ok(())
}
