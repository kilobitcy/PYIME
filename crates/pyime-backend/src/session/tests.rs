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

/// 键入拼音后按数字键选中文字为 `word` 的候选。
fn type_pick(s: &mut Session, codes: &[i64], word: &str) -> Value {
    let r = press(s, codes);
    let pos = r["candidateList"].as_array().unwrap().iter().position(|c| c == word).unwrap();
    key(s, "onKeyDown", 49 + pos as i64)
}

fn backs(s: &mut Session, n: usize) {
    for _ in 0..n {
        key(s, "filterKeyDown", VK_BACK);
        key(s, "filterKeyUp", VK_BACK);
    }
}

#[test]
fn 字母键被接管并出候选() {
    let mut s = session();
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
    let r = press(&mut s, &[N, I]);
    assert_eq!(r["compositionString"], "ni");
    assert_eq!(r["seqNum"], 7);
    assert_eq!(r["candidateList"], json!(["你", "泥", "尼", "拟", "逆", "腻"]));
    assert_eq!(r["customizeUI"]["candPerRow"], 6);
    assert_eq!(r["customizeUI"]["candFontName"], "Microsoft YaHei UI");
    assert_eq!(r["customizeUI"]["candFontSize"], 12);
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
    assert_eq!(key(&mut s, "filterKeyDown", 55)["return"], false); // 本页只有 6 个,7 不接管
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
    assert_eq!(r["candidateList"], json!(["匿", "溺"]));
    assert_eq!(key(&mut s, "onKeyDown", 49)["commitString"], "匿");
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
    press(&mut s, &[N, I, VK_DOWN, VK_DOWN, VK_DOWN, VK_DOWN, VK_DOWN]);
    let r = key(&mut s, "onKeyDown", VK_DOWN);
    assert_eq!((r["candidateList"][0].as_str(), r["candidateCursor"].as_i64()), (Some("匿"), Some(0)));
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
fn 刚松开组合shift紧接着单击视为误触不切换() {
    // 用户真实日志:按住 Shift 打完 SKILLS,松开约 150ms 后又碰了一下 Shift,被当成切到英文
    let mut s = session();
    key(&mut s, "filterKeyDown", 16);
    key(&mut s, "filterKeyDown", N); // Shift+N
    key(&mut s, "filterKeyUp", 16);
    shift_click(&mut s);
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], true);
}

#[test]
fn 组合shift松开后隔一会儿单击仍切换() {
    let mut s = session();
    s.shift_guard = Duration::ZERO;
    key(&mut s, "filterKeyDown", 16);
    key(&mut s, "filterKeyDown", N);
    key(&mut s, "filterKeyUp", 16);
    shift_click(&mut s);
    assert_eq!(key(&mut s, "filterKeyDown", N)["return"], false);
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

#[test]
fn 退格删掉上屏后对同一拼音改选别的词_撤回上次学习() {
    let mut s = session();
    press(&mut s, &[N, I]);
    press(&mut s, &[32]); // 空格选 你
    assert!(s.ime.user_export().contains("ni\t你\t1"));
    key(&mut s, "filterKeyDown", VK_BACK); // 应用删掉了刚上屏的字
    press(&mut s, &[N, I]);
    press(&mut s, &[50]); // 数字 2 选 泥
    let exported = s.ime.user_export();
    assert!(exported.contains("ni\t泥\t1") && !exported.contains("ni\t你"), "{exported}");
}

#[test]
fn 没按退格再选别的词_不撤回() {
    let mut s = session();
    press(&mut s, &[N, I]);
    press(&mut s, &[32]);
    press(&mut s, &[N, I]);
    press(&mut s, &[50]);
    let exported = s.ime.user_export();
    assert!(exported.contains("ni\t泥\t1") && exported.contains("ni\t你\t1"), "{exported}");
}

#[test]
fn 退格删光后重选同一个词_计数不累加() {
    let mut s = session();
    press(&mut s, &[N, I]);
    press(&mut s, &[32]);
    backs(&mut s, 1); // 单字"你"已删光,学习撤回
    press(&mut s, &[N, I]);
    press(&mut s, &[32]);
    assert!(s.ime.user_export().contains("ni\t你\t1"));
}

#[test]
fn 退格中途按了别的键_不再视为撤销() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    backs(&mut s, 1);
    key(&mut s, "filterKeyDown", 37); // 左方向键
    backs(&mut s, 1);
    assert!(s.ime.user_export().contains("nihao\t你好\t1"));
}

