//! 全进程共享的输入法状态:引擎(含词典与用户学习数据)及用户数据存盘。
use crate::apps::ChineseApps;
use crate::log;
use pyime_engine::dict::{Dict, DictBuilder};
use pyime_engine::english::{parse_compiled, parse_list, English, TIER_CUSTOM};
use pyime_engine::engine::{Candidate, Engine, Lesson};
use pyime_engine::fuzzy::FuzzyConfig;
use pyime_engine::gram::Gram;
use pyime_engine::symbol::Symbols;
use pyime_engine::user::UserModel;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

pub struct Ime {
    engine: RefCell<Engine>,
    user_path: Option<PathBuf>,
    en_learned_path: Option<PathBuf>,
    /// 上次成功读入或写盘后 `user.txt` 的 (修改时间, 长度)。与磁盘不一致即视为被外部修改,不再写回;
    /// 为 None 且文件存在(启动时读不出,如非 UTF-8)同样不写回,直到重新加载成功。
    user_stamp: Cell<Option<(SystemTime, u64)>>,
    /// 已为"跳过写回"记过日志(写盘或重载成功后清除),免得每次选词都记一行。
    skip_logged: Cell<bool>,
    /// 用户数据代际号,每次重新加载成功 +1;会话据此丢弃重载前的撤销记录。
    generation: Cell<u32>,
    /// 默认中文的程序列表(`chinese-apps.txt`);`apps_path` 为 None 时只在内存。
    apps: RefCell<ChineseApps>,
    apps_path: Option<PathBuf>,
    /// 启动时文件读不出(如非 UTF-8):勾选只改内存,不覆盖用户的文件。
    apps_writable: Cell<bool>,
    /// 测试用:记录"启动外部程序"而不真的启动。
    #[cfg(test)]
    launched: RefCell<Vec<String>>,
    /// 测试用:记录"模拟按键"而不真的发送。
    #[cfg(test)]
    pressed: RefCell<Vec<u16>>,
}

/// 模拟按键的一个事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyEv {
    pub vk: u16,
    pub scan: u16,
    pub up: bool,
    pub extended: bool,
}

/// 模拟一次方向键的事件序列。用户按着 Shift(打 `(` 时常未松开)会让应用收到 Shift+方向键而选中文字,
/// 故按着的 Shift 先抬起、发完方向键再按回;整串由一次 `SendInput` 发出,不与用户输入交错。
pub(crate) fn arrow_events(vk: u16, lshift: bool, rshift: bool) -> Vec<KeyEv> {
    let shifts: Vec<(u16, u16)> = [(lshift, 0xA0, 0x2A), (rshift, 0xA1, 0x36)].iter().filter(|s| s.0).map(|s| (s.1, s.2)).collect();
    let mut ev: Vec<KeyEv> = shifts.iter().map(|&(vk, scan)| KeyEv { vk, scan, up: true, extended: false }).collect();
    ev.push(KeyEv { vk, scan: 0, up: false, extended: true });
    ev.push(KeyEv { vk, scan: 0, up: true, extended: true });
    ev.extend(shifts.iter().rev().map(|&(vk, scan)| KeyEv { vk, scan, up: false, extended: false }));
    ev
}

