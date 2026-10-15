//! 候选生成:切分格 + 模糊展开 + 词典查询 + 整句 Viterbi,精确匹配优先、模糊匹配降权。
use crate::dict::Dict;
use crate::gram::Gram;
use crate::fuzzy::{partial, variants, FuzzyConfig};
use crate::pinyin::{is_syllable, syllables_at};
use crate::user::UserModel;

/// 每次模糊替换的惩罚(自然对数域,约等于词频缩小 e^3 ≈ 20 倍)。
pub const FUZZY_PENALTY: f64 = 3.0;
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
/// 束宽:整句搜索每个位置保留的路径数。
const BEAM: usize = 12;
/// 模糊/残缺路径的束宽。
const LOOSE_BEAM: usize = 8;
/// 语法模型搭配加分的权重(模型值是 ln 次数,单字词频偏高,需放大才能纠正)。
pub const GRAM_THRESHOLD: f64 = 0.0;
pub const GRAM_WEIGHT: f64 = 0.6;
const MAX_CANDIDATES: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub text: String,
    /// 消耗了输入的多少字节(其余留作下一段)。
    pub consumed: usize,
    /// 模糊替换总次数,0 表示完全精确。
    pub fuzzy: u8,
    /// 用到的模糊规则位掩码(声母 0–4,韵母 5–9),供选中后学习个人偏好。
    pub pairs: u16,
    /// 消耗的输入里音节边界的字节位置(不含 0 与 `consumed`),供组字串显示 `de'dao`。
    /// 词典外的用户短语没有切分,为空。
    pub cuts: Vec<usize>,
}

#[derive(Debug, Clone)]
struct Word {
    start: usize,
    end: usize,
    text: String,
    score: f64,
    fuzzy: u8,
    pairs: u16,
    /// 词内部音节边界的字节位置(不含起止)。
    cuts: Vec<usize>,
    /// 末尾残缺音节凑出来的词,不算"精确"。
    partial: bool,
}

/// 沿切分格累积的模糊状态:次数、个人偏好折扣、用到的规则位。
#[derive(Clone, Copy, Default)]
struct Acc {
    fuzzy: u8,
    discount: f64,
    pairs: u16,
}

pub struct Engine {
    dict: Dict,
    fuzzy_penalty: f64,
    cfg: FuzzyConfig,
    user: UserModel,
    gram: Option<Gram>,
    gram_weight: f64,
    gram_threshold: f64,
}

impl Engine {
    pub fn new(dict: Dict, cfg: FuzzyConfig) -> Engine {
        Engine { dict, fuzzy_penalty: FUZZY_PENALTY, cfg, user: UserModel::default(), gram: None, gram_weight: GRAM_WEIGHT, gram_threshold: GRAM_THRESHOLD }
    }

    pub fn set_gram(&mut self, gram: Gram) {
        self.gram = Some(gram);
    }

    pub fn set_gram_weight(&mut self, w: f64) {
        self.gram_weight = w;
    }

    pub fn set_gram_threshold(&mut self, t: f64) {
        self.gram_threshold = t;
    }

    fn gram_bonus(&self, ctx: &str, w: &str) -> f64 {
        let gram = self.gram.as_ref().map_or(0.0, |g| self.gram_weight * (g.bonus(ctx, w) - self.gram_threshold).max(0.0));
        gram + self.user.collocation(ctx, w)
    }

