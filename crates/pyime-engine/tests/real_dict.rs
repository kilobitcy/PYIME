//! 用真实词典做冒烟检查。需先编译词典并设置 PYIME_DICT,默认忽略:
//! `PYIME_DICT=/tmp/pyime.dict cargo test -p pyime-engine --release -- --ignored --nocapture`
use pyime_engine::dict::Dict;
use std::time::Instant;

fn load() -> Dict {
    let path = std::env::var("PYIME_DICT").expect("请设置 PYIME_DICT");
    let t = Instant::now();
    let dict = Dict::from_bytes(std::fs::read(path).unwrap()).unwrap();
    println!("加载耗时 {:?}", t.elapsed());
    dict
}

#[test]
#[ignore]
fn 常用词能查到且排在前面() {
    let d = load();
    for (key, want) in [("ni hao", "你好"), ("zhong guo", "中国"), ("wei xin", "微信"), ("shu ru fa", "输入法")] {
        let top: Vec<_> = d.get(key).into_iter().take(5).map(|e| e.word).collect();
        println!("{key} -> {top:?}");
        assert!(top.iter().any(|w| w == want), "{key} 前 5 名里没有 {want}");
    }
    let t = Instant::now();
    let n = d.prefix("zhong", 5000).len();
    println!("前缀 zhong: {n} 个键,耗时 {:?}", t.elapsed());
}
