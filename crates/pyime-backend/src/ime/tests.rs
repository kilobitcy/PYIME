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
fn 启动时读入带bom与crlf的文件() {
    let p = temp_user_path("bom");
    std::fs::write(&p, "\u{feff}ni\t泥\t9\r\nhao\t好\t3\r\n").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    let t = ime.user_export();
    assert!(t.contains("ni\t泥\t9") && t.contains("hao\t好\t3") && !t.contains('\u{feff}'), "{t}");
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

#[test]
fn 原子写替换已有文件且不留临时文件() {
    let p = temp_user_path("atomic");
    std::fs::write(&p, "旧内容").unwrap();
    write_atomic(&p, "新内容".as_bytes()).unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "新内容");
    assert!(!p.with_extension("tmp").exists());
}

#[test]
fn 原子写可新建文件() {
    let p = temp_user_path("atomic-new");
    write_atomic(&p, b"ni\t\xe4\xbd\xa0\t1").unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "ni\t你\t1");
}

fn apps_file(p: &Path) -> PathBuf {
    p.with_file_name("chinese-apps.txt")
}

#[test]
fn 默认中文列表_文件不存在时写入初始列表() {
    let p = temp_user_path("apps-init");
    let ime = Ime::with_user_path(engine(), p.clone());
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "Weixin.exe\nWeChat.exe\nwps.exe\net.exe\nwpp.exe\n");
    assert!(ime.is_chinese_app("weixin.exe"));
    assert!(!ime.is_chinese_app("notepad.exe"));
}

#[test]
fn 默认中文列表_空文件不重写() {
    let p = temp_user_path("apps-empty");
    std::fs::write(apps_file(&p), "").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "");
    assert!(!ime.is_chinese_app("Weixin.exe"));
}

#[test]
fn 默认中文列表_勾选加入与移除并写盘() {
    let p = temp_user_path("apps-toggle");
    std::fs::write(apps_file(&p), "Weixin.exe\r\n").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    assert!(ime.toggle_chinese_app("Notepad.exe"));
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "Weixin.exe\nNotepad.exe\n");
    assert!(ime.is_chinese_app("notepad.exe"));
    assert!(!ime.toggle_chinese_app("notepad.exe"));
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "Weixin.exe\n");
    assert!(!ime.toggle_chinese_app("")); // 空名不处理
    assert_eq!(std::fs::read_to_string(apps_file(&p)).unwrap(), "Weixin.exe\n");
}

#[test]
fn 默认中文列表_非utf8按空列表且不覆盖() {
    let p = temp_user_path("apps-badutf8");
    std::fs::write(apps_file(&p), b"Weixin.exe\n\xc4\xe3").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    assert!(!ime.is_chinese_app("Weixin.exe"));
    assert!(ime.toggle_chinese_app("notepad.exe")); // 只改内存
    assert!(ime.is_chinese_app("notepad.exe"));
    assert_eq!(std::fs::read(apps_file(&p)).unwrap(), b"Weixin.exe\n\xc4\xe3");
}

#[test]
fn 默认中文列表_无数据目录时用内存初始列表() {
    let ime = Ime::with_engine(engine());
    assert!(ime.is_chinese_app("WPS.EXE"));
    assert!(ime.toggle_chinese_app("notepad.exe"));
    assert!(ime.is_chinese_app("notepad.exe"));
}

#[test]
fn 无shift时只发方向键() {
    let ev = arrow_events(37, false, false);
    assert_eq!(ev, vec![KeyEv { vk: 37, scan: 0, up: false, extended: true }, KeyEv { vk: 37, scan: 0, up: true, extended: true }]);
}

#[test]
fn 按着左shift时先抬起再按回() {
    let ev = arrow_events(37, true, false);
    let lift = KeyEv { vk: 0xA0, scan: 0x2A, up: true, extended: false };
    let back = KeyEv { vk: 0xA0, scan: 0x2A, up: false, extended: false };
    assert_eq!(ev, vec![lift, KeyEv { vk: 37, scan: 0, up: false, extended: true }, KeyEv { vk: 37, scan: 0, up: true, extended: true }, back]);
}

#[test]
fn 左右shift都按着时都抬起_按回顺序相反() {
    let ev = arrow_events(39, true, true);
    let vks: Vec<(u16, bool)> = ev.iter().map(|e| (e.vk, e.up)).collect();
    assert_eq!(vks, vec![(0xA0, true), (0xA1, true), (39, false), (39, true), (0xA1, false), (0xA0, false)]);
    assert_eq!(ev[1].scan, 0x36);
}
