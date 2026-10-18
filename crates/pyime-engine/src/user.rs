//! 用户学习:记住"键入的拼音 → 选了哪个词",下次提权。
//! 两层计数:(键入拼音, 词) 精确层加分大,足以压过模糊惩罚;词本身一层加分小。
use std::collections::HashMap;

/// 精确层基础加分(自然对数域):大于模糊惩罚(多字 2.5、单字 3.0),选过一次即可翻盘。
const PAIR_BASE: f64 = 8.0;
const PAIR_SLOPE: f64 = 2.0;
const WORD_BASE: f64 = 3.0;
/// 模糊规则个人折扣的上限(自然对数域);默认惩罚多字 2.5、单字 3.0,用得再多也至少剩 0.5、1.0。
const FUZZY_DISCOUNT_CAP: f64 = 2.0;
/// 个人搭配加分:系数与封顶(自然对数域)。上文只取末尾至多 3 个字。
const COLLOCATION_SLOPE: f64 = 1.5;
const COLLOCATION_CAP: f64 = 5.0;
const MAX_CONTEXT: usize = 3;
/// 个人搭配条目上限,满了不再收新条目(已有的仍累加)。
const MAX_COLLOCATIONS: usize = 100_000;
/// 单项计数上限,防止历史无限放大。
const MAX_COUNT: u32 = 1000;

#[derive(Default)]
pub struct UserModel {
    pairs: HashMap<(String, String), u32>,
    words: HashMap<String, u32>,
    /// 各模糊规则被接受的次数(下标同 `fuzzy::variants` 的规则位:声母 0–4,韵母 5–9)。
    fuzzy: [u32; 10],
    /// (上文末尾 1–3 个字, 紧随其后的词) → 次数。
    next: HashMap<(String, String), u32>,
}

impl UserModel {
    /// 记一次选择:用户键入 `typed`,上屏了 `word`。
    pub fn learn(&mut self, typed: &str, word: &str) {
        let p = self.pairs.entry((typed.to_string(), word.to_string())).or_insert(0);
        *p = (*p + 1).min(MAX_COUNT);
        let w = self.words.entry(word.to_string()).or_insert(0);
        *w = (*w + 1).min(MAX_COUNT);
    }

    /// 撤销一次 `learn`(各计数减一,归零即删除)。
    pub fn unlearn(&mut self, typed: &str, word: &str) {
        dec(&mut self.pairs, &(typed.to_string(), word.to_string()));
        dec(&mut self.words, &word.to_string());
    }

    /// 撤销一次 `learn_fuzzy`。
    pub fn unlearn_fuzzy(&mut self, pairs: u16) {
        for (i, n) in self.fuzzy.iter_mut().enumerate() {
            if pairs >> i & 1 == 1 {
                *n = n.saturating_sub(1);
            }
        }
    }

    /// 撤销一次 `learn_next`。
    pub fn unlearn_next(&mut self, prev: &str, word: &str) {
        let chars: Vec<char> = prev.chars().collect();
        for k in 1..=chars.len().min(MAX_CONTEXT) {
            dec(&mut self.next, &(chars[chars.len() - k..].iter().collect::<String>(), word.to_string()));
        }
    }

    /// 删除 `word` 的全部个人学习:各拼音下的精确层、词层,以及以它为后继的搭配。词库不受影响。
    pub fn forget(&mut self, word: &str) {
        self.pairs.retain(|(_, w), _| w != word);
        self.words.remove(word);
        self.next.retain(|(_, w), _| w != word);
    }

    /// 记一次搭配:`prev` 是光标前已有的文字,其后上屏了 `word`。上文末尾 1–3 个字各记一条。
    pub fn learn_next(&mut self, prev: &str, word: &str) {
        let chars: Vec<char> = prev.chars().collect();
        for k in 1..=chars.len().min(MAX_CONTEXT) {
            let key = (chars[chars.len() - k..].iter().collect::<String>(), word.to_string());
            if self.next.len() >= MAX_COLLOCATIONS && !self.next.contains_key(&key) {
                continue;
            }
            let n = self.next.entry(key).or_insert(0);
            *n = (*n + 1).min(MAX_COUNT);
        }
    }

    /// 上文 `prev` 之后接 `word` 的个人搭配加分(≥ 0):取上文各长度中次数最多者,对数且封顶。
    pub fn collocation(&self, prev: &str, word: &str) -> f64 {
        let chars: Vec<char> = prev.chars().collect();
        let best = (1..=chars.len().min(MAX_CONTEXT))
            .filter_map(|k| self.next.get(&(chars[chars.len() - k..].iter().collect::<String>(), word.to_string())))
            .max();
        best.map_or(0.0, |&n| (COLLOCATION_SLOPE * f64::from(n + 1).ln()).min(COLLOCATION_CAP))
    }

    /// 选中的候选用到了这些模糊规则(位掩码),各记一次。
    pub fn learn_fuzzy(&mut self, pairs: u16) {
        for (i, n) in self.fuzzy.iter_mut().enumerate() {
            if pairs >> i & 1 == 1 {
                *n = (*n + 1).min(MAX_COUNT);
            }
        }
    }

    /// 这些模糊规则的惩罚折扣(≥ 0,加到得分上):每条规则 min(ln(1+接受次数), 上限)。
    pub fn fuzzy_discount(&self, pairs: u16) -> f64 {
        self.fuzzy.iter().enumerate().filter(|(i, _)| pairs >> i & 1 == 1).map(|(_, &n)| f64::from(n + 1).ln().min(FUZZY_DISCOUNT_CAP)).sum()
    }

    /// 词是否被学过(任意键入拼音)。
    pub fn knows(&self, word: &str) -> bool {
        self.words.contains_key(word)
    }