    pub fn set_fuzzy_penalty(&mut self, p: f64) {
        self.fuzzy_penalty = p;
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

    /// 撤销一次选择产生的全部学习(`learn` / `learn_fuzzy` / `learn_next`)。
    pub fn unlearn(&mut self, typed: &str, text: &str, pairs: u16, prev: &str) {
        self.user.unlearn(typed, text);
        self.user.unlearn_fuzzy(pairs);
        if !prev.is_empty() {
            self.user.unlearn_next(prev, text);
        }
    }

    /// 删除词 `text` 的全部个人学习(词库不受影响)。
    pub fn forget(&mut self, text: &str) {
        self.user.forget(text);
    }

    /// 在上文 `prev` 之后上屏了 `text`,记下个人搭配。
    pub fn learn_next(&mut self, prev: &str, text: &str) {
        self.user.learn_next(prev, text);
    }

    /// 选中的候选用到了这些模糊规则,记下以便降低这几对的惩罚。
    pub fn learn_fuzzy(&mut self, pairs: u16) {
        self.user.learn_fuzzy(pairs);
    }

    /// 同一键取词频前 `PER_KEY` 个,再补上更靠后但用户学过的词(否则截断会让学习加分落空)。
    fn top_entries(&self, key: &str) -> Vec<crate::dict::Entry> {
        self.dict.get(key).into_iter().enumerate().filter(|(i, e)| *i < PER_KEY || self.user.knows(&e.word)).map(|(_, e)| e).collect()
    }

    /// 再走一个音节:累加模糊次数、规则位,以及这些规则的个人偏好折扣。
    fn step(&self, acc: Acc, count: u8, pairs: u16) -> Acc {
        Acc { fuzzy: acc.fuzzy + count, discount: acc.discount + self.user.fuzzy_discount(pairs), pairs: acc.pairs | pairs }
    }

    /// 末尾没打完的字母按音节开头匹配,词在输入末尾结束。
    fn grow_partial(&self, input: &str, start: usize, pos: usize, key: &str, acc: Acc, cuts: &[usize], out: &mut Vec<Word>) {
        if pos >= input.len() {
            return;
        }
        let rest = &input[pos..];
        // 本身就是完整音节时,它(及其模糊写法)已由 grow 按完整音节处理,不再当残缺重复一遍
        let complete: Vec<String> = if is_syllable(rest) { variants(rest, &self.cfg).into_iter().map(|v| v.0).collect() } else { Vec::new() };
        for (syl, c, m) in partial(rest, &self.cfg) {
            if complete.iter().any(|k| k == syl) {
                continue;
            }
            let next = if key.is_empty() { syl.to_string() } else { format!("{key} {syl}") };
            let a = self.step(acc, c, m);
            for e in self.top_entries(&next) {
                let bonus = self.user.bonus(&input[start..], &e.word);
                let score = f64::from(e.weight.max(1)).ln() - self.fuzzy_penalty * f64::from(a.fuzzy) + a.discount - PARTIAL_PENALTY + bonus - if key.is_empty() { SINGLE_CHAR_DISCOUNT } else { 0.0 };
                out.push(Word { start, end: input.len(), text: e.word, score, fuzzy: a.fuzzy, pairs: a.pairs, cuts: cuts.to_vec(), partial: true });
            }
        }
    }

    /// 收集从 `pos` 出发的所有词(沿切分格深搜,词典前缀不存在即剪枝)。
    fn grow(&self, input: &str, start: usize, pos: usize, key: &str, acc: Acc, depth: usize, cuts: &mut Vec<usize>, out: &mut Vec<Word>) {
        self.grow_partial(input, start, pos, key, acc, cuts, out);
        for syl in syllables_at(input, pos) {
            let end = pos + syl.len();
            for (v, c, m) in variants(syl, &self.cfg) {
                let next = if key.is_empty() { v } else { format!("{key} {v}") };
                if !self.dict.has_prefix(&next) {
                    continue;
                }
                let a = self.step(acc, c, m);
                for e in self.top_entries(&next) {
                    let bonus = self.user.bonus(&input[start..end], &e.word);
                    let score = f64::from(e.weight.max(1)).ln() - self.fuzzy_penalty * f64::from(a.fuzzy) + a.discount + bonus - if depth == 0 { SINGLE_CHAR_DISCOUNT } else { 0.0 };
                    out.push(Word { start, end, text: e.word, score, fuzzy: a.fuzzy, pairs: a.pairs, cuts: cuts.clone(), partial: false });
                }
                if depth + 1 < MAX_SYLLABLES {
                    cuts.push(end);
                    self.grow(input, start, end, &next, a, depth + 1, cuts, out);
                    cuts.pop();
                }
            }
        }
    }

    /// 以 `input`(小写拼音)生成候选:整句最优、整段词、再是较短前缀的词。
    pub fn candidates(&self, input: &str) -> Vec<Candidate> {
        self.candidates_after(input, "")
    }

    /// 同 `candidates`,`context` 是光标前已上屏的文字,用于首词的搭配加分。
    pub fn candidates_after(&self, input: &str, context: &str) -> Vec<Candidate> {
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
            self.grow(input, a, a, "", Acc::default(), 0, &mut Vec::new(), &mut words);
        }
        for (typed, text, bonus) in self.user.matching(input) {
            if !words.iter().any(|w| w.start == 0 && w.end == typed.len() && w.text == text) {
                words.push(Word { start: 0, end: typed.len(), text: text.to_string(), score: USER_PHRASE_BASE + bonus, fuzzy: 0, pairs: 0, cuts: Vec::new(), partial: false });
            }
        }
        if !context.is_empty() {
            for w in words.iter_mut().filter(|w| w.start == 0) {
                w.score += self.gram_bonus(context, &w.text);
            }
        }
        self.assemble(&words)
    }

