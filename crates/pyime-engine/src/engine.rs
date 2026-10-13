//! 候选生成:切分格 + 模糊展开 + 词典查询 + 整句 Viterbi,精确匹配优先、模糊匹配降权。
use crate::dict::Dict;
use crate::fuzzy::{partial, variants, FuzzyConfig};
use crate::pinyin::syllables_at;
use crate::user::UserModel;

/// 每次模糊替换的惩罚(自然对数域,约等于词频缩小 e^4 ≈ 55 倍)。
pub const FUZZY_PENALTY: f64 = 4.0;
/// 整句里每多一个词的代价,使"长词"优于"拆成单字"。
/// 末尾残缺音节(只打了开头)的惩罚,使同长度时完整音节的词优先。
const PARTIAL_PENALTY: f64 = 3.0;
const WORD_COST: f64 = 18.0;
/// 同一拼音键最多取前几个词条。
const PER_KEY: usize = 8;
/// 一个词最多包含的音节数。
const MAX_SYLLABLES: usize = 10;
/// 返回候选数上限。
const MAX_CANDIDATES: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub text: String,
    /// 消耗了输入的多少字节(其余留作下一段)。
    pub consumed: usize,
    /// 模糊替换总次数,0 表示完全精确。
    pub fuzzy: u8,
}

#[derive(Debug, Clone)]
struct Word {
    start: usize,
    end: usize,
    text: String,
    score: f64,
    fuzzy: u8,
}

pub struct Engine {
    dict: Dict,
    cfg: FuzzyConfig,
    user: UserModel,
}

impl Engine {
    pub fn new(dict: Dict, cfg: FuzzyConfig) -> Engine {
        Engine { dict, cfg, user: UserModel::default() }
    }

    pub fn set_user(&mut self, user: UserModel) {
        self.user = user;
    }

    pub fn user(&self) -> &UserModel {
        &self.user
    }

    /// 用户上屏了 `text`(对应键入的 `typed`),记下以便下次提权。
    pub fn learn(&mut self, typed: &str, text: &str) {
        self.user.learn(typed, text);
    }

    /// 末尾没打完的字母按音节开头匹配,词在输入末尾结束。
    fn grow_partial(&self, input: &str, start: usize, pos: usize, key: &str, fuzzy: u8, out: &mut Vec<Word>) {
        if pos >= input.len() {
            return;
        }
        for (syl, c) in partial(&input[pos..], &self.cfg) {
            let next = if key.is_empty() { syl.to_string() } else { format!("{key} {syl}") };
            let f = fuzzy + c;
            for e in self.dict.get(&next).into_iter().take(PER_KEY) {
                let bonus = self.user.bonus(&input[start..], &e.word);
                let score = f64::from(e.weight.max(1)).ln() - FUZZY_PENALTY * f64::from(f) - PARTIAL_PENALTY + bonus;
                out.push(Word { start, end: input.len(), text: e.word, score, fuzzy: f });
            }
        }
    }

    /// 收集从 `pos` 出发的所有词(沿切分格深搜,词典前缀不存在即剪枝)。
    fn grow(&self, input: &str, start: usize, pos: usize, key: &str, fuzzy: u8, depth: usize, out: &mut Vec<Word>) {
        self.grow_partial(input, start, pos, key, fuzzy, out);
        for syl in syllables_at(input, pos) {
            let end = pos + syl.len();
            for (v, c) in variants(syl, &self.cfg) {
                let next = if key.is_empty() { v } else { format!("{key} {v}") };
                if !self.dict.has_prefix(&next) {
                    continue;
                }
                let f = fuzzy + c;
                for e in self.dict.get(&next).into_iter().take(PER_KEY) {
                    let bonus = self.user.bonus(&input[start..end], &e.word);
                    let score = f64::from(e.weight.max(1)).ln() - FUZZY_PENALTY * f64::from(f) + bonus;
                    out.push(Word { start, end, text: e.word, score, fuzzy: f });
                }
                if depth + 1 < MAX_SYLLABLES {
                    self.grow(input, start, end, &next, f, depth + 1, out);
                }
            }
        }
    }

    /// 以 `input`(小写拼音)生成候选:整句最优、整段词、再是较短前缀的词。
    pub fn candidates(&self, input: &str) -> Vec<Candidate> {
        let mut words = Vec::new();
        let mut reach = vec![false; input.len() + 1];
        reach[0] = true;
        for a in 0..input.len() {
            if !reach[a] {
                continue;
            }
            for s in syllables_at(input, a) {
                reach[a + s.len()] = true;
            }
            self.grow(input, a, a, "", 0, 0, &mut words);
        }
        self.assemble(&words)
    }

    fn assemble(&self, words: &[Word]) -> Vec<Candidate> {
        let Some(far) = words.iter().map(|w| w.end).max() else { return Vec::new() };
        let mut out: Vec<Candidate> = Vec::new();
        // 整段本身就是词典词时不拼整句:单字词频量级远高于词,拼出的整句常是噪声。
        if !words.iter().any(|w| w.start == 0 && w.end == far) {
            out.extend(sentence(words, far));
        }
        let mut firsts: Vec<&Word> = words.iter().filter(|w| w.start == 0).collect();
        firsts.sort_by(|a, b| b.end.cmp(&a.end).then(b.score.total_cmp(&a.score)));
        for w in firsts {
            if out.len() >= MAX_CANDIDATES {
                break;
            }
            if !out.iter().any(|c| c.text == w.text) {
                out.push(Candidate { text: w.text.clone(), consumed: w.end, fuzzy: w.fuzzy });
            }
        }
        out
    }
}

