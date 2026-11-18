//! 留一法整句评测(忽略):对 user.txt 里每条"拼音 ≥8 字母、词 ≥3 字"的选择,
//! 先忘掉这个词(保留其余学习),再看它在该拼音候选里的名次。衡量"没学过整句时能否给出用户要的句子"。
//! PYIME_DICT=… PYIME_USER=user.txt cargo test -q -p pyime-engine --release --test loo_bench -- --ignored --nocapture
//! 加 `PYIME_RANKS_OUT=a.tsv` 存逐例名次,`PYIME_RANKS_BASE=a.tsv` 与之逐例比对。
use pyime_engine::dict::Dict;
use pyime_engine::engine::Engine;
use pyime_engine::fuzzy::FuzzyConfig;
use pyime_engine::user::UserModel;
use std::collections::BTreeMap;

#[test]
#[ignore]
fn 留一法整句名次() {
    let dict = std::env::var("PYIME_DICT").expect("需设置 PYIME_DICT");
    let text = std::fs::read_to_string(std::env::var("PYIME_USER").expect("需设置 PYIME_USER")).unwrap();
    let mut engine = Engine::new(Dict::from_bytes(std::fs::read(dict).unwrap()).unwrap(), FuzzyConfig::default());
    let samples: Vec<(String, String)> = text
        .lines()
        .filter(|l| !l.starts_with('@'))
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((f.next()?.to_string(), f.next()?.to_string()))
        })
        .filter(|(t, w)| t.len() >= 8 && w.chars().count() >= 3)
        .collect();
    let mut ranks: BTreeMap<String, usize> = BTreeMap::new();
    let (mut h1, mut h3, mut h6) = (0, 0, 0);
    for (typed, word) in &samples {
        engine.set_user(UserModel::import(&text, 0));
        engine.forget(word);
        let rank = engine.candidates(typed).iter().position(|c| &c.text == word).map_or(0, |i| i + 1);
        h1 += usize::from(rank == 1);
        h3 += usize::from((1..=3).contains(&rank));
        h6 += usize::from((1..=6).contains(&rank));
        ranks.insert(format!("{typed}\t{word}"), rank);
    }
    println!("样本 {}:首位 {h1},前 3 {h3},首页(6) {h6}", samples.len());
    if let Ok(p) = std::env::var("PYIME_RANKS_OUT") {
        std::fs::write(p, ranks.iter().map(|(k, r)| format!("{k}\t{r}\n")).collect::<String>()).unwrap();
    }
    if let Ok(p) = std::env::var("PYIME_RANKS_BASE") {
        let (mut better, mut worse) = (Vec::new(), Vec::new());
        for l in std::fs::read_to_string(p).unwrap().lines() {
            let f: Vec<&str> = l.split('\t').collect();
            let (k, old) = (format!("{}\t{}", f[0], f[1]), f[2].parse::<usize>().unwrap());
            let new = ranks.get(&k).copied().unwrap_or(0);
            let key = |r: usize| if r == 0 { usize::MAX } else { r };
            if key(new) < key(old) {
                better.push(format!("{k} {old}→{new}"));
            } else if key(new) > key(old) {
                worse.push(format!("{k} {old}→{new}"));
            }
        }
        println!("变好 {}:\n  {}\n变差 {}:\n  {}", better.len(), better.join("\n  "), worse.len(), worse.join("\n  "));
    }
}