#[test]
fn 退格删光两字词即撤回_只删一字不撤回() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    backs(&mut s, 1);
    assert!(s.ime.user_export().contains("nihao\t你好\t1"));
    backs(&mut s, 1);
    assert!(!s.ime.user_export().contains("nihao\t你好"), "{}", s.ime.user_export());
}

#[test]
fn 同一次退格的两次_filterkeydown_只计一次() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    // libIME 对同一次按键会调两次 filterKeyDown(OnTestKeyDown 与 OnKeyDown),中间不抬键
    key(&mut s, "filterKeyDown", VK_BACK);
    key(&mut s, "filterKeyDown", VK_BACK);
    key(&mut s, "filterKeyUp", VK_BACK);
    assert!(s.ime.user_export().contains("nihao\t你好\t1"));
    backs(&mut s, 1);
    assert!(!s.ime.user_export().contains("nihao\t你好"), "{}", s.ime.user_export());
}

#[test]
fn 跳过右括号上屏后不跟踪退格() {
    let mut s = session();
    assert_eq!(commit(&pk(&mut s, 188, true)), "\u{300A}\u{300B}");
    press(&mut s, &[N, I]);
    let r = pk(&mut s, 190, true); // 跳过》,上屏"你"
    assert_eq!(commit(&r), "你");
    backs(&mut s, 1);
    assert!(s.ime.user_export().contains("ni\t你\t1"), "{}", s.ime.user_export());
}

#[test]
fn 标点上屏后删到词内再整串重选同一个词_不撤回() {
    let mut s = session();
    press(&mut s, &[N, I, H, A, O]);
    let r = pk(&mut s, 188, false);
    assert_eq!(commit(&r), "你好\u{FF0C}");
    backs(&mut s, 2); // 删掉逗号和"好",未删光
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    assert!(s.ime.user_export().contains("nihao\t你好\t2"), "{}", s.ime.user_export());
}

#[test]
fn 部分删除时上文同步回退() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    backs(&mut s, 1);
    type_pick(&mut s, &[N, I], "你");
    let e = s.ime.user_export();
    assert!(e.contains("@ng\t你\t你\t1"), "{e}");
    assert!(!e.contains("@ng\t好\t你"), "{e}");
}

#[test]
fn 只删标点时上文不回退() {
    let mut s = session();
    press(&mut s, &[N, I]);
    pk(&mut s, 188, false);
    backs(&mut s, 1);
    type_pick(&mut s, &[N, I], "你");
    let e = s.ime.user_export();
    assert!(e.contains("@ng\t你\t你\t1"), "{e}");
}

#[test]
fn 带标点上屏后连标点一起删光才撤回() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(commit(&pk(&mut s, 188, false)), "你\u{FF0C}");
    backs(&mut s, 1); // 只删了逗号
    assert!(s.ime.user_export().contains("ni\t你\t1"));
    backs(&mut s, 1);
    assert!(!s.ime.user_export().contains("ni\t你"));
}

#[test]
fn 成对符号上屏后不跟踪退格() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(commit(&pk(&mut s, 188, true)), "你\u{300A}\u{300B}");
    backs(&mut s, 3);
    assert!(s.ime.user_export().contains("ni\t你\t1"));
}