/// 在 [0, far] 上做 Viterbi;只有拆成 ≥2 个词时才作为"整句"候选。
fn sentence(words: &[Word], far: usize) -> Option<Candidate> {
    let mut best: Vec<Option<(f64, usize)>> = vec![None; far + 1]; // (得分, 词下标)
    let mut order: Vec<usize> = (0..words.len()).collect();
    order.sort_by_key(|&i| words[i].start);
    let mut reachable = vec![false; far + 1];
    reachable[0] = true;
    for &i in &order {
        let w = &words[i];
        if !reachable[w.start] {
            continue;
        }
        let base = if w.start == 0 { 0.0 } else { best[w.start].map_or(f64::MIN, |b| b.0) };
        let total = base + w.score - WORD_COST;
        if best[w.end].is_none_or(|b| total > b.0) {
            best[w.end] = Some((total, i));
        }
        reachable[w.end] = true;
    }
    let mut pos = far;
    let (mut text, mut fuzzy, mut n) = (Vec::new(), 0u8, 0);
    while pos > 0 {
        let w = &words[best[pos]?.1];
        text.push(w.text.as_str());
        fuzzy = fuzzy.saturating_add(w.fuzzy);
        pos = w.start;
        n += 1;
    }
    (n >= 2).then(|| Candidate { text: text.iter().rev().copied().collect(), consumed: far, fuzzy })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::DictBuilder;

    fn engine(entries: &[(&str, &str, u32)]) -> Engine {
        let mut b = DictBuilder::default();
        for &(k, w, n) in entries {
            b.add(k, w, n);
        }
        Engine::new(Dict::from_bytes(b.build().unwrap()).unwrap(), FuzzyConfig::default())
    }

    fn texts(e: &Engine, input: &str) -> Vec<String> {
        e.candidates(input).into_iter().map(|c| c.text).collect()
    }

    #[test]
    fn 精确优先于同频的模糊() {
        let e = engine(&[("si", "四", 1000), ("shi", "是", 1000)]);
        assert_eq!(texts(&e, "si"), ["四", "是"]);
        assert_eq!(texts(&e, "shi"), ["是", "四"]);
    }

    #[test]
    fn 模糊词频足够高时可以胜出() {
        // 惩罚约 55 倍:高频模糊词压过极低频的精确词,但不会压过同量级的精确词
        let e = engine(&[("si", "死", 10), ("shi", "是", 100_000), ("shi", "时", 2_000)]);
        assert_eq!(texts(&e, "si")[0], "是");
        let e = engine(&[("si", "四", 5_000), ("shi", "是", 100_000)]);
        assert_eq!(texts(&e, "si")[0], "四");
    }

    #[test]
    fn 切分歧义两种都能查到() {
        let e = engine(&[("xian", "先", 500), ("xi an", "西安", 500)]);
        let t = texts(&e, "xian");
        assert!(t.contains(&"先".to_string()) && t.contains(&"西安".to_string()));
    }

    #[test]
    fn 整句由词拼成并标出消耗长度() {
        let e = engine(&[("wo", "我", 5000), ("ai", "爱", 5000), ("ni", "你", 5000), ("wo ai", "我爱", 3000)]);
        let c = e.candidates("woaini");
        assert_eq!((c[0].text.as_str(), c[0].consumed), ("我爱你", 6));
        assert!(c.iter().any(|c| c.text == "我爱" && c.consumed == 4));
    }

    #[test]
    fn 末尾不完整音节留作剩余() {
        let e = engine(&[("ni", "你", 5000)]);
        let c = e.candidates("nih");
        assert_eq!((c[0].text.as_str(), c[0].consumed), ("你", 2));
    }

    #[test]
    fn 整段有词时不拼整句() {
        let e = engine(&[("shi shi", "事实", 100), ("shi", "是", 1_000_000)]);
        assert_eq!(texts(&e, "shishi")[0], "事实");
    }

    #[test]
    fn 选过的模糊词下次排第一() {
        let mut e = engine(&[("zhong guo", "中国", 500_000), ("zong guo", "总过", 800_000)]);
        assert_eq!(texts(&e, "zongguo")[0], "总过");
        e.learn("zongguo", "中国");
        assert_eq!(texts(&e, "zongguo")[0], "中国");
        assert_eq!(texts(&e, "zhongguo")[0], "中国");
    }

    #[test]
    fn 无匹配返回空() {
        assert!(engine(&[("ni", "你", 1)]).candidates("vv").is_empty());
    }

    #[test]
    fn 末尾残缺音节命中长词且覆盖全部输入优先() {
        let e = engine(&[
            ("jing hong yi pie", "惊鸿一瞥", 4000),
            ("jin hong yi", "金宏义", 90_000),
            ("jin hong", "金红", 90_000),
        ]);
        let c = e.candidates("jinhongyip");
        assert_eq!((c[0].text.as_str(), c[0].consumed), ("惊鸿一瞥", 10));
        assert!(c.iter().any(|c| c.text == "金宏义" && c.consumed == 9));
    }

    #[test]
    fn 完整音节优先于同位置的残缺音节() {
        let e = engine(&[("ni", "你", 1000), ("nian", "年", 1500)]);
        assert_eq!(texts(&e, "ni")[0], "你");
    }
}
