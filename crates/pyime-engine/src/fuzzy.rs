//! 模糊音:把"用户键入的音节"展开成"词典里可能的音节",每个变体带模糊次数。
//! 规则两两成对且不传递(n=l、r=l 并存,但 n 与 r 不互通)。
use crate::pinyin::{all, canonical, is_syllable, MAX_LEN};

/// 声母对(用户键入 ↔ 词典)。
const INITIALS: [(&str, &str); 5] = [("z", "zh"), ("c", "ch"), ("s", "sh"), ("n", "l"), ("r", "l")];
/// 韵母对。
const FINALS: [(&str, &str); 5] = [("an", "ang"), ("en", "eng"), ("in", "ing"), ("on", "ong"), ("uan", "uang")];

/// `fuzzy.txt` 中表示关闭按键纠错的行。
pub const TYPO_OFF: &str = "纠错";

#[derive(Debug, Clone)]
pub struct FuzzyConfig {
    pub initials: [bool; 5],
    pub finals: [bool; 5],
    /// 按键纠错:输入没有精确整段词时查"只改一处"的整段词(见 `typo.rs`),默认开。
    pub typo: bool,
}

impl Default for FuzzyConfig {
    /// 默认十对全开,按键纠错开。
    fn default() -> Self {
        FuzzyConfig { initials: [true; 5], finals: [true; 5], typo: true }
    }
}

impl FuzzyConfig {
    pub fn none() -> Self {
        FuzzyConfig { initials: [false; 5], finals: [false; 5], typo: false }
    }

    /// 十对规则的名字(如 `z=zh`),下标即规则位:声母 0–4,韵母 5–9。
    pub fn rule_names() -> Vec<String> {
        INITIALS.iter().chain(&FINALS).map(|(a, b)| format!("{a}={b}")).collect()
    }

    pub fn is_on(&self, i: usize) -> bool {
        if i < 5 { self.initials[i] } else { self.finals[i - 5] }
    }

    pub fn set(&mut self, i: usize, on: bool) {
        if i < 5 { self.initials[i] = on } else { self.finals[i - 5] = on }
    }

    /// 读配置文本:每行一个关闭的规则名(或 `纠错`),其余开;不认识的行忽略。
    pub fn parse_off(text: &str) -> Self {
        let names = Self::rule_names();
        let mut cfg = Self::default();
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line == TYPO_OFF {
                cfg.typo = false;
            } else if let Some(i) = names.iter().position(|n| n == line) {
                cfg.set(i, false);
            }
        }
        cfg
    }

    /// 写配置文本:关闭的规则名,每行一个;关闭纠错时末尾加 `纠错` 行;全开时为空。
    pub fn export_off(&self) -> String {
        let mut s: String = Self::rule_names().into_iter().enumerate().filter(|&(i, _)| !self.is_on(i)).map(|(_, n)| n + "\n").collect();
        if !self.typo {
            s.push_str(TYPO_OFF);
            s.push('\n');
        }
        s
    }
}

/// 拆成 (声母, 韵母);零声母时声母为空。
fn split(s: &str) -> (&str, &str) {
    let n = if s.starts_with("zh") || s.starts_with("ch") || s.starts_with("sh") {
        2
    } else if s.starts_with(['a', 'e', 'o']) {
        0
    } else {
        1
    };
    s.split_at(n)
}

/// 与 `from` 互为模糊的另一侧写法,带规则位(声母 0–4,韵母 5–9,用于 `pairs` 位掩码)。
fn others<'a>(pairs: &[(&'a str, &'a str)], on: &[bool; 5], from: &str, shift: u16) -> Vec<(&'a str, u16)> {
    let mut out = Vec::new();
    for (i, &(a, b)) in pairs.iter().enumerate() {
        if !on[i] {
            continue;
        }
        if a == from {
            out.push((b, 1 << (shift + i as u16)));
        } else if b == from {
            out.push((a, 1 << (shift + i as u16)));
        }
    }
    out
}