#[test]
fn 删光后重打不学到被删内容的搭配() {
    let mut s = session();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    backs(&mut s, 2);
    type_pick(&mut s, &[N, I], "你");
    assert!(!s.ime.user_export().contains("@ng"), "{}", s.ime.user_export());
}

fn pk(s: &mut Session, code: i64, shift: bool) -> Value {
    let mut ks = vec![0; 256];
    ks[16] = if shift { 0x80 } else { 0 };
    s.handle(&json!({"method": "onKeyDown", "seqNum": 1, "keyCode": code, "keyStates": ks}))
}

fn pkc(s: &mut Session, method: &str, code: i64, shift: bool, ctrl: bool) -> Value {
    let mut ks = vec![0; 256];
    ks[16] = if shift { 0x80 } else { 0 };
    ks[17] = if ctrl { 0x80 } else { 0 };
    s.handle(&json!({"method": method, "seqNum": 1, "keyCode": code, "keyStates": ks}))
}

#[test]
fn 组字中ctrl_shift_数字删除该候选的学习() {
    let mut s = session();
    type_pick(&mut s, &[N, I], "泥");
    let r = press(&mut s, &[N, I]);
    assert_eq!(r["candidateList"][0], "泥");
    let r = pkc(&mut s, "onKeyDown", 49, true, true);
    assert_eq!(r["return"], true);
    assert_eq!(commit(&r), "");
    assert!(s.composing());
    assert_eq!(r["candidateList"][0], "你");
    assert!(!s.ime.user_export().contains("泥"), "{}", s.ime.user_export());
}

#[test]
fn 非组字时ctrl_shift_数字放行() {
    let mut s = session();
    assert_eq!(pkc(&mut s, "filterKeyDown", 49, true, true)["return"], false);
}

#[test]
fn 组字中ctrl_数字不选词() {
    let mut s = session();
    press(&mut s, &[N, I]);
    let r = pkc(&mut s, "onKeyDown", 50, false, true);
    assert_eq!(r["return"], false);
    assert!(s.composing());
    assert!(!s.ime.user_export().contains("ni\t"));
}

#[test]
fn 未组字时ctrl字母放行给应用() {
    let mut s = session();
    for k in [65, 67, 86, 88, 90] {
        assert_eq!(pkc(&mut s, "filterKeyDown", k, false, true)["return"], false, "Ctrl+{k}");
    }
    assert!(!s.composing());
}

#[test]
fn 组字中ctrl字母放行且不改拼音() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(pkc(&mut s, "filterKeyDown", 86, false, true)["return"], false);
    assert_eq!(pkc(&mut s, "onKeyDown", 86, false, true)["return"], false);
    assert_eq!(s.input, "ni");
}

fn commit(v: &Value) -> &str {
    v["commitString"].as_str().unwrap_or("")
}

#[test]
fn 中文状态标点转全角() {
    let mut s = session();
    assert_eq!(commit(&pk(&mut s, 220, false)), "\u{3001}"); // \ → 、
    assert_eq!(commit(&pk(&mut s, 190, false)), "\u{3002}"); // . → 。
    assert_eq!(commit(&pk(&mut s, 188, false)), "\u{FF0C}");
    assert_eq!(commit(&pk(&mut s, 192, false)), "\u{00B7}"); // ` → ·
    assert_eq!(commit(&pk(&mut s, 52, true)), "\u{FFE5}"); // $ → ￥
    assert_eq!(commit(&pk(&mut s, 191, true)), "\u{FF1F}"); // ?
}

#[test]
fn 斜杠和英文状态保持原样() {
    let mut s = session();
    assert_eq!(key(&mut s, "filterKeyDown", 191)["return"], false);
    key(&mut s, "filterKeyDown", 16);
    key(&mut s, "filterKeyUp", 16); // 单击 Shift 切英文
    assert_eq!(key(&mut s, "filterKeyDown", 190)["return"], false);
    assert_eq!(key(&mut s, "filterKeyDown", 220)["return"], false);
}

