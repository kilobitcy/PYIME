//! 真实词典上的候选观察(忽略):
//! PYIME_DICT=/tmp/pyime.dict cargo test -p pyime-engine --release --test real_engine -- --ignored --nocapture
use pyime_engine::dict::Dict;
use pyime_engine::engine::Engine;
use pyime_engine::fuzzy::FuzzyConfig;

#[test]
#[ignore]
fn 观察候选() {
    let path = std::env::var("PYIME_DICT").expect("需设置 PYIME_DICT");
    let mut engine = Engine::new(Dict::from_bytes(std::fs::read(path).unwrap()).unwrap(), FuzzyConfig::default());
    if let Ok(g) = std::env::var("PYIME_GRAM") {
        engine.set_gram(pyime_engine::gram::Gram::from_bytes(std::fs::read(g).unwrap()).unwrap());
    }
    let inputs = std::env::var("PYIME_INPUTS").unwrap_or("nihao,zhongguo,zongguo,shurufa,surufa,women,fujian,husheng".into());
    for input in inputs.split(',') {
        let t = std::time::Instant::now();
        let c = engine.candidates(input);
        let shown: Vec<String> = c.iter().take(std::env::var("PYIME_SHOW").ok().and_then(|v| v.parse().ok()).unwrap_or(8)).map(|c| format!("{}{}", c.text, if c.fuzzy > 0 { "~" } else { "" })).collect();
        println!("{input:<14} {:>6.1?}  {}", t.elapsed(), shown.join(" "));
    }
}
