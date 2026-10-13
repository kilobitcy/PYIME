//! 用户学习:记住"键入的拼音 → 选了哪个词",下次提权。
//! 两层计数:(键入拼音, 词) 精确层加分大,足以压过模糊惩罚;词本身一层加分小。
use std::collections::HashMap;

/// 精确层基础加分(自然对数域):大于模糊惩罚 4,选过一次即可翻盘。
const PAIR_BASE: f64 = 8.0;
const PAIR_SLOPE: f64 = 2.0;
const WORD_BASE: f64 = 3.0;
/// 单项计数上限,防止历史无限放大。
const MAX_COUNT: u32 = 1000;

#[derive(Default)]
pub struct UserModel {
    pairs: HashMap<(String, String), u32>,
    words: HashMap<String, u32>,
}

impl UserModel {
    /// 记一次选择:用户键入 `typed`,上屏了 `word`。
    pub fn learn(&mut self, typed: &str, word: &str) {
        let p = self.pairs.entry((typed.to_string(), word.to_string())).or_insert(0);
        *p = (*p + 1).min(MAX_COUNT);
        let w = self.words.entry(word.to_string()).or_insert(0);
        *w = (*w + 1).min(MAX_COUNT);
    }

    /// `typed` 对应 `word` 的加分,没学过为 0。
    pub fn bonus(&self, typed: &str, word: &str) -> f64 {
        let mut b = 0.0;
        if let Some(&n) = self.pairs.get(&(typed.to_string(), word.to_string())) {
            b += PAIR_BASE + PAIR_SLOPE * f64::from(n).ln();
        }
        if let Some(&n) = self.words.get(word) {
            b += WORD_BASE + f64::from(n).ln();
        }
        b
    }

    /// 序列化为 `拼音⇥词⇥次数` 行,便于存盘。
    pub fn export(&self) -> String {
        let mut lines: Vec<String> = self.pairs.iter().map(|((t, w), n)| format!("{t}\t{w}\t{n}")).collect();
        lines.sort();
        lines.join("\n")
    }

    /// 读回 `export` 的结果;坏行跳过。词层计数由精确层累加得出。
    pub fn import(text: &str) -> UserModel {
        let mut m = UserModel::default();
        for line in text.lines() {
            let mut c = line.split('\t');
            let (Some(t), Some(w), Some(n)) = (c.next(), c.next(), c.next().and_then(|n| n.parse::<u32>().ok())) else {
                continue;
            };
            let n = n.min(MAX_COUNT);
            m.pairs.insert((t.to_string(), w.to_string()), n);
            let e = m.words.entry(w.to_string()).or_insert(0);
            *e = (*e + n).min(MAX_COUNT);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 没学过没有加分_学过随次数增加() {
        let mut m = UserModel::default();
        assert_eq!(m.bonus("zongguo", "中国"), 0.0);
        m.learn("zongguo", "中国");
        let one = m.bonus("zongguo", "中国");
        m.learn("zongguo", "中国");
        assert!(one > 4.0 && m.bonus("zongguo", "中国") > one);
        assert!(m.bonus("zhongguo", "中国") > 0.0 && m.bonus("zhongguo", "中国") < one); // 仅词层
    }

    #[test]
    fn 导出再导入保持一致() {
        let mut m = UserModel::default();
        m.learn("ni", "你");
        m.learn("ni", "你");
        let back = UserModel::import(&m.export());
        assert_eq!(back.bonus("ni", "你"), m.bonus("ni", "你"));
        assert_eq!(UserModel::import("坏行\n").export(), "");
    }
}