#[test]
fn 组字时按标点先上屏高亮候选() {
    let mut s = session();
    press(&mut s, &[N, I]);
    assert_eq!(commit(&pk(&mut s, 188, false)), "你\u{FF0C}");
    assert!(!s.composing());
}

#[test]
fn 数字后的句点逗号冒号用半角() {
    let mut s = session();
    key(&mut s, "filterKeyDown", 49); // 数字 1 放行给应用
    assert_eq!(commit(&pk(&mut s, 190, false)), ".");
    assert_eq!(commit(&pk(&mut s, 190, false)), "\u{3002}"); // 紧接的第二个句点不再在数字后
    key(&mut s, "filterKeyDown", 50);
    assert_eq!(commit(&pk(&mut s, 186, true)), ":");
}

#[test]
fn 引号开闭交替() {
    let mut s = session();
    assert_eq!(commit(&pk(&mut s, 222, true)), "\u{201C}");
    assert_eq!(commit(&pk(&mut s, 222, true)), "\u{201D}");
    assert_eq!(commit(&pk(&mut s, 222, false)), "\u{2018}");
}

#[test]
fn 成对符号输出一对并回退光标_右符号跳过() {
    let mut s = session();
    assert_eq!(commit(&pk(&mut s, 188, true)), "\u{300A}\u{300B}"); // < → 《》
    assert_eq!(s.ime.take_pressed(), [37]);
    press(&mut s, &[N, I]);
    press(&mut s, &[32]); // 在书名号里输入 你
    assert_eq!(commit(&pk(&mut s, 190, true)), ""); // > 跳过自动补全的 》
    assert_eq!(s.ime.take_pressed(), [39]);
    assert_eq!(commit(&pk(&mut s, 190, true)), "\u{300B}"); // 再按才真输出
}

#[test]
fn 括号对应关系() {
    let mut s = session();
    assert_eq!(commit(&pk(&mut s, 57, true)), "\u{FF08}\u{FF09}"); // ( → （）
    assert_eq!(commit(&pk(&mut s, 219, true)), "\u{3010}\u{3011}"); // { → 【】
    assert_eq!(commit(&pk(&mut s, 219, false)), "\u{300C}\u{300D}"); // [ → 「」
}

#[test]
fn 方括号在组字时仍是翻页键() {
    let mut s = session();
    press(&mut s, &[N, I]);
    let r = pk(&mut s, 221, false);
    assert_eq!(commit(&r), "");
    assert!(s.composing());
    assert!(s.ime.take_pressed().is_empty());
}

#[test]
fn 用户移动光标后右符号不再跳过() {
    let mut s = session();
    pk(&mut s, 188, true);
    s.inject_echo = Duration::ZERO;
    key(&mut s, "filterKeyDown", 37); // 用户自己按了左方向键
    s.ime.take_pressed();
    assert_eq!(commit(&pk(&mut s, 190, true)), "\u{300B}");
    assert!(s.ime.take_pressed().is_empty());
}

#[test]
fn 自己注入的方向键回声不清状态() {
    let mut s = session();
    pk(&mut s, 188, true);
    key(&mut s, "filterKeyDown", 37); // 注入的左键经输入法回来
    assert_eq!(commit(&pk(&mut s, 190, true)), "");
}

#[test]
fn 组字串按高亮候选的切分加音节分隔符() {
    let mut b = DictBuilder::default();
    b.add("xian", "先", 500);
    b.add("xi an", "西安", 900);
    b.add("de", "得", 5000);
    b.add("dao", "道", 5000);
    let engine = Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default());
    let mut s = Session::new(Ime::with_engine(engine));
    let r = press(&mut s, &[88, I, A, N]); // xian
    let list: Vec<String> = r["candidateList"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect();
    let a = list.iter().position(|c| c == "西安").unwrap();
    let f = list.iter().position(|c| c == "先").unwrap();
    s.cursor = a;
    assert_eq!(s.display_input(), "xi'an");
    s.cursor = f;
    assert_eq!(s.display_input(), "xian");
}