/// 返回 `(词典音节, 模糊次数, 用到的规则位掩码)`;第一项恒为规范化(`canonical`)后的精确写法 `(s, 0, 0)`,其余只含合法音节。
pub fn variants(s: &str, cfg: &FuzzyConfig) -> Vec<(String, u8, u16)> {
    let s = canonical(s);
    let (ini, fin) = split(s);
    let mut inis: Vec<(&str, u8, u16)> = vec![(ini, 0, 0)];
    inis.extend(others(&INITIALS, &cfg.initials, ini, 0).into_iter().map(|(x, m)| (x, 1, m)));
    let mut fins: Vec<(&str, u8, u16)> = vec![(fin, 0, 0)];
    fins.extend(others(&FINALS, &cfg.finals, fin, 5).into_iter().map(|(x, m)| (x, 1, m)));
    let mut out = vec![(s.to_string(), 0, 0)];
    for &(i, ci, mi) in &inis {
        for &(f, cf, mf) in &fins {
            let cand = format!("{i}{f}");
            if ci + cf > 0 && is_syllable(&cand) {
                out.push((cand, ci + cf, mi | mf));
            }
        }
    }
    out
}

/// `input[pos..]` 的所有可作音节的前缀(由短到长):合法音节,或本身不合法、按 `cfg` 模糊后合法的写法(zhon→zhong)。
pub fn spellings_at<'a>(input: &'a str, pos: usize, cfg: &FuzzyConfig) -> Vec<&'a str> {
    let rest = &input[pos..];
    (1..=MAX_LEN.min(rest.len())).map(|n| &rest[..n]).filter(|s| is_syllable(s) || variants(s, cfg).len() > 1).collect()
}