    fn assemble(&self, words: &[Word]) -> Vec<Candidate> {
        let Some(far) = words.iter().map(|w| w.end).max() else { return Vec::new() };
        let bigram = |ctx: &str, w: &str| self.gram_bonus(ctx, w);
        let scored = sentences(words, far, &bigram);
        let top_sentence = scored.first().map(|s| s.1);
        let top_exact = scored.first().is_some_and(|s| s.2);
        let sents: Vec<Candidate> = scored.into_iter().map(|s| s.0).collect();
        let mut firsts: Vec<&Word> = words.iter().filter(|w| w.start == 0).collect();
        firsts.sort_by(|a, b| b.end.cmp(&a.end).then(b.score.total_cmp(&a.score)));
        let as_cand = |w: &&Word| Candidate { text: w.text.clone(), consumed: w.end, fuzzy: w.fuzzy, pairs: w.pairs, cuts: w.cuts.clone() };
        let (whole, rest): (Vec<&Word>, Vec<&Word>) = firsts.into_iter().partition(|w| w.end == far);
        // 没有精确的整段词、整句完全精确且得分高于最佳整段词时,整句排最前;否则整句紧跟整段词
        let exact_whole = whole.iter().any(|w| w.fuzzy == 0);
        let best_whole = whole.iter().map(|w| w.score - WORD_COST).fold(f64::MIN, f64::max);
        let front = !exact_whole && top_exact && top_sentence.is_some_and(|t| t > best_whole);
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

/// 在 [0, far] 上做束搜索 Viterbi,返回至多 `SENTENCES` 条拆成 ≥2 个词的整句(得分从高到低)。
/// `bigram(上文, 词)` 给相邻词的搭配加分。
fn sentences(words: &[Word], far: usize, bigram: &dyn Fn(&str, &str) -> f64) -> Vec<(Candidate, f64, bool)> {
    // best[pos] = 到达 pos 的前 BEAM 条路径:(得分, 词下标, 前驱位置上的名次, 路径末尾几个字)
    let mut best: Vec<Vec<(f64, usize, usize, String, bool)>> = vec![Vec::new(); far + 1];
    let mut order: Vec<usize> = (0..words.len()).filter(|&i| words[i].end <= far).collect();
    order.sort_by_key(|&i| words[i].start);
    for &i in &order {
        let w = &words[i];
        let mut next = Vec::new();
        if w.start == 0 {
            next.push((w.score - WORD_COST, i, 0, tail(&w.text), w.fuzzy > 0 || w.partial));
        } else {
            for (r, p) in best[w.start].iter().enumerate() {
                let total = p.0 + w.score - WORD_COST + bigram(&p.3, &w.text);
                next.push((total, i, r, tail(&format!("{}{}", p.3, w.text)), p.4 || w.fuzzy > 0 || w.partial));
            }
        }
        best[w.end].extend(next);
        best[w.end].sort_by(|a, b| b.0.total_cmp(&a.0));
        // 精确路径与模糊/残缺路径分开留束,避免模糊路径被大量精确噪声挤出
        let (mut exact, loose): (Vec<_>, Vec<_>) = std::mem::take(&mut best[w.end]).into_iter().partition(|p| !p.4);
        exact.truncate(BEAM);
        exact.extend(loose.into_iter().take(LOOSE_BEAM));
        exact.sort_by(|a, b| b.0.total_cmp(&a.0));
        best[w.end] = exact;
    }
    let mut out: Vec<(Candidate, f64, bool)> = Vec::new();
    for r in 0..best[far].len() {
        let (mut pos, mut rank) = (far, r);
        let (mut parts, mut fuzzy, mut pairs, mut partial, mut cuts) = (Vec::new(), 0u8, 0u16, false, Vec::new());
        while pos > 0 {
            let (_, wi, prev_rank, _, _) = &best[pos][rank];
            let w = &words[*wi];
            parts.push(w.text.as_str());
            fuzzy = fuzzy.saturating_add(w.fuzzy);
            pairs |= w.pairs;
            cuts.extend(w.cuts.iter().copied());
            if w.start > 0 {
                cuts.push(w.start);
            }
            partial |= w.partial;
            (pos, rank) = (w.start, *prev_rank);
        }
        let text: String = parts.iter().rev().copied().collect();
        if parts.len() >= 2 && out.len() < BEAM && !out.iter().any(|c| c.0.text == text) {
            out.push(({
                cuts.sort_unstable();
                Candidate { text, consumed: far, fuzzy, pairs, cuts }
            }, best[far][r].0, fuzzy == 0 && !partial));
        }
    }
    // 三档:精确 / 仅模糊 / 含残缺音节;各档按得分,取 3+2+1 条,免得被前一档的噪声挤掉
    let class = |s: &(Candidate, f64, bool)| if s.2 { 0 } else if s.0.fuzzy > 0 { 1 } else { 2 };
    let mut picked = Vec::new();
    for (c, quota) in [(0, SENTENCES), (1, 2), (2, 1)] {
        picked.extend(out.iter().filter(|s| class(s) == c).take(quota).cloned());
    }
    picked
}

/// 取末尾 3 个字作为下一词的上文。
fn tail(s: &str) -> String {
    let n = s.chars().count();
    s.chars().skip(n.saturating_sub(3)).collect()
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
        // 惩罚约 20 倍:高频模糊词压过极低频的精确词,但不会压过同量级的精确词
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

    #[test]
    fn 词频靠后的学过词在句中也能进候选() {
        // "shi" 下有 10 个词,学过的 "氏" 词频排第 10,超出 PER_KEY
        let mut ents = vec![("ni", "你", 5000)];
        let words = ["是", "时", "事", "市", "十", "石", "室", "世", "食", "氏"];
        let mut v: Vec<(&str, &str, u32)> = ents.drain(..).collect();
        for (i, w) in words.iter().enumerate() {
            v.push(("shi", w, 5000 - i as u32 * 10));
        }
        let mut e = engine(&v);
        assert_eq!(texts(&e, "nishi")[0], "你是");
        e.learn("shi", "氏");
        assert_eq!(texts(&e, "nishi")[0], "你氏");
    }

    #[test]
    fn 个人模糊偏好降低该规则惩罚() {
        // 之 的词频是 字 的 e^3 倍,惩罚 4 时输给精确的 字;接受两次 z=zh 后折扣约 1.1,反超
        let mut e = engine(&[("zi", "字", 1000), ("zhi", "之", 20_000)]);
        assert_eq!(texts(&e, "zi")[0], "字");
        let c = e.candidates("zi").into_iter().find(|c| c.text == "之").unwrap();
        assert_eq!(c.pairs, 1);
        e.learn_fuzzy(c.pairs);
        e.learn_fuzzy(c.pairs);
        assert_eq!(texts(&e, "zi")[0], "之");
    }

    #[test]
    fn 惩罚可调() {
        let mut e = engine(&[("zi", "字", 1000), ("zhi", "之", 20_000)]);
        e.set_fuzzy_penalty(2.0);
        assert_eq!(texts(&e, "zi")[0], "之");
    }

    #[test]
    fn 个人搭配只在对应上文下提权() {
        let mut e = engine(&[("shi", "是", 5000), ("shi", "市", 4900)]);
        assert_eq!(e.candidates_after("shi", "我")[0].text, "是");
        e.learn_next("上海", "市");
        e.learn_next("上海", "市");
        assert_eq!(e.candidates_after("shi", "上海")[0].text, "市");
        assert_eq!(e.candidates_after("shi", "我")[0].text, "是");
        assert_eq!(e.candidates("shi")[0].text, "是");
    }

    #[test]
    fn 候选带音节边界() {
        let e = engine(&[("xian", "先", 500), ("xi an", "西安", 500), ("wo", "我", 5000), ("ai", "爱", 5000), ("ni", "你", 5000)]);
        let c = e.candidates("xian");
        assert_eq!(c.iter().find(|c| c.text == "先").unwrap().cuts, Vec::<usize>::new());
        assert_eq!(c.iter().find(|c| c.text == "西安").unwrap().cuts, [2]);
        let s = e.candidates("woaini");
        assert_eq!((s[0].text.as_str(), s[0].cuts.as_slice()), ("我爱你", [2usize, 4].as_slice()));
    }

    #[test]
    fn 残缺音节前的边界() {
        let e = engine(&[("ni hao", "你好", 5000), ("ni", "你", 5000)]);
        let c = e.candidates("nih");
        let w = c.iter().find(|c| c.text == "你好").unwrap();
        assert_eq!((w.consumed, w.cuts.as_slice()), (3, [2usize].as_slice()));
    }

    #[test]
    fn lue_与_lve_等价_残缺前缀也能召回() {
        let e = engine(&[("ce lve", "策略", 1000), ("ce", "测", 500), ("lu", "路", 500)]);
        assert_eq!(texts(&e, "celue")[0], "策略");
        assert!(texts(&e, "chelue").contains(&"策略".to_string()));
        assert!(texts(&e, "celu").contains(&"策略".to_string()));
    }

    #[test]
    fn forget_后恢复词库排序() {
        let mut e = engine(&[("ni", "你", 1000), ("ni", "泥", 10)]);
        e.learn("ni", "泥");
        assert_eq!(texts(&e, "ni")[0], "泥");
        e.forget("泥");
        assert_eq!(texts(&e, "ni"), ["你", "泥"]);
    }
}