/// 向前台应用注入一次方向键(按下 + 弹起),用于成对符号补全后回退/跳过光标;按着的 Shift 临时抬起。
#[cfg(windows)]
fn send_key(vk: u16) {
    #[repr(C)]
    struct KeybdInput {
        vk: u16,
        scan: u16,
        flags: u32,
        time: u32,
        extra: usize,
    }
    #[repr(C)]
    struct Input {
        kind: u32,
        ki: KeybdInput,
        pad: [u8; 8],
    }
    #[link(name = "user32")]
    extern "system" {
        fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
        fn GetAsyncKeyState(vk: i32) -> i16;
    }
    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_EXTENDEDKEY: u32 = 1;
    const KEYEVENTF_KEYUP: u32 = 2;
    // SAFETY: GetAsyncKeyState 只读全局键盘状态。
    let down = |vk: i32| unsafe { GetAsyncKeyState(vk) } < 0;
    // 剩余竞态:查询与 SendInput 之间(微秒级)用户恰好松开 Shift,Shift 会逻辑上停在按下,直到下次按 Shift
    let (l, r) = (down(0xA0), down(0xA1));
    let inputs: Vec<Input> = arrow_events(vk, l, r)
        .into_iter()
        .map(|e| {
            let flags = if e.up { KEYEVENTF_KEYUP } else { 0 } | if e.extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
            Input { kind: INPUT_KEYBOARD, ki: KeybdInput { vk: e.vk, scan: e.scan, flags, time: 0, extra: 0 }, pad: [0; 8] }
        })
        .collect();
    // SAFETY: inputs 是有效的 INPUT 数组,布局与 Win32 INPUT(64 位下 40 字节)一致。
    let t = std::time::Instant::now();
    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<Input>() as i32) };
    // 诊断:注入时的物理 Shift、实际注入条数与 SendInput 耗时(被宿主低级键盘钩子拖住时明显变长)
    log::line(&format!("注入方向键 vk={vk} 物理Shift L={} R={} SendInput {sent}/{} 耗时 {:?}", u8::from(l), u8::from(r), inputs.len(), t.elapsed()));
}

/// 在专用线程里按序注入方向键,不阻塞请求处理。
/// 宿主(如微信 4.x)在等本次按键回复时线程被占住,若其装有低级键盘钩子,同步 `SendInput` 会被拖到钩子超时(实测约 1.2s);
/// 期间用户松开 Shift、继续打的键与注入序列交错,注入的 Shift↓ 落在用户的 Shift↑ 之后,Shift 卡在按下(日志 2026-11-11)。
/// 放到线程里后回复先发出、宿主线程空出,钩子即时处理;同一线程保证多次注入不乱序。
#[cfg(windows)]
fn send_key_async(vk: u16) {
    use std::sync::mpsc::{channel, Sender};
    use std::sync::{Mutex, OnceLock};
    static TX: OnceLock<Mutex<Sender<u16>>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = channel::<u16>();
        std::thread::spawn(move || {
            for vk in rx {
                send_key(vk);
            }
        });
        Mutex::new(tx)
    });
    if let Ok(tx) = tx.lock() {
        let _ = tx.send(vk);
    }
}

/// 词典随安装包放在 `server.exe` 同目录。
fn dict_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("pyime.dict"))
}

/// 可选的语法模型(octagram 格式),放在词典旁;没有就不做搭配加分。
fn gram_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("pyime.gram"))
}

fn user_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("PYIME").join("user.txt"))
}

/// 英文词表随安装包放在 `server.exe` 同目录。
fn en_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("pyime.en"))
}

/// 符号表随安装包放在 `server.exe` 同目录;缺失则不出符号。
fn symbols_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("symbols.txt"))
}

/// 用户数据目录 `%LOCALAPPDATA%\PYIME` 下的文件。
fn local_file(name: &str) -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("PYIME").join(name))
}

/// 文件的 (修改时间, 长度);不存在或取不到时为 None。
fn stamp(p: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// 先写同目录临时文件 `<名>.tmp` 再改名替换:写到一半进程被结束(看门狗、taskkill)时原文件不受损。
/// 改名失败(如目标正被别的程序独占打开)时删掉临时文件,退回直接覆盖写。
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        log::line(&format!("改名 {tmp:?} → {path:?} 失败,改为直接覆盖: {e}"));
        return std::fs::write(path, data);
    }
    Ok(())
}

/// 测试用:本进程独占的临时 `user.txt` 路径(目录已建、文件不存在)。
#[cfg(test)]
pub fn temp_user_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pyime-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("user.txt")
}

/// 读可选文本文件:不存在静默跳过,其他错误记日志后跳过。
fn read_optional(p: &std::path::Path) -> Option<String> {
    match std::fs::read_to_string(p) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            log::line(&format!("读取 {p:?} 失败: {e}"));
            None
        }
    }
}

