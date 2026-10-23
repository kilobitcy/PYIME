//! 全进程共享的输入法状态:引擎(含词典与用户学习数据)及用户数据存盘。
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
    /// 测试用:记录"启动外部程序"而不真的启动。
    #[cfg(test)]
    launched: RefCell<Vec<String>>,
    /// 测试用:记录"模拟按键"而不真的发送。
    #[cfg(test)]
    pressed: RefCell<Vec<u16>>,
}

/// 向前台应用注入一次按键(按下 + 弹起),用于成对符号补全后回退/跳过光标。
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
    }
    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_EXTENDEDKEY: u32 = 1;
    const KEYEVENTF_KEYUP: u32 = 2;
    let mk = |flags| Input { kind: INPUT_KEYBOARD, ki: KeybdInput { vk, scan: 0, flags: flags | KEYEVENTF_EXTENDEDKEY, time: 0, extra: 0 }, pad: [0; 8] };
    let inputs = [mk(0), mk(KEYEVENTF_KEYUP)];
    // SAFETY: inputs 是有效的 INPUT 数组,布局与 Win32 INPUT(64 位下 40 字节)一致。
    unsafe { SendInput(2, inputs.as_ptr(), std::mem::size_of::<Input>() as i32) };
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
        Rc::new(Ime {
            engine: RefCell::new(engine),
            user_path,
            en_learned_path,
            user_stamp: Cell::new(user_stamp),
            skip_logged: Cell::new(false),
            generation: Cell::new(0),
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
                engine.set_user(UserModel::import(&text));
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
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text).replace("\r\n", "\n");
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

    /// 菜单"打开数据目录":目录不存在先建(从未选过词时还没有),再用资源管理器打开。
    pub fn open_data_dir(&self) {
        let Some(dir) = self.user_path.as_deref().and_then(Path::parent) else { return };
        let _ = std::fs::create_dir_all(dir);
        self.launch("explorer.exe", dir);
    }

    /// 启动外部程序,不等待。三路 stdio 置空,让子进程的标准句柄指向 NUL;
    /// 与 PIME 通信的管道句柄已在 `main` 启动时设为不可继承(见 `disinherit_std_handles`)。测试里只记录。
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
        send_key(vk);
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
