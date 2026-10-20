//! 真实词典回归用例 (拼音, 期望候选, 允许的最大名次,从 1 起)。
//! PYIME_DICT=/tmp/pyime.dict cargo test -p pyime-engine --release --test regression -- --ignored --nocapture
//! 输出按期望候选来源分桶的通过数;`PYIME_RANKS_OUT=文件` 存逐例名次,
//! `PYIME_RANKS_BASE=文件` 与之前存的名次逐例比对(调参前后对照用)。
use std::collections::BTreeMap;

use pyime_engine::dict::Dict;
use pyime_engine::engine::{Candidate, Engine, Source};
use pyime_engine::fuzzy::FuzzyConfig;

const CASES: &[(&str, &str, usize)] = &[
    ("jinhongyip", "惊鸿一瞥", 1), // 用户真实例子:in=ing + 末尾残缺音节
    ("jinghongyipie", "惊鸿一瞥", 1),
    ("zongguo", "中国", 1),
    ("surufa", "输入法", 1),
    ("woaini", "我爱你", 1),
    ("jintiantianqihenhao", "今天天气很好", 1),
    ("fujian", "福建", 3),
    ("nihao", "你好", 1),
    ("woxiangquguangzhou", "我想去广州", 1),
    ("tamenzaichifan", "他们在吃饭", 1),
    ("xiexienin", "谢谢您", 1),
    ("wozaijia", "我在家", 1),
    ("mingtianjian", "明天见", 1),
    ("zhidaole", "知道了", 1),
    ("bukeqi", "不客气", 1),
    ("woxihuanni", "我喜欢你", 1),
    ("haojiubujian", "好久不见", 1),
    ("zaijian", "再见", 1),
    ("zaiyici", "再一次", 1),
    ("zaishuoyibian", "再说一遍", 1),
    ("weishenme", "为什么", 1),
    ("shenmeshihou", "什么时候", 1),
    ("xianzai", "现在", 1),
    ("zaizuijia", "再追加", 2), // 模糊 zui→zhui
    ("fengzhong", "分钟", 1),   // 模糊 en→eng
    ("nanse", "蓝色", 3),       // 模糊 n→l
    ("yijingzhuijia", "已经追加", 1),
    ("woyaoshuijiao", "我要睡觉", 1),
    ("jintianxingqijia", "今天星期几", 1),
    ("zhuijia", "追加", 1),
    ("qinjixu", "请继续", 1),   // in=ing:精确的 亲继续 不应压过模糊的高频整句
    ("zaizhuijia", "再追加", 2), // 用户期望第 1:需 bigram 或学习一次
    ("celue", "策略", 1),       // lue 写法:词库键为 lve
    ("chelue", "策略", 3),      // 用户真实例子:c=ch 模糊 + lue
    ("jiashan", "加上", 1),     // 多字模糊 an=ang:惩罚 2.5 时首选
    ("yinp", "硬盘", 3),        // 缩写 + in=ing
    ("ne", "呢", 1),            // 护住单字:高频 了(n=l)不得压过
    ("si", "死", 1),            // 护住单字:高频 是(s=sh)不得压过
    ("lan", "蓝", 1),           // 护住单字:高频 然(r=l)不得压过
];

#[test]
#[ignore]
fn 回归用例() {
    let path = std::env::var("PYIME_DICT").expect("需设置 PYIME_DICT");
    let mut engine = Engine::new(Dict::from_bytes(std::fs::read(path).unwrap()).unwrap(), FuzzyConfig::default());
    if let Ok(g) = std::env::var("PYIME_GRAM") {
        engine.set_gram(pyime_engine::gram::Gram::from_bytes(std::fs::read(g).unwrap()).unwrap());
    }
    if let Ok(w) = std::env::var("PYIME_GRAM_WEIGHT") {
        engine.set_gram_weight(w.parse().unwrap());
    }
    if let Ok(p) = std::env::var("PYIME_FUZZY_PENALTY") {
        engine.set_fuzzy_penalty(p.parse().unwrap());
    }
    if let Ok(w) = std::env::var("PYIME_GRAM_T") {
        engine.set_gram_threshold(w.parse().unwrap());
    }
    let mut bad = Vec::new();
    // 按期望候选的来源分桶:桶名 → (用例数, 通过数)
    let mut buckets: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    // 逐例名次(0 表示未召回),供与上一次运行逐例比对
    let mut ranks = String::new();
    for &(input, want, max) in CASES {
        let cands = engine.candidates(input);
        let pos = cands.iter().position(|c| c.text == want);
        let rank = pos.map(|i| i + 1);
        let pass = rank.is_some_and(|r| r <= max);
        if !pass {
            bad.push(format!("{input} → {want}: 名次 {rank:?},要求 ≤{max}"));
        }
        let b = buckets.entry(pos.map_or("未召回".into(), |i| bucket(&cands[i]))).or_default();
        b.0 += 1;
        b.1 += pass as usize;
        ranks += &format!("{input}\t{want}\t{}\n", rank.unwrap_or(0));
    }
    for (name, (n, ok)) in &buckets {
        println!("通过 {ok:>2}/{n:<2} {name}");
    }
    if let Ok(base) = std::env::var("PYIME_RANKS_BASE") {
        diff(&std::fs::read_to_string(base).unwrap(), &ranks);
    }
    if let Ok(out) = std::env::var("PYIME_RANKS_OUT") {
        std::fs::write(out, &ranks).unwrap();
    }
    println!("通过 {}/{}", CASES.len() - bad.len(), CASES.len());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

fn bucket(c: &Candidate) -> String {
    let src = match c.source {
        Source::Whole => "整段词",
        Source::Sentence => "整句",
        Source::Prefix => "前缀词",
        Source::English => "英文",
    };
    if c.fuzzy > 0 { format!("{src}·模糊") } else { src.to_string() }
}

/// 逐例比对两次运行的名次:分开报告救回、弄坏与其余名次变化(按名次比,不看要求的上限)。
fn diff(base: &str, now: &str) {
    let old: BTreeMap<(&str, &str), usize> = base
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            Some(((*f.first()?, *f.get(1)?), f.get(2)?.parse().ok()?))
        })
        .collect();
    let (mut saved, mut broken, mut moved) = (Vec::new(), Vec::new(), Vec::new());
    for l in now.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        let r: usize = f[2].parse().unwrap();
        let Some(&o) = old.get(&(f[0], f[1])) else { continue };
        let line = format!("  {} → {}: {o} → {r}", f[0], f[1]);
        match (o == 1, r == 1) {
            (false, true) => saved.push(line),
            (true, false) => broken.push(line),
            _ if o != r => moved.push(line),
            _ => {}
        }
    }
    println!("与基线比对(名次 0 = 未召回):救回首位 {},跌出首位 {},其余变化 {}", saved.len(), broken.len(), moved.len());
    for (title, v) in [("救回首位", &saved), ("跌出首位", &broken), ("其余变化", &moved)] {
        if !v.is_empty() {
            println!("{title}:\n{}", v.join("\n"));
        }
    }
}
