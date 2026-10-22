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

#[test]
fn 重新加载读入外部修改并恢复写回() {
    let p = temp_user_path("reload");
    let ime = Ime::with_user_path(engine(), p.clone());
    std::fs::write(&p, "\u{feff}ni\t泥\t9\r\nhao\t好\t3\r\n").unwrap();
    assert_eq!(ime.reload_user(), Ok(()));
    assert!(!ime.user_file_changed());
    assert_eq!(ime.generation(), 1);
    ime.learn("ni", "你", 0, "", &[]);
    let t = std::fs::read_to_string(&p).unwrap();
    assert!(t.contains("ni\t泥\t9") && t.contains("hao\t好\t3") && t.contains("ni\t你\t1") && !t.contains('\r'), "{t}");
}

#[test]
fn 重新加载遇非utf8不读入且继续保护() {
    let p = temp_user_path("reload-bad");
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    std::fs::write(&p, b"ni\t\xc4\xe3\t5").unwrap();
    assert!(ime.reload_user().is_err());
    assert_eq!(ime.generation(), 0);
    assert!(ime.user_export().contains("ni\t你\t1"));
    ime.learn("ni", "你", 0, "", &[]);
    assert_eq!(std::fs::read(&p).unwrap(), b"ni\t\xc4\xe3\t5");
}

#[test]
fn 编辑先写盘再备份再开记事本() {
    let p = temp_user_path("edit");
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    ime.edit_user();
    let bak = std::fs::read_to_string(p.with_extension("txt.bak")).unwrap();
    assert!(bak.contains("ni\t你\t1"), "{bak}");
    assert_eq!(ime.take_launched(), vec![format!("notepad.exe {}", p.display())]);
}

#[test]
fn 已被外部修改时再点编辑不冲掉用户的改动() {
    let p = temp_user_path("edit-twice");
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    std::fs::write(&p, "hao\t好\t3").unwrap();
    ime.edit_user();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "hao\t好\t3");
    assert_eq!(std::fs::read_to_string(p.with_extension("txt.bak")).unwrap(), "hao\t好\t3");
}

#[test]
fn 打开数据目录前先建目录() {
    let p = temp_user_path("opendir");
    let dir = p.parent().unwrap().to_path_buf();
    std::fs::remove_dir_all(&dir).unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.open_data_dir();
    assert!(dir.is_dir());
    assert_eq!(ime.take_launched(), vec![format!("explorer.exe {}", dir.display())]);
}
