//! 全进程共享的输入法状态:引擎(含词典与用户学习数据)及用户数据存盘。
use crate::log;
use pyime_engine::dict::{Dict, DictBuilder};
use pyime_engine::english::{parse_compiled, parse_list, English, TIER_CUSTOM};
use pyime_engine::engine::{Candidate, Engine, Lesson};
use pyime_engine::fuzzy::FuzzyConfig;
use pyime_engine::gram::Gram;
use pyime_engine::user::UserModel;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub struct Ime {
    engine: RefCell<Engine>,
    user_path: Option<PathBuf>,
    en_learned_path: Option<PathBuf>,
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

/// 用户数据目录 `%LOCALAPPDATA%\PYIME` 下的文件。
fn local_file(name: &str) -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("PYIME").join(name))
}

/// 英文词表:安装目录 `pyime.en` + 用户 `en.txt`(档位 0),再读回车学习计数;缺失的来源跳过。
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
        let user_path = user_path();
        if let Some(text) = user_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
            engine.set_user(UserModel::import(&text));
        }
        log::line(&format!("引擎就绪,耗时 {:?}", t.elapsed()));
        Rc::new(Ime { engine: RefCell::new(engine), user_path, en_learned_path, #[cfg(test)] pressed: RefCell::new(Vec::new()) })
    }

    /// 测试用:不读盘、不存盘。
    #[cfg(test)]
    pub fn with_engine(engine: Engine) -> Rc<Ime> {
        Rc::new(Ime { engine: RefCell::new(engine), user_path: None, en_learned_path: None, pressed: RefCell::new(Vec::new()) })
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
        if let Err(e) = std::fs::write(path, en.export_raw()) {
            log::line(&format!("保存英文学习失败: {e}"));
        }
    }

    fn save(&self, engine: &Engine) {
        let Some(path) = &self.user_path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path, engine.user().export()) {
            log::line(&format!("保存用户数据失败: {e}"));
        }
    }
}