#[test]
fn 整句分隔符和没打完的尾部() {
    let mut b = DictBuilder::default();
    for (k, w) in [("de", "得"), ("dao", "道"), ("zhe", "者")] {
        b.add(k, w, 5000);
    }
    let engine = Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default());
    let mut s = Session::new(Ime::with_engine(engine));
    let r = press(&mut s, &[68, 69, 68, 65, 79, 90, 72, 69]); // dedaozhe
    assert_eq!(r["compositionString"], "de'dao'zhe");
}

/// 取出本测试线程至今写出的 cancel 行的拼音(同时清空捕获列表)。
fn cancels() -> Vec<String> {
    crate::inputlog::take_captured().into_iter().filter_map(|l| l["cancel"].as_str().map(String::from)).collect()
}

#[test]
fn esc放弃组字记一次cancel() {
    let mut s = session();
    cancels();
    press(&mut s, &[N, I, H, A, O]);
    press(&mut s, &[VK_ESCAPE]);
    let lines: Vec<Value> = crate::inputlog::take_captured().into_iter().filter(|l| l["cancel"].is_string()).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["cancel"], "nihao");
    assert_eq!(lines[0]["top"][0], "你好");
}

#[test]
fn 退格删光拼音记最长的串() {
    let mut s = session();
    cancels();
    press(&mut s, &[N, I, H, A, O]);
    press(&mut s, &[VK_BACK, VK_BACK, VK_BACK, VK_BACK, VK_BACK]);
    assert_eq!(cancels(), ["nihao"]);
}

#[test]
fn 部分选词后esc也记cancel() {
    let mut s = session();
    cancels();
    type_pick(&mut s, &[N, I, H, A, O], "你");
    press(&mut s, &[VK_ESCAPE]);
    assert_eq!(cancels(), ["nihao"]);
}

#[test]
fn 回车与shift切英文不记cancel() {
    let mut s = session();
    cancels();
    press(&mut s, &[N, I]);
    press(&mut s, &[VK_RETURN]);
    press(&mut s, &[W, O]);
    shift_click(&mut s);
    assert!(cancels().is_empty());
}

#[test]
fn 上屏后新组字的cancel记新串() {
    let mut s = session();
    cancels();
    type_pick(&mut s, &[N, I], "你");
    press(&mut s, &[W, O]);
    press(&mut s, &[VK_ESCAPE]);
    assert_eq!(cancels(), ["wo"]);
}

#[test]
fn 部分选词后退格删光不记cancel() {
    let mut s = session();
    cancels();
    type_pick(&mut s, &[N, I, H, A, O], "你");
    press(&mut s, &[VK_BACK, VK_BACK, VK_BACK]);
    assert!(cancels().is_empty());
}

const L: i64 = 76;

#[test]
fn 整句里的模糊词随选中被学习_退格删光一并撤回() {
    let mut s = session();
    type_pick(&mut s, &[L, I, W, O], "你我"); // li 经 n=l 模糊得"你"
    let x = s.ime.user_export();
    assert!(x.contains("liwo\t你我\t1") && x.contains("li\t你\t1"), "{x}");
    backs(&mut s, 2);
    let x = s.ime.user_export();
    assert!(!x.contains("liwo\t你我") && !x.contains("li\t你"), "{x}");
}

#[test]
fn 撤回日志仍只记候选自身四元组() {
    let mut s = session();
    crate::inputlog::take_captured();
    type_pick(&mut s, &[L, I, W, O], "你我");
    backs(&mut s, 2);
    let lines = crate::inputlog::take_captured();
    let items = lines.iter().find_map(|l| l["retract"].as_array().cloned()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_array().unwrap().len(), 4);
    assert_eq!(items[0][1], "你我");
}

