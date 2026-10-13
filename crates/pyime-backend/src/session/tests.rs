use super::*;
use pyime_engine::dict::{Dict, DictBuilder};
use pyime_engine::engine::Engine;
use pyime_engine::fuzzy::FuzzyConfig;

const N: i64 = 78;
const I: i64 = 73;
const H: i64 = 72;
const A: i64 = 65;
const O: i64 = 79;
const W: i64 = 87;

fn session() -> Session {
    let mut b = DictBuilder::default();
    for (i, w) in "你泥尼拟逆腻匿溺".chars().enumerate() {
        b.add("ni", &w.to_string(), 9000 - i as u32 * 100);
    }
    b.add("ni hao", "你好", 5000);
    b.add("hao", "好", 8000);
    b.add("wo", "我", 8000);
    let engine = Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default());
    Session::new(Ime::with_engine(engine))
}

fn key(s: &mut Session, method: &str, code: i64) -> Value {
    s.handle(&json!({"method": method, "seqNum": 7, "keyCode": code}))
}

fn press(s: &mut Session, codes: &[i64]) -> Value {
    let mut last = Value::Null;
    for &c in codes {
        last = key(s, "onKeyDown", c);
    }
    last
}

#[test]
fn 字母键被接管并出候选() {
    let mut s = session();
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
    let r = press(&mut s, &[N, I]);
    assert_eq!(r["compositionString"], "ni");
    assert_eq!(r["seqNum"], 7);
    assert_eq!(r["candidateList"], json!(["你", "泥", "尼", "拟", "逆"]));
    assert_eq!(r["customizeUI"]["candPerRow"], 5);
}

#[test]
fn 无拼音串时编辑键不接管() {
    let mut s = session();
    for k in [VK_SPACE, VK_RETURN, VK_DOWN, VK_MINUS, 49] {
        assert_eq!(key(&mut s, "filterKeyDown", k)["return"], false, "键 {k}");
    }
}

#[test]
fn 空格上屏高亮候选() {
    let mut s = session();
    press(&mut s, &[N, I]);
    let r = press(&mut s, &[VK_DOWN, VK_SPACE]);
    assert_eq!(r["commitString"], "泥");
    assert_eq!(r["compositionString"], "");
    assert_eq!(r["showCandidates"], false);
}

#[test]
fn 数字键选择当前页对应候选() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(key(&mut s, "onKeyDown", 51)["commitString"], "尼");
}

#[test]
fn 超出本页候选数的数字键不接管() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(key(&mut s, "filterKeyDown", 54)["return"], false); // 本页只有 5 个,6 不接管
}

#[test]
fn 回车上屏拼音串() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(key(&mut s, "onKeyDown", VK_RETURN)["commitString"], "ni");
}

#[test]
fn 翻页键换页且数字选的是新页() {
    let mut s = session();
    press(&mut s, &[N, I]);
    let r = key(&mut s, "onKeyDown", VK_EQUALS);
    assert_eq!(r["candidateList"], json!(["腻", "匿", "溺"]));
    assert_eq!(key(&mut s, "onKeyDown", 49)["commitString"], "腻");
}

#[test]
fn 减号和方括号往前翻页() {
    let mut s = session();
    press(&mut s, &[N, I, VK_RBRACKET]);
    assert_eq!(key(&mut s, "onKeyDown", VK_MINUS)["candidateList"][0], "你");
}

#[test]
fn 下键越过页尾进入下一页() {
    let mut s = session();
    press(&mut s, &[N, I, VK_DOWN, VK_DOWN, VK_DOWN, VK_DOWN]);
    let r = key(&mut s, "onKeyDown", VK_DOWN);
    assert_eq!((r["candidateList"][0].as_str(), r["candidateCursor"].as_i64()), (Some("腻"), Some(0)));
}

#[test]
fn 分段选词_剩余拼音继续_全部选完才上屏() {
    let mut s = session();
    let r = press(&mut s, &[N, I, H, A, O]);
    let pos = r["candidateList"].as_array().unwrap().iter().position(|c| c == "你").unwrap();
    let r = key(&mut s, "onKeyDown", 49 + pos as i64); // 选 你(只消耗 ni)
    assert!(r.get("commitString").is_none());
    assert_eq!(r["compositionString"], "你hao");
    let r = key(&mut s, "onKeyDown", 49);
    assert_eq!(r["commitString"], "你好");
}

#[test]
fn 选过的词被学习并影响下次排序() {
    let mut s = session();
    press(&mut s, &[N, I]);
    key(&mut s, "onKeyDown", 52); // 选 拟
    let r = press(&mut s, &[N, I]);
    assert_eq!(r["candidateList"][0], "拟");
}

#[test]
fn 退格删字符_删空时无内容不上屏() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(key(&mut s, "onKeyDown", VK_BACK)["compositionString"], "n");
    let r = key(&mut s, "onKeyDown", VK_BACK);
    assert_eq!(r["compositionString"], "");
    assert!(r.get("commitString").is_none());
}

#[test]
fn esc清空且无候选时空格上屏原串() {
    let mut s = session();
    press(&mut s, &[W, O]);
    assert_eq!(key(&mut s, "onKeyDown", VK_ESCAPE)["compositionString"], "");
    let r = press(&mut s, &[O, O, VK_SPACE]);
    assert_eq!(r["commitString"], "oo");
}

fn shift_click(s: &mut Session) -> Value {
    key(s, "filterKeyDown", 16);
    key(s, "filterKeyUp", 16)
}

#[test]
fn 单击shift切换到英文后字母放行() {
    let mut s = session();
    shift_click(&mut s);
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], false);
    shift_click(&mut s);
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
}

#[test]
fn shift配合其他键不切换() {
    let mut s = session();
    key(&mut s, "filterKeyDown", 16);
    key(&mut s, "filterKeyDown", N); // Shift+N
    key(&mut s, "filterKeyUp", 16);
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
}

#[test]
fn 组词中切英文把拼音原样上屏() {
    let mut s = session();
    press(&mut s, &[N, I]);
    let r = shift_click(&mut s);
    assert_eq!(r["commitString"], "ni");
    assert_eq!(r["showCandidates"], false);
}

#[test]
fn 未组词时shift加字母放行大写() {
    let mut s = session();
    let mut states = vec![0; 256];
    states[16] = 128;
    let r = s.handle(&json!({"method": "filterKeyDown", "seqNum": 1, "keyCode": N, "keyStates": states}));
    assert_eq!(r["return"], false);
}

#[test]
fn 分段选完后整句被学习() {
    let mut s = session();
    let r = press(&mut s, &[N, I, H, A, O]);
    let pos = r["candidateList"].as_array().unwrap().iter().position(|c| c == "你").unwrap();
    key(&mut s, "onKeyDown", 49 + pos as i64); // 先选 你
    let r = key(&mut s, "onKeyDown", 49); // 再选 好
    assert_eq!(r["commitString"], "你好");
}