/// 末尾没打完的字母 `rest` 可能对应的音节:`(音节, 模糊次数)`。
/// 只以"音节开头"匹配;声母套用模糊音,单独的 z/c/s 不吞掉 zh/ch/sh 开头的音节。
pub fn partial(rest: &str, cfg: &FuzzyConfig) -> Vec<(&'static str, u8, u16)> {
    let (ini, tail) = split(rest);
    let mut starts: Vec<(String, &str, u8, u16)> = vec![(rest.to_string(), ini, 0, 0)];
    for (a, m) in others(&INITIALS, &cfg.initials, ini, 0) {
        starts.push((format!("{a}{tail}"), a, 1, m));
    }
    let mut out = Vec::new();
    for t in all() {
        let t_ini = split(t).0;
        if let Some(&(_, _, c, m)) = starts.iter().find(|(pre, i, _, _)| t.starts_with(pre.as_str()) && t_ini == *i) {
            let t = canonical(t);
            match out.iter_mut().find(|(x, _, _)| *x == t) {
                Some(e) if e.1 > c => *e = (t, c, m),
                Some(_) => {}
                None => out.push((t, c, m)),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(s: &str, cfg: &FuzzyConfig) -> Vec<String> {
        variants(s, cfg).into_iter().map(|(k, _, _)| k).collect()
    }

    #[test]
    fn 精确写法永远第一() {
        assert_eq!(variants("zi", &FuzzyConfig::default())[0], ("zi".to_string(), 0, 0));
        assert_eq!(keys("zi", &FuzzyConfig::none()), ["zi"]);
    }

    #[test]
    fn 平翘舌与前后鼻音() {
        assert_eq!(keys("zi", &FuzzyConfig::default()), ["zi", "zhi"]);
        assert_eq!(keys("shan", &FuzzyConfig::default()), ["shan", "shang", "san", "sang"]);
    }

    #[test]
    fn n_l_r_不传递() {
        let k = keys("nu", &FuzzyConfig::default());
        assert!(k.contains(&"lu".to_string()) && !k.contains(&"ru".to_string()));
        let k = keys("lu", &FuzzyConfig::default());
        assert!(k.contains(&"nu".to_string()) && k.contains(&"ru".to_string()));
    }

    #[test]
    fn 同时模糊计两次且丢弃非法音节() {
        let v = variants("sen", &FuzzyConfig::default());
        assert!(v.contains(&("sheng".to_string(), 2, 1 << 2 | 1 << 6)));
        assert!(v.iter().all(|(k, _, _)| is_syllable(k)));
    }

    #[test]
    fn 单项开关生效() {
        let mut cfg = FuzzyConfig::none();
        cfg.initials[0] = true;
        assert_eq!(keys("zan", &cfg), ["zan", "zhan"]);
    }

    #[test]
    fn 规则开关按名字读写_只记关闭的() {
        assert_eq!(FuzzyConfig::rule_names()[3], "n=l");
        assert_eq!(FuzzyConfig::rule_names()[9], "uan=uang");
        let mut cfg = FuzzyConfig::default();
        assert_eq!(cfg.export_off(), "");
        cfg.set(4, false);
        cfg.set(8, false);
        assert!(!cfg.is_on(4) && cfg.is_on(5));
        assert_eq!(cfg.export_off(), "r=l\non=ong\n");
        // 未知名字、空行、注释、BOM、CRLF 忽略
        let back = FuzzyConfig::parse_off("\u{feff}# 关闭的模糊音\r\nr=l\r\n\r\nx=y\non=ong");
        assert_eq!((back.initials, back.finals), (cfg.initials, cfg.finals));
    }

    #[test]
    fn 只有模糊后才合法的写法也能切出() {
        // zhon 本身不是音节,on=ong 后为 zhong;zuang 经 z=zh 为 zhuang
        assert_eq!(spellings_at("zhonwen", 0, &FuzzyConfig::default()), ["zhon"]);
        assert_eq!(spellings_at("zuang", 0, &FuzzyConfig::default()), ["zu", "zua", "zuan", "zuang"]);
        assert_eq!(spellings_at("zhonwen", 0, &FuzzyConfig::none()), Vec::<&str>::new());
    }

    #[test]
    fn 末尾残缺音节按开头匹配并套用声母模糊() {
        let got = partial("p", &FuzzyConfig::default());
        assert!(got.contains(&("pie", 0, 0)) && got.contains(&("pian", 0, 0)) && !got.iter().any(|(t, _, _)| t.starts_with('b')));
        let got = partial("s", &FuzzyConfig::default());
        assert!(got.contains(&("si", 0, 0)) && got.contains(&("shi", 1, 1 << 2)));
        let got = partial("sh", &FuzzyConfig::none());
        assert!(got.contains(&("shi", 0, 0)) && !got.iter().any(|(t, _, _)| *t == "si"));
    }

    #[test]
    fn lue_nue_按规范写法展开_残缺前缀也映射() {
        assert_eq!(variants("lue", &FuzzyConfig::none())[0], ("lve".to_string(), 0, 0));
        assert!(variants("nue", &FuzzyConfig::default()).contains(&("lve".to_string(), 1, 1 << 3)));
        let got = partial("lu", &FuzzyConfig::none());
        assert!(got.contains(&("lve", 0, 0)) && !got.iter().any(|(t, _, _)| *t == "lue"));
    }

    #[test]
    fn 纠错开关按名字读写() {
        let mut cfg = FuzzyConfig::default();
        assert!(cfg.typo && !FuzzyConfig::none().typo);
        cfg.typo = false;
        cfg.set(4, false);
        assert_eq!(cfg.export_off(), "r=l\n纠错\n");
        let back = FuzzyConfig::parse_off("\u{feff}纠错\r\nr=l\r\n");
        assert!(!back.typo && !back.is_on(4) && back.is_on(3));
        assert!(FuzzyConfig::parse_off("r=l\n").typo);
    }
}