    /// 用户是否曾为拼音 `typed` 直接选过 `word`(精确层有记录)。
    pub fn chose(&self, typed: &str, word: &str) -> bool {
        self.pairs.contains_key(&(typed.to_string(), word.to_string()))
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

    /// 学过的、键入拼音是 `input` 开头的 (拼音, 词, 加分);用于补充词典外的整句。
    pub fn matching<'a>(&'a self, input: &str) -> Vec<(&'a str, &'a str, f64)> {
        let mut out = Vec::new();
        for ((t, w), _) in &self.pairs {
            if input.starts_with(t.as_str()) {
                out.push((t.as_str(), w.as_str(), self.bonus(t, w)));
            }
        }
        out
    }

    /// 序列化为 `拼音⇥词⇥次数` 行,便于存盘。
    pub fn export(&self) -> String {
        let mut lines: Vec<String> = self.pairs.iter().map(|((t, w), n)| format!("{t}\t{w}\t{n}")).collect();
        lines.sort();
        lines.extend(self.next.iter().map(|((p, w), n)| format!("@ng\t{p}\t{w}\t{n}")));
        lines.sort();
        lines.extend(self.fuzzy.iter().enumerate().filter(|(_, &n)| n > 0).map(|(i, n)| format!("@fuzzy\t{i}\t{n}")));
        lines.join("\n")
    }

    /// 读回 `export` 的结果;坏行跳过。词层计数由精确层累加得出。
    pub fn import(text: &str) -> UserModel {
        let mut m = UserModel::default();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("@ng\t") {
                let c: Vec<&str> = rest.split('\t').collect();
                if let [p, w, n] = c.as_slice() {
                    if let Ok(n) = n.parse::<u32>() {
                        m.next.insert((p.to_string(), w.to_string()), n.min(MAX_COUNT));
                    }
                }
                continue;
            }
            let mut c = line.split('\t');
            let (Some(t), Some(w), Some(n)) = (c.next(), c.next(), c.next().and_then(|n| n.parse::<u32>().ok())) else {
                continue;
            };
            let n = n.min(MAX_COUNT);
            if t == "@fuzzy" {
                if let Some(slot) = w.parse::<usize>().ok().and_then(|i| m.fuzzy.get_mut(i)) {
                    *slot = n;
                }
                continue;
            }
            m.pairs.insert((t.to_string(), w.to_string()), n);
            let e = m.words.entry(w.to_string()).or_insert(0);
            *e = (*e + n).min(MAX_COUNT);
        }
        m
    }
}

/// 计数减一,归零删除;不存在则不动。
fn dec<K: std::hash::Hash + Eq>(map: &mut HashMap<K, u32>, key: &K) {
    if let Some(n) = map.get_mut(key) {
        *n -= 1;
        if *n == 0 {
            map.remove(key);
        }
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
    fn 模糊规则折扣随次数增加且有上限_并能存取() {
        let mut m = UserModel::default();
        assert_eq!(m.fuzzy_discount(0b100), 0.0);
        m.learn_fuzzy(0b100);
        let one = m.fuzzy_discount(0b100);
        assert!(one > 0.0 && m.fuzzy_discount(0b1000) == 0.0);
        for _ in 0..1000 {
            m.learn_fuzzy(0b100);
        }
        assert_eq!(m.fuzzy_discount(0b100), FUZZY_DISCOUNT_CAP);
        let back = UserModel::import(&m.export());
        assert_eq!(back.fuzzy_discount(0b100), FUZZY_DISCOUNT_CAP);
    }

    #[test]
    fn 个人搭配随次数增加_上文越短越通用_并能存取() {
        let mut m = UserModel::default();
        assert_eq!(m.collocation("我想", "吃饭"), 0.0);
        m.learn_next("我想", "吃饭");
        let one = m.collocation("我想", "吃饭");
        assert!(one > 0.0);
        assert!(m.collocation("你想", "吃饭") > 0.0, "只共享末字也算");
        assert_eq!(m.collocation("我想", "喝水"), 0.0);
        for _ in 0..1000 {
            m.learn_next("我想", "吃饭");
        }
        assert_eq!(m.collocation("我想", "吃饭"), COLLOCATION_CAP);
        let back = UserModel::import(&m.export());
        assert_eq!(back.collocation("我想", "吃饭"), COLLOCATION_CAP);
    }

    #[test]
    fn 撤销学习回到学习前() {
        let mut m = UserModel::default();
        m.learn("zongguo", "中国");
        m.learn_fuzzy(0b100);
        m.learn_next("我", "中国");
        m.unlearn("zongguo", "中国");
        m.unlearn_fuzzy(0b100);
        m.unlearn_next("我", "中国");
        assert_eq!(m.export(), "");
        assert_eq!(m.bonus("zongguo", "中国"), 0.0);
        assert!(!m.knows("中国"));
        m.unlearn("没学过", "词"); // 不 panic
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

    #[test]
    fn forget_删除该词全部学习_不影响其他词() {
        let mut m = UserModel::default();
        m.learn("qinjixu", "亲继续");
        m.learn("qjx", "亲继续");
        m.learn("qin", "请");
        m.learn_next("是", "亲继续");
        m.learn_next("是", "请");
        m.forget("亲继续");
        assert_eq!(m.bonus("qinjixu", "亲继续"), 0.0);
        assert_eq!(m.bonus("qjx", "亲继续"), 0.0);
        assert!(!m.knows("亲继续"));
        assert_eq!(m.collocation("是", "亲继续"), 0.0);
        assert!(m.bonus("qin", "请") > 0.0 && m.collocation("是", "请") > 0.0);
    }
}