/// 小写字母串 → 对应的虚拟键码序列。
fn letters(s: &str) -> Vec<i64> {
    s.bytes().map(|b| i64::from(b.to_ascii_uppercase())).collect()
}

#[test]
fn 回车原样上屏的串满2次出现在候选首位_并记日志() {
    let mut s = session();
    crate::inputlog::take_captured();
    for _ in 0..2 {
        press(&mut s, &letters("zhegnshi"));
        assert_eq!(key(&mut s, "onKeyDown", VK_RETURN)["commitString"], "zhegnshi");
    }
    let logs = crate::inputlog::take_captured();
    assert_eq!(logs.iter().filter(|l| l["enter_raw"] == "zhegnshi" && l["counted"] == true).count(), 2);
    let r = press(&mut s, &letters("zhegnshi"));
    assert_eq!(r["candidateList"][0], "zhegnshi");
}

#[test]
fn 回车不学_精确通_不足4字母_分段选过词() {
    let mut s = session();
    crate::inputlog::take_captured();
    for _ in 0..2 {
        press(&mut s, &letters("nihao"));
        key(&mut s, "onKeyDown", VK_RETURN);
        press(&mut s, &letters("zhg"));
        key(&mut s, "onKeyDown", VK_RETURN);
        type_pick(&mut s, &letters("nizhegnshi"), "你");
        assert_eq!(key(&mut s, "onKeyDown", VK_RETURN)["commitString"], "你zhegnshi");
    }
    let logs = crate::inputlog::take_captured();
    let raws: Vec<&Value> = logs.iter().filter(|l| l.get("enter_raw").is_some()).collect();
    assert_eq!(raws.len(), 4, "分段选过词后回车不记 enter_raw");
    assert!(raws.iter().all(|l| l["counted"] == false));
    let r = press(&mut s, &letters("zhegnshi"));
    assert!(!r["candidateList"].as_array().is_some_and(|c| c.iter().any(|t| t == "zhegnshi")));
}

#[test]
fn 回车学会的英文串可用ctrl_shift_数字删除() {
    let mut s = session();
    for _ in 0..2 {
        press(&mut s, &letters("zhegnshi"));
        key(&mut s, "onKeyDown", VK_RETURN);
    }
    let r = press(&mut s, &letters("zhegnshi"));
    assert_eq!(r["candidateList"][0], "zhegnshi");
    let r = pkc(&mut s, "onKeyDown", 49, true, true);
    assert!(!r["candidateList"].as_array().is_some_and(|c| c.iter().any(|t| t == "zhegnshi")));
}

/// 取出本测试线程至今写出的 edit 行:(拼音, 首个候选)(同时清空捕获列表)。
fn edits() -> Vec<(String, String)> {
    crate::inputlog::take_captured()
        .into_iter()
        .filter_map(|l| Some((l["edit"].as_str()?.to_string(), l["top"][0].as_str().unwrap_or("").to_string())))
        .collect()
}

#[test]
fn 连续退格只记一次edit_重新打长后再删再记() {
    let mut s = session();
    edits();
    press(&mut s, &[N, I, H, A, O]);
    press(&mut s, &[VK_BACK, VK_BACK, VK_BACK]);
    assert_eq!(edits(), [("nihao".to_string(), "你好".to_string())]);
    press(&mut s, &[W, O]);
    press(&mut s, &[VK_BACK, VK_BACK]);
    assert_eq!(edits().into_iter().map(|e| e.0).collect::<Vec<_>>(), ["niwo"]);
}

#[test]
fn 没有退格不记edit() {
    let mut s = session();
    edits();
    type_pick(&mut s, &[N, I, H, A, O], "你好");
    assert!(edits().is_empty());
}

/// 未组词时按住 Shift 敲一个字母:输入法放行,应用里直接出大写字母。
fn shift_letter(s: &mut Session, code: i64) {
    pkc(s, "filterKeyDown", code, true, false);
    pkc(s, "filterKeyUp", code, true, false);
}

