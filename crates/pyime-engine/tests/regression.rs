//! 真实词典回归用例 (拼音, 期望候选, 允许的最大名次,从 1 起)。
//! PYIME_DICT=/tmp/pyime.dict cargo test -p pyime-engine --release --test regression -- --ignored
use pyime_engine::dict::Dict;
use pyime_engine::engine::Engine;
use pyime_engine::fuzzy::FuzzyConfig;

const CASES: &[(&str, &str, usize)] = &[
    ("jinhongyip", "惊鸿一瞥", 1), // 用户真实例子:in=ing + 末尾残缺音节
    ("jinghongyipie", "惊鸿一瞥", 1),
    ("zongguo", "中国", 1),
    ("surufa", "输入法", 1),
    ("woaini", "我爱你", 1),
    ("jintiantianqihenhao", "今天天气很好", 1),
    ("fujian", "福建", 3),
];

#[test]
#[ignore]
fn 回归用例() {
    let path = std::env::var("PYIME_DICT").expect("需设置 PYIME_DICT");
    let engine = Engine::new(Dict::from_bytes(std::fs::read(path).unwrap()).unwrap(), FuzzyConfig::default());
    let mut bad = Vec::new();
    for &(input, want, max) in CASES {
        let rank = engine.candidates(input).iter().position(|c| c.text == want).map(|i| i + 1);
        if !rank.is_some_and(|r| r <= max) {
            bad.push(format!("{input} → {want}: 名次 {rank:?},要求 ≤{max}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
