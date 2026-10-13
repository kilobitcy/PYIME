//! 全进程共享的输入法状态:引擎(含词典与用户学习数据)及用户数据存盘。
use crate::log;
use pyime_engine::dict::{Dict, DictBuilder};
use pyime_engine::engine::{Candidate, Engine};
use pyime_engine::fuzzy::FuzzyConfig;
use pyime_engine::gram::Gram;
use pyime_engine::user::UserModel;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub struct Ime {
    engine: RefCell<Engine>,
    user_path: Option<PathBuf>,
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
        let user_path = user_path();
        if let Some(text) = user_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
            engine.set_user(UserModel::import(&text));
        }
        log::line(&format!("引擎就绪,耗时 {:?}", t.elapsed()));
        Rc::new(Ime { engine: RefCell::new(engine), user_path })
    }

    /// 测试用:不读盘、不存盘。
    #[cfg(test)]
    pub fn with_engine(engine: Engine) -> Rc<Ime> {
        Rc::new(Ime { engine: RefCell::new(engine), user_path: None })
    }

    pub fn candidates(&self, input: &str, context: &str) -> Vec<Candidate> {
        self.engine.borrow().candidates_after(input, context)
    }

    /// 学习一次选择并存盘;存盘失败只记日志。
    pub fn learn(&self, typed: &str, text: &str) {
        let mut engine = self.engine.borrow_mut();
        engine.learn(typed, text);
        let Some(path) = &self.user_path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path, engine.user().export()) {
            log::line(&format!("保存用户数据失败: {e}"));
        }
    }
}
