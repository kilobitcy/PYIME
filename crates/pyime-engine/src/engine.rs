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
/// 词典外的用户短语(整句选过的)基础得分,再加学习加分。
const USER_PHRASE_BASE: f64 = 7.0;
/// 单字词频(rime-ice 里按字累计)量级高于词,折扣后再与词比较。
const SINGLE_CHAR_DISCOUNT: f64 = 3.0;
const WORD_COST: f64 = 18.0;
/// 同一拼音键最多取前几个词条。
const PER_KEY: usize = 8;
/// 一个词最多包含的音节数。
const MAX_SYLLABLES: usize = 10;
/// 返回候选数上限。
/// 整句候选最多几条。
const SENTENCES: usize = 3;
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
                let score = f64::from(e.weight.max(1)).ln() - FUZZY_PENALTY * f64::from(f) - PARTIAL_PENALTY + bonus - if key.is_empty() { SINGLE_CHAR_DISCOUNT } else { 0.0 };
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
                    let score = f64::from(e.weight.max(1)).ln() - FUZZY_PENALTY * f64::from(f) + bonus - if depth == 0 { SINGLE_CHAR_DISCOUNT } else { 0.0 };
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
        for (typed, text, bonus) in self.user.matching(input) {
            if !words.iter().any(|w| w.start == 0 && w.end == typed.len() && w.text == text) {
                words.push(Word { start: 0, end: typed.len(), text: text.to_string(), score: USER_PHRASE_BASE + bonus, fuzzy: 0 });
            }
        }
        self.assemble(&words)
    }

    fn assemble(&self, words: &[Word]) -> Vec<Candidate> {
        let Some(far) = words.iter().map(|w| w.end).max() else { return Vec::new() };
        let scored = sentences(words, far);
        let top_sentence = scored.first().map(|s| s.1);
        let sents: Vec<Candidate> = scored.into_iter().map(|s| s.0).collect();
        let mut firsts: Vec<&Word> = words.iter().filter(|w| w.start == 0).collect();
        firsts.sort_by(|a, b| b.end.cmp(&a.end).then(b.score.total_cmp(&a.score)));
        let as_cand = |w: &&Word| Candidate { text: w.text.clone(), consumed: w.end, fuzzy: w.fuzzy };
        let (whole, rest): (Vec<&Word>, Vec<&Word>) = firsts.into_iter().partition(|w| w.end == far);
        // 没有精确的整段词、整句完全精确且得分高于最佳整段词时,整句排最前;否则整句紧跟整段词
        let exact_whole = whole.iter().any(|w| w.fuzzy == 0);
        let best_whole = whole.iter().map(|w| w.score - WORD_COST).fold(f64::MIN, f64::max);
        let front = !exact_whole && sents.first().is_some_and(|s| s.fuzzy == 0) && top_sentence.is_some_and(|t| t > best_whole);
        let groups: [Vec<Candidate>; 3] = if front {
            [sents, whole.iter().map(as_cand).collect(), rest.iter().map(as_cand).collect()]
        } else {
            [whole.iter().map(as_cand).collect(), sents, rest.iter().map(as_cand).collect()]
        };
        let mut out: Vec<Candidate> = Vec::new();
        for c in groups.into_iter().flatten() {
            if out.len() < MAX_CANDIDATES && !out.iter().any(|o| o.text == c.text) {
                out.push(c);
            }
        }
        out
    }
}

/// 在 [0, far] 上做 k 最优 Viterbi,返回至多 `SENTENCES` 条拆成 ≥2 个词的整句(得分从高到低)。
fn sentences(words: &[Word], far: usize) -> Vec<(Candidate, f64)> {
    // best[pos] = 到达 pos 的前 SENTENCES 条路径:(得分, 词下标, 前驱位置上的名次)
    let mut best: Vec<Vec<(f64, usize, usize)>> = vec![Vec::new(); far + 1];
    let mut order: Vec<usize> = (0..words.len()).filter(|&i| words[i].end <= far).collect();
    order.sort_by_key(|&i| words[i].start);
    for &i in &order {
        let w = &words[i];
        let prevs: Vec<(f64, usize)> = if w.start == 0 {
            vec![(0.0, 0)]
        } else {
            best[w.start].iter().enumerate().map(|(r, p)| (p.0, r)).collect()
        };
        for (base, rank) in prevs {
            best[w.end].push((base + w.score - WORD_COST, i, rank));
        }
        best[w.end].sort_by(|a, b| b.0.total_cmp(&a.0));
        best[w.end].truncate(SENTENCES);
    }
    let mut out: Vec<(Candidate, f64)> = Vec::new();
    for r in 0..best[far].len() {
        let (mut pos, mut rank) = (far, r);
        let (mut parts, mut fuzzy) = (Vec::new(), 0u8);
        while pos > 0 {
            let (_, wi, prev_rank) = best[pos][rank];
            let w = &words[wi];
            parts.push(w.text.as_str());
            fuzzy = fuzzy.saturating_add(w.fuzzy);
            (pos, rank) = (w.start, prev_rank);
        }
        let text: String = parts.iter().rev().copied().collect();
        if parts.len() >= 2 && !out.iter().any(|c| c.0.text == text) {
            out.push((Candidate { text, consumed: far, fuzzy }, best[far][r].0));
        }
    }
    out
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

    #[test]
    fn 整句选过后成为短语并排第一() {
        let mut e = engine(&[
            ("zai", "在", 900_000), ("zai", "再", 130_000), ("zhui jia", "追加", 60_000),
            ("zhui", "追", 50_000), ("jia", "加", 70_000),
        ]);
        let t = texts(&e, "zaizhuijia");
        assert_eq!(t[0], "在追加");
        assert!(t.contains(&"再追加".to_string()));
        e.learn("zaizhuijia", "再追加");
        assert_eq!(texts(&e, "zaizhuijia")[0], "再追加");
    }
}
