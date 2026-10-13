//! 模糊音:把"用户键入的音节"展开成"词典里可能的音节",每个变体带模糊次数。
//! 规则两两成对且不传递(n=l、r=l 并存,但 n 与 r 不互通)。
use crate::pinyin::is_syllable;

/// 声母对(用户键入 ↔ 词典)。
const INITIALS: [(&str, &str); 5] = [("z", "zh"), ("c", "ch"), ("s", "sh"), ("n", "l"), ("r", "l")];
/// 韵母对。
const FINALS: [(&str, &str); 5] = [("an", "ang"), ("en", "eng"), ("in", "ing"), ("on", "ong"), ("uan", "uang")];

#[derive(Debug, Clone)]
pub struct FuzzyConfig {
    pub initials: [bool; 5],
    pub finals: [bool; 5],
}

impl Default for FuzzyConfig {
    /// 默认十对全开。
    fn default() -> Self {
        FuzzyConfig { initials: [true; 5], finals: [true; 5] }
    }
}

impl FuzzyConfig {
    pub fn none() -> Self {
        FuzzyConfig { initials: [false; 5], finals: [false; 5] }
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

/// 与 `from` 互为模糊的另一侧写法。
fn others<'a>(pairs: &[(&'a str, &'a str)], on: &[bool; 5], from: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    for (i, &(a, b)) in pairs.iter().enumerate() {
        if !on[i] {
            continue;
        }
        if a == from {
            out.push(b);
        } else if b == from {
            out.push(a);
        }
    }
    out
}

/// 返回 `(词典音节, 模糊次数)`;第一项恒为精确写法 `(s, 0)`,其余只含合法音节。
pub fn variants(s: &str, cfg: &FuzzyConfig) -> Vec<(String, u8)> {
    let (ini, fin) = split(s);
    let mut inis: Vec<(&str, u8)> = vec![(ini, 0)];
    inis.extend(others(&INITIALS, &cfg.initials, ini).into_iter().map(|x| (x, 1)));
    let mut fins: Vec<(&str, u8)> = vec![(fin, 0)];
    fins.extend(others(&FINALS, &cfg.finals, fin).into_iter().map(|x| (x, 1)));
    let mut out = vec![(s.to_string(), 0)];
    for &(i, ci) in &inis {
        for &(f, cf) in &fins {
            let cand = format!("{i}{f}");
            if ci + cf > 0 && is_syllable(&cand) {
                out.push((cand, ci + cf));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(s: &str, cfg: &FuzzyConfig) -> Vec<String> {
        variants(s, cfg).into_iter().map(|(k, _)| k).collect()
    }

    #[test]
    fn 精确写法永远第一() {
        assert_eq!(variants("zi", &FuzzyConfig::default())[0], ("zi".to_string(), 0));
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
        assert!(v.contains(&("sheng".to_string(), 2)));
        assert!(v.iter().all(|(k, _)| is_syllable(k)));
    }

    #[test]
    fn 单项开关生效() {
        let mut cfg = FuzzyConfig::none();
        cfg.initials[0] = true;
        assert_eq!(keys("zan", &cfg), ["zan", "zhan"]);
    }
}
