use super::*;
use pyime_engine::dict::DictBuilder;

fn engine() -> Engine {
    let mut b = DictBuilder::default();
    b.add("ni", "你", 9000);
    b.add("ni", "泥", 8000);
    Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default())
}

#[test]
fn 文件不存在时照常写_自己写盘后再学习照常写回() {
    let p = temp_user_path("own");
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    ime.learn("ni", "泥", 0, "", &[]);
    let t = std::fs::read_to_string(&p).unwrap();
    assert!(t.contains("ni\t你\t1") && t.contains("ni\t泥\t1"), "{t}");
}

#[test]
fn 文件被外部修改后不覆盖() {
    let p = temp_user_path("external");
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    std::fs::write(&p, "ni\t泥\t9\nhao\t好\t3").unwrap();
    assert!(ime.user_file_changed());
    ime.learn("ni", "你", 0, "", &[]);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "ni\t泥\t9\nhao\t好\t3");
}

#[test]
fn 启动时读不出的文件不被覆盖() {
    let p = temp_user_path("badutf8");
    std::fs::write(&p, b"ni\t\xc4\xe3\t5").unwrap(); // GBK 编码的"你"
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    assert_eq!(std::fs::read(&p).unwrap(), b"ni\t\xc4\xe3\t5");
}