/// 模糊音开关文件 `fuzzy.txt`:与 `user.txt` 同目录,每行一个关闭的规则。
fn fuzzy_path(user: Option<&Path>) -> Option<PathBuf> {
    user.map(|p| p.with_file_name("fuzzy.txt"))
}

/// 默认中文程序列表 `chinese-apps.txt`:与 `user.txt` 同目录。
fn apps_path(user: Option<&Path>) -> Option<PathBuf> {
    user.map(|p| p.with_file_name("chinese-apps.txt"))
}

/// 读入默认中文程序列表,返回 (列表, 能否写回)。文件不存在时写入初始列表;
/// 读不出(非 UTF-8 等)按空列表处理且不写回;没有数据目录时用内存初始列表。
fn read_apps(path: Option<&Path>) -> (ChineseApps, bool) {
    let Some(p) = path else { return (ChineseApps::defaults(), true) };
    match std::fs::read(p) {
        Ok(b) => match String::from_utf8(b) {
            Ok(t) => (ChineseApps::parse(&t), true),
            Err(_) => {
                log::line(&format!("{p:?} 不是 UTF-8,按空列表处理且不写回"));
                (ChineseApps::parse(""), false)
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let apps = ChineseApps::defaults();
            write_apps(p, &apps);
            (apps, true)
        }
        Err(e) => {
            log::line(&format!("读取 {p:?} 失败,按空列表处理且不写回: {e}"));
            (ChineseApps::parse(""), false)
        }
    }
}

fn write_apps(p: &Path, apps: &ChineseApps) {
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = write_atomic(p, apps.export().as_bytes()) {
        log::line(&format!("写入 {p:?} 失败: {e}"));
    }
}

/// `fuzzy.txt` 中表示关闭首键暂存补发的一行。属于前端功能,引擎不认识,由后端自己读写。
const STASH_OFF: &str = "首键暂存";

fn stash_off_in(text: &str) -> bool {
    text.trim_start_matches('\u{feff}').lines().any(|l| l.trim() == STASH_OFF)
}

/// 读入模糊音开关;文件不存在时全开。
fn read_fuzzy(engine: &mut Engine, user: Option<&Path>) {
    if let Some(t) = fuzzy_path(user).and_then(|p| read_optional(&p)) {
        engine.set_fuzzy(FuzzyConfig::parse_off(&t));
    }
}

/// 规整手工编辑过的 `user.txt`:去 BOM、CRLF→LF。
fn normalize_user(text: &str) -> String {
    text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n")
}

/// 英文词表:安装目录 `pyime.en` + 用户 `en.txt`(档位 0),再读回车学习计数;缺失的来源跳过。
fn load_english(learned: Option<&PathBuf>) -> English {
    let mut items = match en_path().map(|p| (std::fs::read_to_string(&p), p)) {
        Some((Ok(t), _)) => parse_compiled(&t),
        Some((Err(e), p)) => {
            log::line(&format!("读取英文词表 {p:?} 失败: {e}"));
            Vec::new()
        }
        None => Vec::new(),
    };
    if let Some(t) = local_file("en.txt").and_then(|p| read_optional(&p)) {
        items.extend(parse_list(&t).into_iter().map(|(k, w)| (k, w, TIER_CUSTOM)));
    }
    let mut en = English::new(items);
    if let Some(t) = learned.and_then(|p| read_optional(p)) {
        en.import_raw(&t);
    }
    en
}

fn load_dict() -> Dict {
    let loaded = dict_path().and_then(|p| match std::fs::read(&p) {
        Ok(b) => Dict::from_bytes(b).map_err(|e| log::line(&format!("词典 {p:?} 无效: {e}"))).ok(),
        Err(e) => {
            log::line(&format!("读取词典 {p:?} 失败: {e}"));
            None
        }
    });
    // 词典缺失时用空词典,输入法仍可运行(只能上屏拼音)
    loaded.unwrap_or_else(|| Dict::from_bytes(DictBuilder::default().build().unwrap()).unwrap())
}

impl Ime {
    fn build(engine: Engine, user_path: Option<PathBuf>, en_learned_path: Option<PathBuf>, user_stamp: Option<(SystemTime, u64)>) -> Rc<Ime> {
        let apps_path = apps_path(user_path.as_deref());
        let (apps, apps_writable) = read_apps(apps_path.as_deref());
        Rc::new(Ime {
            engine: RefCell::new(engine),
            user_path,
            en_learned_path,
            user_stamp: Cell::new(user_stamp),
            skip_logged: Cell::new(false),
            generation: Cell::new(0),
            apps: RefCell::new(apps),
            apps_path,
            apps_writable: Cell::new(apps_writable),
            #[cfg(test)]
            pressed: RefCell::new(Vec::new()),
            #[cfg(test)]
            launched: RefCell::new(Vec::new()),
        })
    }

    /// 启动时读入 `user.txt`,成功返回其 stamp。读不出时按空数据启动且不记 stamp,文件存在即不被覆盖。
    fn read_user(engine: &mut Engine, p: &Path) -> Option<(SystemTime, u64)> {
        match std::fs::read_to_string(p) {
            Ok(text) => {
                engine.set_user(UserModel::import(&normalize_user(&text)));
                stamp(p)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                log::line(&format!("读取用户数据 {p:?} 失败,重新加载成功前不写回: {e}"));
                None
            }
        }
    }

    pub fn load() -> Rc<Ime> {
        let t = std::time::Instant::now();
        let mut engine = Engine::new(load_dict(), FuzzyConfig::default());
        if let Some(p) = gram_path().filter(|p| p.exists()) {
            match std::fs::read(&p).map_err(|e| e.to_string()).and_then(Gram::from_bytes) {
                Ok(g) => engine.set_gram(g),
                Err(e) => log::line(&format!("语法模型 {p:?} 无效: {e}")),
            }
        }
        let en_learned_path = local_file("en_learned.txt");
        engine.set_english(load_english(en_learned_path.as_ref()));
        if let Some(t) = symbols_path().and_then(|p| read_optional(&p)) {
            engine.set_symbols(Symbols::parse(&t));
        }
        let user_path = user_path();
        read_fuzzy(&mut engine, user_path.as_deref());
        let user_stamp = user_path.as_deref().and_then(|p| Self::read_user(&mut engine, p));
        log::line(&format!("引擎就绪,耗时 {:?}", t.elapsed()));
        Self::build(engine, user_path, en_learned_path, user_stamp)
    }

    /// 测试用:不读盘、不存盘。
    #[cfg(test)]
    pub fn with_engine(engine: Engine) -> Rc<Ime> {
        Self::build(engine, None, None, None)
    }

    /// 测试用:用户数据读写指定文件(按启动时的规则读入),不读词典等其他文件。
    #[cfg(test)]
    pub fn with_user_path(mut engine: Engine, path: PathBuf) -> Rc<Ime> {
        read_fuzzy(&mut engine, Some(&path));
        let stamp = Self::read_user(&mut engine, &path);
        Self::build(engine, Some(path), None, stamp)
    }

    /// `user.txt` 自上次读入或写盘后是否被外部修改过(文件不存在不算)。
    pub fn user_file_changed(&self) -> bool {
        let Some(path) = &self.user_path else { return false };
        stamp(path).is_some_and(|s| self.user_stamp.get() != Some(s))
    }

    /// 当前用户数据的代际号。
    pub fn generation(&self) -> u32 {
        self.generation.get()
    }

    /// 菜单"重新加载用户词库":重读 `user.txt`(去 BOM、CRLF→LF)与英文词表。
    /// 读不出(如非 UTF-8)时不动内存数据、不更新 stamp,文件继续受保护。
    pub fn reload_user(&self) -> Result<(), String> {
        let Some(path) = &self.user_path else { return Err("没有用户数据路径".into()) };
        let text = match std::fs::read(path) {
            Ok(b) => String::from_utf8(b).map_err(|_| format!("{path:?} 不是 UTF-8 编码"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("读取 {path:?} 失败: {e}")),
        };
        let text = normalize_user(&text);
        let mut engine = self.engine.borrow_mut();
        engine.set_user(UserModel::import(&text));
        engine.set_english(load_english(self.en_learned_path.as_ref()));
        self.user_stamp.set(stamp(path));
        self.skip_logged.set(false);
        self.generation.set(self.generation.get() + 1);
        Ok(())
    }

    /// 菜单"编辑用户词库…":写盘一次(文件已被外部修改则 `save` 自动跳过)、备份为 `user.txt.bak`,
    /// 再用记事本打开。不等待记事本退出:Win11 新记事本可能并入已有窗口,启动的进程随即退出。
    pub fn edit_user(&self) {
        let Some(path) = &self.user_path else { return };
        self.save(&self.engine.borrow());
        if let Err(e) = std::fs::copy(path, path.with_extension("txt.bak")) {
            log::line(&format!("备份 {path:?} 失败: {e}"));
        }
        self.launch("notepad.exe", path);
    }

    /// 当前模糊音开关。
    pub fn fuzzy(&self) -> FuzzyConfig {
        self.engine.borrow().fuzzy().clone()
    }

    /// 首键暂存补发是否开启:`fuzzy.txt` 含 `首键暂存` 行即关闭。每次 `init` 现读,改文件后重启输入法生效。
    pub fn first_key_stash(&self) -> bool {
        !fuzzy_path(self.user_path.as_deref()).and_then(|p| read_optional(&p)).is_some_and(|t| stash_off_in(&t))
    }

    /// 模糊音子菜单:切换第 `i` 对规则,即时生效并写 `fuzzy.txt`(与 `user.txt` 同目录)。
    pub fn toggle_fuzzy(&self, i: usize) {
        let mut cfg = self.fuzzy();
        let on = !cfg.is_on(i);
        cfg.set(i, on);
        crate::log::line(&format!("菜单:模糊音 {} {}", FuzzyConfig::rule_names()[i], if on { "开" } else { "关" }));
        self.apply_fuzzy(cfg);
    }

    /// 菜单"按键纠错":切换开关,即时生效并写 `fuzzy.txt`。
    pub fn toggle_typo(&self) {
        let mut cfg = self.fuzzy();
        cfg.typo = !cfg.typo;
        crate::log::line(&format!("菜单:按键纠错 {}", if cfg.typo { "开" } else { "关" }));
        self.apply_fuzzy(cfg);
    }

    /// 启用新的模糊音/纠错配置并写 `fuzzy.txt`。
    fn apply_fuzzy(&self, cfg: FuzzyConfig) {
        let mut text = cfg.export_off();
        self.engine.borrow_mut().set_fuzzy(cfg);
        let Some(path) = fuzzy_path(self.user_path.as_deref()) else { return };
        // 托盘重写时保留用户手加的首键暂存开关
        if read_optional(&path).is_some_and(|t| stash_off_in(&t)) {
            text.push_str(STASH_OFF);
            text.push('\n');
        }
        let _ = path.parent().map(std::fs::create_dir_all);
        if let Err(e) = std::fs::write(&path, text) {
            crate::log::line(&format!("写入 {path:?} 失败: {e}"));
        }
    }

    /// 程序 `exe` 是否默认中文。
    pub fn is_chinese_app(&self, exe: &str) -> bool {
        self.apps.borrow().contains(exe)
    }

    /// 菜单"本程序默认中文":切换 `exe` 是否在表并写 `chinese-apps.txt`;返回切换后是否在表。空名不处理。
    pub fn toggle_chinese_app(&self, exe: &str) -> bool {
        if exe.is_empty() {
            return false;
        }
        let on = self.apps.borrow_mut().toggle(exe);
        log::line(&format!("菜单:本程序默认中文 {exe} {}", if on { "加入" } else { "移除" }));
        match &self.apps_path {
            Some(p) if self.apps_writable.get() => write_apps(p, &self.apps.borrow()),
            Some(p) => log::line(&format!("{p:?} 启动时读不出,只改内存不写回")),
            None => {}
        }
        on
    }

    /// 菜单"打开数据目录":目录不存在先建(从未选过词时还没有),再用资源管理器打开。
    pub fn open_data_dir(&self) {
        let Some(dir) = self.user_path.as_deref().and_then(Path::parent) else { return };
        let _ = std::fs::create_dir_all(dir);
        self.launch("explorer.exe", dir);
    }

    /// 启动外部程序,不等待。三路 stdio 置空,让子进程的标准句柄指向 NUL。测试里只记录。
    fn launch(&self, program: &str, arg: &Path) {
        #[cfg(test)]
        self.launched.borrow_mut().push(format!("{program} {}", arg.display()));
        #[cfg(not(test))]
        {
            use std::process::{Command, Stdio};
            let child = Command::new(program).arg(arg).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
            if let Err(e) = child {
                log::line(&format!("启动 {program} 失败: {e}"));
            }
        }
    }

    /// 测试用:取走已记录的外部程序启动。
    #[cfg(test)]
    pub fn take_launched(&self) -> Vec<String> {
        std::mem::take(&mut self.launched.borrow_mut())
    }

    /// 测试用:当前用户学习数据的存盘文本。
    #[cfg(test)]
    pub fn user_export(&self) -> String {
        self.engine.borrow().user().export()
    }

    /// 向应用注入一次按键(测试里只记录)。
    pub fn press(&self, vk: u16) {
        #[cfg(test)]
        self.pressed.borrow_mut().push(vk);
        #[cfg(windows)]
        #[cfg(not(test))]
        send_key_async(vk);
        #[cfg(not(any(windows, test)))]
        let _ = vk;
    }

    /// 测试用:取走已记录的模拟按键。
    #[cfg(test)]
    pub fn take_pressed(&self) -> Vec<u16> {
        std::mem::take(&mut self.pressed.borrow_mut())
    }

    pub fn candidates(&self, input: &str, context: &str) -> Vec<Candidate> {
        self.engine.borrow().candidates_after(input, context)
    }

    /// 学习一次选择(连同整句拆出的模糊词)并只存盘一次;存盘失败只记日志。
    /// `pairs` 是该候选用到的模糊规则,`prev` 是其上文,`lessons` 见 `Candidate::lessons`。
    pub fn learn(&self, typed: &str, text: &str, pairs: u16, prev: &str, lessons: &[Lesson]) {
        let mut engine = self.engine.borrow_mut();
        engine.learn(typed, text);
        engine.learn_fuzzy(pairs);
        engine.learn_next(prev, text);
        engine.learn_lessons(lessons);
        self.save(&engine);
    }

    /// 撤销一次 `learn`(含 lessons)并存盘。
    pub fn unlearn(&self, typed: &str, text: &str, pairs: u16, prev: &str, lessons: &[Lesson]) {
        let mut engine = self.engine.borrow_mut();
        engine.unlearn(typed, text, pairs, prev);
        engine.unlearn_lessons(lessons);
        self.save(&engine);
    }

    /// 删除词 `text` 的全部个人学习并存盘。
    pub fn forget(&self, text: &str) {
        let mut engine = self.engine.borrow_mut();
        engine.forget(text);
        self.save(&engine);
        self.save_english(&engine);
    }

    /// 回车原样上屏了 `input`:引擎判定可学则记一次并存盘;返回是否记了。
    pub fn learn_raw(&self, input: &str) -> bool {
        let mut engine = self.engine.borrow_mut();
        let counted = engine.learn_raw(input);
        if counted {
            self.save_english(&engine);
        }
        counted
    }

    fn save_english(&self, engine: &Engine) {
        let (Some(path), Some(en)) = (&self.en_learned_path, engine.english()) else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = write_atomic(path, en.export_raw().as_bytes()) {
            log::line(&format!("保存英文学习失败: {e}"));
        }
    }

    fn save(&self, engine: &Engine) {
        let Some(path) = &self.user_path else { return };
        if self.user_file_changed() {
            if !self.skip_logged.replace(true) {
                log::line(&format!("{path:?} 已被外部修改,重新加载前不写回"));
            }
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match write_atomic(path, engine.user().export().as_bytes()) {
            Ok(()) => {
                self.user_stamp.set(stamp(path));
                self.skip_logged.set(false);
            }
            Err(e) => log::line(&format!("保存用户数据失败: {e}")),
        }
    }
}

#[cfg(test)]
mod tests;