#[test]
fn 首字母大写的英文词回车学整词_不学去掉首字母的残片() {
    // 用户真实日志:Johnson 的 J 被放行给应用,其余 ohnson 组字后回车,曾把 ohnson 记进英文学习
    let mut s = session();
    crate::inputlog::take_captured();
    for _ in 0..2 {
        pkc(&mut s, "filterKeyDown", 74, true, false); // Shift+J
        pkc(&mut s, "filterKeyDown", 74, true, false); // libIME 对同一次按键可能重复调用
        pkc(&mut s, "filterKeyUp", 74, true, false);
        press(&mut s, &letters("ohnson"));
        assert_eq!(key(&mut s, "onKeyDown", VK_RETURN)["commitString"], "ohnson", "应用里已有 J,只上屏其余字母");
    }
    let logs = crate::inputlog::take_captured();
    assert_eq!(logs.iter().filter(|l| l["enter_raw"] == "johnson").count(), 2, "{logs:?}");
    let r = press(&mut s, &letters("johnson"));
    assert_eq!(r["candidateList"][0], "johnson");
    key(&mut s, "onKeyDown", VK_ESCAPE);
    let r = press(&mut s, &letters("ohnson"));
    assert!(!r["candidateList"].as_array().is_some_and(|c| c.iter().any(|t| t == "ohnson")));
}

#[test]
fn 大写字母后隔了别的键或连打多个大写_回车不拼接() {
    let mut s = session();
    crate::inputlog::take_captured();
    shift_letter(&mut s, 74);
    key(&mut s, "filterKeyDown", VK_SPACE); // 放行给应用的空格
    press(&mut s, &letters("ohnson"));
    key(&mut s, "onKeyDown", VK_RETURN);
    shift_letter(&mut s, 79); // O
    shift_letter(&mut s, 67); // C
    press(&mut s, &letters("rtest"));
    key(&mut s, "onKeyDown", VK_RETURN);
    let raws: Vec<String> = crate::inputlog::take_captured().iter().filter_map(|l| l["enter_raw"].as_str().map(String::from)).collect();
    assert_eq!(raws, ["ohnson", "rtest"]);
}

#[test]
fn 选了英文候选后上文不含英文字母() {
    // 用户真实日志:选 test 后再打 bianhao,记下了 est→编号、st→编号、t→编号 的个人搭配
    let mut s = session();
    for _ in 0..2 {
        press(&mut s, &letters("zhegnshi"));
        key(&mut s, "onKeyDown", VK_RETURN);
    }
    press(&mut s, &letters("zhegnshi"));
    assert_eq!(key(&mut s, "onKeyDown", VK_SPACE)["commitString"], "zhegnshi");
    type_pick(&mut s, &[N, I], "你");
    let e = s.ime.user_export();
    assert!(!e.lines().any(|l| l.starts_with("@ng\t") && l.split('\t').nth(1).unwrap().bytes().any(|b| b.is_ascii_alphabetic())), "{e}");
}

#[test]
fn 应用强制结束组字后清空拼音_不带到下次输入() {
    // 用户真实日志:组字 ocr 时应用强制结束组字(forced=true),会话仍留着 ocr,之后打的字母接在后面
    let mut s = session();
    press(&mut s, &[N, I]);
    s.handle(&json!({"method": "onCompositionTerminated", "forced": true, "seqNum": 9}));
    assert_eq!(press(&mut s, &[H, A, O])["compositionString"], "hao");
    // 正常上屏后的 forced=false 不影响组字
    press(&mut s, &[VK_ESCAPE]);
    press(&mut s, &[N, I]);
    s.handle(&json!({"method": "onCompositionTerminated", "forced": false, "seqNum": 10}));
    assert_eq!(press(&mut s, &[H, A, O])["compositionString"], "ni'hao");
}
