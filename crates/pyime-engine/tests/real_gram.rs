//! 观察真实语法模型(忽略):PYIME_GRAM=/tmp/gram/zh-hans-t-essay-bgw.gram cargo test -p pyime-engine --release --test real_gram -- --ignored --nocapture
use pyime_engine::gram::Gram;

#[test]
#[ignore]
fn 观察搭配() {
    let t = std::time::Instant::now();
    let g = Gram::from_bytes(std::fs::read(std::env::var("PYIME_GRAM").unwrap()).unwrap()).unwrap();
    println!("加载 {:?}", t.elapsed());
    for k in ["再", "在", "追加", "再追加", "在追加", "再追", "追", "的", "我爱", "我爱你", "爱你", "今天", "天气", "今天天气"] {
        println!("count {k}: {:?}", g.count(k));
    }
    for (c, w) in [("再", "追加"), ("在", "追加"), ("我", "爱你"), ("今天", "天气"), ("你", "好"), ("惊鸿", "一瞥"), ("在", "最佳"), ("好", "哦"), ("一", "惊"), ("已经", "追加"), ("一", "经"), ("已", "经"), ("再", "见"), ("在", "见")] {
        println!("{c}+{w}: {:.2}", g.bonus(c, w));
    }
}
