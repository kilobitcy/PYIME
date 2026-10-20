//! 候选生成:切分格 + 模糊展开 + 词典查询 + 整句 Viterbi,精确匹配优先、模糊匹配降权。
use crate::dict::Dict;
use crate::english::{English, Hit, TIER_CUSTOM};
use crate::gram::Gram;
use crate::fuzzy::{partial, variants, FuzzyConfig};
use crate::pinyin::{is_syllable, steals, syllables_at};
use crate::user::UserModel;

/// 多字词每次模糊替换的惩罚(自然对数域,约等于词频缩小 e^2.5 ≈ 12 倍)。
pub const FUZZY_PENALTY: f64 = 2.5;
/// 单字词模糊惩罚的额外部分:单字里"了""是"等虚词词频极高,需更高惩罚(默认合计 3.0)。
const SINGLE_CHAR_FUZZY_EXTRA: f64 = 0.5;
/// 整句里每多一个词的代价,使"长词"优于"拆成单字"。
/// 末尾残缺音节(只打了开头)的惩罚,使同长度时完整音节的词优先。
const PARTIAL_PENALTY: f64 = 3.0;
/// 每处"抢音"切分(xi|an、da|n,见 `pinyin::steals`)的惩罚。
const SPLIT_PENALTY: f64 = 8.0;
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
/// 英文候选落位:残缺通时完全匹配、档位 0 补全各至多几条(拼音不通时不限)。
const EN_PARTIAL_EXACT: usize = 2;
const EN_PARTIAL_COMPLETE: usize = 2;
/// 英文插入位置的下标(第 3 位)。
const EN_SLOT: usize = 2;
/// 精确通时英文完全匹配至少几个字母才出(档位 0 不限)。
const EN_EXACT_MIN: usize = 4;

/// 选中整句时顺带学习的一项:(拼音片段, 词, 上文)。
pub type Lesson = (String, String, String);

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
    /// 整句中经模糊匹配得到的完整词:(起, 止, 句中字位置, 词),起止为输入中的字节位置,
    /// 句中字位置为该词之前的字数。只有整句候选填写,供选中后逐词学习。
    pub parts: Vec<(usize, usize, usize, String)>,
    /// 候选来源,供评测按来源分桶。
    pub source: Source,
}

/// 候选来源:整段词(词典或用户短语吃满输入)、整句(Viterbi 拼装)、前缀词(只吃前一段)、英文(文字全为 ASCII)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Whole,
    Sentence,
    Prefix,
    English,
}

impl Candidate {
    /// 选中本候选时顺带学习的模糊词。`typed` 为本候选消耗的拼音(`input[..consumed]`),
    /// 上文取 `context` 加句中该词之前文字的末尾 3 个字。
    pub fn lessons(&self, typed: &str, context: &str) -> Vec<Lesson> {
        self.parts
            .iter()
            .map(|(s, e, at, w)| {
                let before: String = self.text.chars().take(*at).collect();
                (typed[*s..*e].to_string(), w.clone(), tail(&format!("{context}{before}")))
            })
            .collect()
    }
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
    /// 最后一个音节的起点(用户短语无切分,取 `end`),供判断与下一词之间是否抢音。
    last: usize,
    /// 用户曾为这段拼音直接选过该词:模糊得来也按精确参与整句分档。
    learned: bool,
}

impl Word {
    /// 整句分档时是否算"不精确":未学过的模糊词,或残缺音节凑出的词。
    fn loose(&self) -> bool {
        (self.fuzzy > 0 && !self.learned) || self.partial
    }
}

/// 沿切分格累积的状态:模糊次数、个人偏好折扣、用到的规则位、抢音处数,以及上一音节起点。
#[derive(Clone, Copy, Default)]
struct Acc {
    fuzzy: u8,
    discount: f64,
    pairs: u16,
    /// 词内抢音切分的处数。
    splits: u8,
    /// 词内上一音节的起点(尚无音节时为词首)。
    prev: usize,
}

/// 中文候选吃满输入的情况,决定英文候选落位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cover {
    /// 没有吃满输入的中文。
    None,
    /// 吃满输入的路径都含残缺音节词。
    Partial,
    /// 有不含残缺音节词的路径吃满输入(模糊词允许)。
    Exact,
}

/// 沿词的起止做可达性:只走完整词能到末尾为精确通,走上残缺词才到为残缺通。
fn cover(words: &[Word], len: usize) -> Cover {
    let (mut exact, mut any) = (vec![false; len + 1], vec![false; len + 1]);
    exact[0] = true;
    any[0] = true;
    // 英文文字的词(选过的英文用户短语)不算拼音覆盖
    let mut ws: Vec<&Word> = words.iter().filter(|w| w.end <= len && !w.text.is_ascii()).collect();
    ws.sort_by_key(|w| w.start);
    for w in ws {
        if any[w.start] {
            any[w.end] = true;
        }
        if exact[w.start] && !w.partial {
            exact[w.end] = true;
        }
    }
    if exact[len] { Cover::Exact } else if any[len] { Cover::Partial } else { Cover::None }
}

pub struct Engine {
    dict: Dict,
    fuzzy_penalty: f64,
    cfg: FuzzyConfig,
    user: UserModel,
    gram: Option<Gram>,
    gram_weight: f64,
    gram_threshold: f64,
    english: Option<English>,
}

impl Engine {
    pub fn new(dict: Dict, cfg: FuzzyConfig) -> Engine {
        Engine { dict, fuzzy_penalty: FUZZY_PENALTY, cfg, user: UserModel::default(), gram: None, gram_weight: GRAM_WEIGHT, gram_threshold: GRAM_THRESHOLD, english: None }
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

    /// 设多字词每次模糊替换的惩罚;单字词另加 `SINGLE_CHAR_FUZZY_EXTRA`(0.5)。
    pub fn set_fuzzy_penalty(&mut self, p: f64) {
        self.fuzzy_penalty = p;
    }

    /// 一次模糊替换的惩罚;单字词另加 `SINGLE_CHAR_FUZZY_EXTRA`。
    fn penalty(&self, single: bool) -> f64 {
        self.fuzzy_penalty + if single { SINGLE_CHAR_FUZZY_EXTRA } else { 0.0 }
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

    /// 删除词 `text` 的全部个人学习(含回车学会的英文串;词库不受影响)。
    pub fn forget(&mut self, text: &str) {
        self.user.forget(text);
        if let Some(en) = &mut self.english {
            en.forget(text);
        }
    }

    pub fn set_english(&mut self, english: English) {
        self.english = Some(english);
    }

    pub fn english(&self) -> Option<&English> {
        self.english.as_ref()
    }

    /// 回车原样上屏了 `input`:全小写字母、≥ 4 个且拼音非精确通时记一次;返回是否记了。
    pub fn learn_raw(&mut self, input: &str) -> bool {
        if input.len() < EN_EXACT_MIN || !input.bytes().all(|b| b.is_ascii_lowercase()) || cover(&self.words(input, ""), input.len()) == Cover::Exact {
            return false;
        }
        self.english.get_or_insert_with(English::default).learn_raw(input);
        true
    }

    /// 在上文 `prev` 之后上屏了 `text`,记下个人搭配。
    pub fn learn_next(&mut self, prev: &str, text: &str) {
        self.user.learn_next(prev, text);
    }

    /// 选中的候选用到了这些模糊规则,记下以便降低这几对的惩罚。
    pub fn learn_fuzzy(&mut self, pairs: u16) {
        self.user.learn_fuzzy(pairs);
    }

    /// 学一组整句拆出的模糊词(各记一次选词与搭配)。
    pub fn learn_lessons(&mut self, lessons: &[Lesson]) {
        for (t, w, p) in lessons {
            self.user.learn(t, w);
            self.user.learn_next(p, w);
        }
    }

    /// 撤销 `learn_lessons`。
    pub fn unlearn_lessons(&mut self, lessons: &[Lesson]) {
        for (t, w, p) in lessons {
            self.unlearn(t, w, 0, p);
        }
    }

    /// 同一键取词频前 `PER_KEY` 个,再补上更靠后但用户学过的词(否则截断会让学习加分落空)。
    fn top_entries(&self, key: &str) -> Vec<crate::dict::Entry> {
        self.dict.get(key).into_iter().enumerate().filter(|(i, e)| *i < PER_KEY || self.user.knows(&e.word)).map(|(_, e)| e).collect()
    }

    /// 再走一个音节:累加模糊次数、规则位,以及这些规则的个人偏好折扣。
    fn step(&self, acc: Acc, count: u8, pairs: u16) -> Acc {
        Acc { fuzzy: acc.fuzzy + count, discount: acc.discount + self.user.fuzzy_discount(pairs), pairs: acc.pairs | pairs, ..acc }
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
            let mut a = self.step(acc, c, m);
            a.splits += u8::from(!key.is_empty() && steals(input, acc.prev, pos, true));
            for e in self.top_entries(&next) {
                let bonus = self.user.bonus(&input[start..], &e.word);
                let score = f64::from(e.weight.max(1)).ln() - self.penalty(key.is_empty()) * f64::from(a.fuzzy) + a.discount - SPLIT_PENALTY * f64::from(a.splits) - PARTIAL_PENALTY + bonus - if key.is_empty() { SINGLE_CHAR_DISCOUNT } else { 0.0 };
                out.push(Word { start, end: input.len(), text: e.word, score, fuzzy: a.fuzzy, pairs: a.pairs, cuts: cuts.to_vec(), partial: true, learned: false, last: pos });
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
                let mut a = self.step(acc, c, m);
                a.splits += u8::from(depth > 0 && steals(input, acc.prev, pos, false));
                for e in self.top_entries(&next) {
                    let bonus = self.user.bonus(&input[start..end], &e.word);
                    let learned = a.fuzzy > 0 && self.user.chose(&input[start..end], &e.word);
                    let score = f64::from(e.weight.max(1)).ln() - self.penalty(depth == 0) * f64::from(a.fuzzy) + a.discount - SPLIT_PENALTY * f64::from(a.splits) + bonus - if depth == 0 { SINGLE_CHAR_DISCOUNT } else { 0.0 };
                    out.push(Word { start, end, text: e.word, score, fuzzy: a.fuzzy, pairs: a.pairs, cuts: cuts.clone(), partial: false, learned, last: pos });
                }
                if depth + 1 < MAX_SYLLABLES {
                    cuts.push(end);
                    self.grow(input, start, end, &next, Acc { prev: pos, ..a }, depth + 1, cuts, out);
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
        let words = self.words(input, context);
        let mut out = self.assemble(input, &words);
        if let Some(en) = &self.english {
            place_english(en, input, cover(&words, input.len()), &mut out);
        }
        for c in out.iter_mut().filter(|c| c.text.is_ascii()) {
            c.source = Source::English;
        }
        out.truncate(MAX_CANDIDATES);
        out
    }

    /// 收集从各可达位置出发的词与匹配的用户短语;有上文时首词加搭配分。
    fn words(&self, input: &str, context: &str) -> Vec<Word> {
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
            self.grow(input, a, a, "", Acc { prev: a, ..Acc::default() }, 0, &mut Vec::new(), &mut words);
        }
        for (typed, text, bonus) in self.user.matching(input) {
            if !words.iter().any(|w| w.start == 0 && w.end == typed.len() && w.text == text) {
                words.push(Word { start: 0, end: typed.len(), text: text.to_string(), score: USER_PHRASE_BASE + bonus, fuzzy: 0, pairs: 0, cuts: Vec::new(), partial: false, learned: false, last: typed.len() });
            }
        }
        if !context.is_empty() {
            // 用户在这个上文后选过其中某个首词时,只按个人搭配排,不计语法模型:
            // 模型里查不到的搭配按 0 分,常用字(如"在")白拿近 10 分,个人搭配追不上
            let mine = words.iter().any(|w| w.start == 0 && self.user.collocation(context, &w.text) > 0.0);
            for w in words.iter_mut().filter(|w| w.start == 0) {
                w.score += if mine { self.user.collocation(context, &w.text) } else { self.gram_bonus(context, &w.text) };
            }
        }
        words
    }

    fn assemble(&self, input: &str, words: &[Word]) -> Vec<Candidate> {
        let Some(far) = words.iter().map(|w| w.end).max() else { return Vec::new() };
        let bigram = |ctx: &str, w: &str| self.gram_bonus(ctx, w);
        let scored = sentences(input, words, far, &bigram);
        // 学过的模糊词只影响整句之间的分档(.2);置前只看第一条真正无模糊的精确整句的得分,
        // 免得学过模糊词的整句占首位后,精确整句不再参与判断、低频整段模糊词反被顶到第一
        let top_sentence = scored.iter().find(|s| s.2 && s.0.fuzzy == 0).map(|s| s.1);
        let top_exact = top_sentence.is_some();
        let sents: Vec<Candidate> = scored.into_iter().map(|s| s.0).collect();
        let mut firsts: Vec<&Word> = words.iter().filter(|w| w.start == 0).collect();
        firsts.sort_by(|a, b| b.end.cmp(&a.end).then(b.score.total_cmp(&a.score)));
        let as_cand = |w: &&Word| Candidate {
            text: w.text.clone(), consumed: w.end, fuzzy: w.fuzzy, pairs: w.pairs, cuts: w.cuts.clone(), parts: Vec::new(),
            source: if w.end == far { Source::Whole } else { Source::Prefix },
        };
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

/// 按拼音三态把英文候选插进 `out`;`input` 须全为小写字母。
fn place_english(en: &English, input: &str, cover: Cover, out: &mut Vec<Candidate>) {
    if input.is_empty() || !input.bytes().all(|b| b.is_ascii_lowercase()) {
        return;
    }
    let (exact, more) = en.lookup(input);
    match cover {
        Cover::None => {
            // 选过的英文(用户短语)留在最前;补全档位 0 在前,其余档位不分档按词长排。
            let mut more = more;
            more.sort_by_key(|h| (h.tier != TIER_CUSTOM, h.word.len()));
            let at = out.iter().take_while(|c| c.text.is_ascii()).count();
            insert_english(out, at, input, exact.iter().chain(&more), usize::MAX);
        }
        Cover::Partial => {
            // 档位 0 的完全匹配排前部;其他档位的完全匹配与档位 0 补全一起放第 3 位起。
            let k = insert_english(out, 0, input, exact.iter().filter(|h| h.tier == TIER_CUSTOM), EN_PARTIAL_EXACT);
            let rest = exact.iter().filter(|h| h.tier != TIER_CUSTOM).chain(more.iter().filter(|h| h.tier == TIER_CUSTOM));
            // 前部已插 k 条时,英文补全前至少留 1 条中文。
            insert_english(out, EN_SLOT.max(k + 1), input, rest, EN_PARTIAL_COMPLETE);
        }
        Cover::Exact => {
            let hits = exact.iter().filter(|h| input.len() >= EN_EXACT_MIN || h.tier == TIER_CUSTOM);
            insert_english(out, EN_SLOT, input, hits, 1);
        }
    }
}

/// 从下标 `at`(超出则接末尾)起依次插入至多 `max` 条英文候选,已有同文字的跳过且不计数;返回实际插入条数。
fn insert_english<'a>(out: &mut Vec<Candidate>, at: usize, input: &str, hits: impl Iterator<Item = &'a Hit<'a>>, max: usize) -> usize {
    let mut i = at.min(out.len());
    let mut n = 0;
    for h in hits {
        if n >= max {
            break;
        }
        if out.iter().any(|c| c.text == h.word) {
            continue;
        }
        out.insert(i, Candidate { text: h.word.to_string(), consumed: input.len(), fuzzy: 0, pairs: 0, cuts: Vec::new(), parts: Vec::new(), source: Source::English });
        i += 1;
        n += 1;
    }
    n
}

/// 在 [0, far] 上做束搜索 Viterbi,返回至多 `SENTENCES` 条拆成 ≥2 个词的整句(得分从高到低)。
/// `bigram(上文, 词)` 给相邻词的搭配加分。
/// 相邻两词之间若是抢音切分(见 `pinyin::steals`),另扣 `SPLIT_PENALTY`。
fn sentences(input: &str, words: &[Word], far: usize, bigram: &dyn Fn(&str, &str) -> f64) -> Vec<(Candidate, f64, bool)> {
    // best[pos] = 到达 pos 的前 BEAM 条路径:(得分, 词下标, 前驱位置上的名次, 路径末尾几个字)
    let mut best: Vec<Vec<(f64, usize, usize, String, bool)>> = vec![Vec::new(); far + 1];
    let mut order: Vec<usize> = (0..words.len()).filter(|&i| words[i].end <= far).collect();
    order.sort_by_key(|&i| words[i].start);
    for &i in &order {
        let w = &words[i];
        let mut next = Vec::new();
        if w.start == 0 {
            next.push((w.score - WORD_COST, i, 0, tail(&w.text), w.loose()));
        } else {
            for (r, p) in best[w.start].iter().enumerate() {
                let split = steals(input, words[p.1].last, w.start, w.partial && w.cuts.is_empty());
                let total = p.0 + w.score - WORD_COST + bigram(&p.3, &w.text) - if split { SPLIT_PENALTY } else { 0.0 };
                next.push((total, i, r, tail(&format!("{}{}", p.3, w.text)), p.4 || w.loose()));
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
        let (mut path, mut fuzzy, mut pairs, mut cuts) = (Vec::new(), 0u8, 0u16, Vec::new());
        while pos > 0 {
            let (_, wi, prev_rank, _, _) = &best[pos][rank];
            let w = &words[*wi];
            path.push(w);
            fuzzy = fuzzy.saturating_add(w.fuzzy);
            pairs |= w.pairs;
            cuts.extend(w.cuts.iter().copied());
            if w.start > 0 {
                cuts.push(w.start);
            }
            (pos, rank) = (w.start, *prev_rank);
        }
        path.reverse();
        let text: String = path.iter().map(|w| w.text.as_str()).collect();
        if path.len() >= 2 && out.len() < BEAM && !out.iter().any(|c| c.0.text == text) {
            out.push(({
                cuts.sort_unstable();
                Candidate { text, consumed: far, fuzzy, pairs, cuts, parts: fuzzy_parts(&path), source: Source::Sentence }
            }, best[far][r].0, !best[far][r].4));
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

/// 整句路径(按出现顺序)中模糊且完整的词:(起, 止, 句中字位置, 词)。
fn fuzzy_parts(path: &[&Word]) -> Vec<(usize, usize, usize, String)> {
    let mut at = 0;
    let mut out = Vec::new();
    for w in path {
        if w.fuzzy > 0 && !w.partial {
            out.push((w.start, w.end, at, w.text.clone()));
        }
        at += w.text.chars().count();
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
        // 单字惩罚 3.0 约 20 倍:高频模糊词压过极低频的精确词,但不会压过同量级的精确词
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
        // 之 的词频是 字 的 20 倍(≈ e^3),单字惩罚 3.0 时输给精确的 字;接受两次 z=zh 后折扣约 1.1,反超
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

    fn cand(e: &Engine, input: &str, text: &str) -> Candidate {
        e.candidates(input).into_iter().find(|c| c.text == text).unwrap()
    }

    #[test]
    fn 整句候选只带出模糊完整词() {
        let e = engine(&[("qin", "亲", 50_000), ("qing", "请", 20_000), ("si kao", "思考", 10_000)]);
        assert_eq!(cand(&e, "qinsikao", "请思考").parts, [(0, 3, 0, "请".to_string())]);
        assert!(cand(&e, "qinsikao", "亲思考").parts.is_empty());
        assert!(cand(&e, "qinsikao", "请").parts.is_empty());
    }

    #[test]
    fn 句末残缺词不进_parts() {
        let e = engine(&[("qing", "请", 20_000), ("zhi", "之", 10_000)]);
        assert_eq!(cand(&e, "qinz", "请之").lessons("qinz", ""), [("qin".to_string(), "请".to_string(), String::new())]);
    }

    #[test]
    fn lessons_上文含前文与句中前面的词() {
        let e = engine(&[("wo", "我", 5000), ("xi wang", "希望", 5000)]);
        let c = cand(&e, "woxiwan", "我希望");
        assert_eq!(c.parts, [(2, 7, 1, "希望".to_string())]);
        assert_eq!(c.lessons("woxiwan", "说"), [("xiwan".to_string(), "希望".to_string(), "说我".to_string())]);
    }

    #[test]
    fn 全精确整句没有_lessons() {
        let e = engine(&[("wo", "我", 5000), ("kan dao", "看到", 5000), ("ta", "他", 9000), ("ta", "她", 5000), ("le", "了", 5000)]);
        assert!(cand(&e, "wokandaotale", "我看到她了").lessons("wokandaotale", "").is_empty());
    }

    #[test]
    fn learn_lessons_与_unlearn_lessons_对称() {
        let mut e = engine(&[("qin", "亲", 50_000)]);
        let ls = vec![("qin".to_string(), "请".to_string(), "说".to_string())];
        e.learn_lessons(&ls);
        let x = e.user().export();
        assert!(x.contains("qin\t请\t1") && x.contains("@ng\t说\t请\t1"), "{x}");
        e.unlearn_lessons(&ls);
        assert_eq!(e.user().export(), "");
    }

    #[test]
    fn 整句学过的模糊词推广到新句子() {
        let mut e = engine(&[("qin", "亲", 50_000), ("qing", "请", 20_000), ("si kao", "思考", 10_000), ("jian cha", "检查", 10_000)]);
        assert_ne!(texts(&e, "qinjiancha")[0], "请检查");
        let ls = cand(&e, "qinsikao", "请思考").lessons("qinsikao", "");
        e.learn_lessons(&ls);
        assert_eq!(texts(&e, "qinjiancha")[0], "请检查");
    }

    #[test]
    fn 学过的模糊单字不使整句压过整段模糊词() {
        let mut e = engine(&[
            ("qin", "亲", 50_000), ("qing", "请", 20_000), ("qing kuang xia", "情况下", 10_000),
            ("kuan", "款", 10_000_000), ("xia", "下", 10_000_000),
        ]);
        assert_eq!(texts(&e, "qinkuanxia")[0], "情况下");
        e.learn("qin", "请");
        assert_eq!(texts(&e, "qinkuanxia")[0], "情况下");
    }

    #[test]
    fn 学过模糊词不使低频整段模糊词反超精确整句() {
        let mut e = engine(&[
            ("qin", "亲", 50_000), ("qing", "请", 20_000), ("qing kuang xia", "情况下", 20),
            ("kuan", "款", 10_000_000), ("xia", "下", 10_000_000),
        ]);
        assert_ne!(texts(&e, "qinkuanxia")[0], "情况下");
        e.learn("qin", "请");
        assert_ne!(texts(&e, "qinkuanxia")[0], "情况下");
    }

    #[test]
    fn 多字词模糊惩罚低于单字() {
        // 模糊词词频为精确词 16 倍(ln16 ≈ 2.77):多字词惩罚 2.5,模糊词胜出;单字惩罚 3.0,精确字仍居首
        let e = engine(&[("zi lei", "子类", 1000), ("zhi lei", "之类", 16_000)]);
        assert_eq!(texts(&e, "zilei")[0], "之类");
        let e = engine(&[("si", "四", 1000), ("shi", "是", 16_000)]);
        assert_eq!(texts(&e, "si")[0], "四");
    }

    #[test]
    fn 抢音切分降权_完整音节优先() {
        // 词内(li|ang → 立案)、词间(da + 句末残缺 n → 大脑的"脑")都扣抢音惩罚
        let e = engine(&[("liang", "两", 1000), ("li an", "立案", 20_000)]);
        assert_eq!(texts(&e, "liang")[0], "两");
        let e = engine(&[("dan", "但", 1000), ("da nao", "大脑", 20_000), ("da", "大", 1_000_000), ("nao", "脑", 1_000_000)]);
        assert_eq!(texts(&e, "dan")[0], "但");
        // 不抢音的元音开头切分(fang|an)不受影响
        let e = engine(&[("fang an", "方案", 1000), ("fan gan", "反感", 900)]);
        assert_eq!(texts(&e, "fangan")[0], "方案");
    }

    #[test]
    fn 候选标注来源() {
        let e = engine(&[("ni hao", "你好", 5000), ("ni", "你", 5000), ("hao", "好", 5000), ("ma", "吗", 5000)]);
        assert_eq!(cand(&e, "nihao", "你好").source, Source::Whole);
        assert_eq!(cand(&e, "nihaoma", "你好吗").source, Source::Sentence);
        assert_eq!(cand(&e, "nihaoma", "你好").source, Source::Prefix);
    }

    fn with_english(entries: &[(&str, &str, u32)], en: &[(&str, &str, u8)]) -> Engine {
        let mut e = engine(entries);
        e.set_english(English::new(en.iter().map(|&(k, w, t)| (k.to_string(), w.to_string(), t)).collect()));
        e
    }

    #[test]
    fn 拼音不通时英文排最前_全部列出() {
        let en = [("playwright", "Playwright", 0), ("playwright", "playwright", 2), ("playwrights", "playwrights", 2), ("playwriting", "playwriting", 2)];
        let e = with_english(&[("pa", "怕", 100)], &en);
        let c = e.candidates("playw");
        assert_eq!(c.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), vec!["Playwright", "playwright", "playwrights", "playwriting"]);
        assert!(c.iter().all(|c| c.source == Source::English && c.consumed == 5 && c.cuts.is_empty()));
    }

    #[test]
    fn 拼音不通时完全匹配在前_档位0补全次之_其余补全不分档按长度() {
        let en = [("playstation", "PlayStation", 1), ("player", "player", 2), ("plays", "plays", 2), ("play", "play", 2), ("playwright", "Playwright", 0)];
        let e = with_english(&[("pa", "怕", 100)], &en);
        assert_eq!(texts(&e, "play"), ["play", "Playwright", "plays", "player", "PlayStation"]);
    }

    #[test]
    fn 拼音不通时选过的英文排最前_其余照常全部列出() {
        let en = [("player", "player", 2), ("plays", "plays", 2), ("play", "play", 2), ("playwright", "Playwright", 0)];
        let mut e = with_english(&[("pa", "怕", 100)], &en);
        e.learn("play", "player");
        assert_eq!(texts(&e, "play"), ["player", "play", "Playwright", "plays"]);
    }

    #[test]
    fn 残缺通时完全匹配的英文排第一() {
        let e = with_english(&[("su", "苏", 100), ("ba", "把", 100), ("gen", "跟", 100), ("ta", "他", 100)], &[("subagent", "subagent", 0)]);
        let t = texts(&e, "subagent");
        assert_eq!(t[0], "subagent");
        assert!(t.len() > 1, "中文候选仍在");
    }

    #[test]
    fn 残缺通时只补全档位0_插在第3位() {
        let e = with_english(&[("ku", "苦", 100), ("bei", "被", 100)], &[("kubernetes", "Kubernetes", 0), ("kubectl", "kubectl", 2)]);
        let t = texts(&e, "kube");
        assert_eq!(t[2], "Kubernetes");
        assert!(!t[..2].iter().any(|s| s.is_ascii()));
        assert!(!t.contains(&"kubectl".to_string()));
    }

    #[test]
    fn 精确通时完全匹配的英文插第3位_不补全() {
        let dict = [("chang e", "嫦娥", 1000), ("chang ge", "唱歌", 900), ("chang", "长", 800), ("e", "鹅", 500)];
        let e = with_english(&dict, &[("change", "change", 2), ("changes", "changes", 2)]);
        let t = texts(&e, "change");
        assert_eq!(t[0], "嫦娥");
        assert_eq!(t[2], "change");
        assert!(!t.contains(&"changes".to_string()));
    }

    #[test]
    fn 精确通时不足4字母只出档位0() {
        let e = with_english(&[("a", "啊", 100), ("pi", "皮", 100), ("me", "么", 100)], &[("api", "API", 0), ("me", "me", 2)]);
        assert_eq!(texts(&e, "api").iter().position(|t| t == "API"), Some(2));
        assert!(!texts(&e, "me").contains(&"me".to_string()));
    }

    #[test]
    fn 学过的英文由用户短语排首位且不重复() {
        let mut e = with_english(&[("ku", "苦", 100), ("bei", "被", 100)], &[("kubernetes", "Kubernetes", 0)]);
        e.learn("kube", "Kubernetes");
        let c = e.candidates("kube");
        assert_eq!(c[0].text, "Kubernetes");
        assert_eq!(c[0].source, Source::English);
        assert_eq!(c.iter().filter(|c| c.text == "Kubernetes").count(), 1);
    }

    #[test]
    fn 未注入或空英文词表时候选不变() {
        let dict = [("ni", "你", 100), ("hao", "好", 90), ("ni hao", "你好", 80)];
        let plain = engine(&dict);
        let mut with = engine(&dict);
        with.set_english(English::default());
        for input in ["nihao", "ni", "nih"] {
            assert_eq!(plain.candidates(input), with.candidates(input));
        }
    }

    #[test]
    fn 回车学习_不通或残缺通且满4字母才记_满2次出现_forget后消失() {
        let mut e = with_english(&[("zhe", "这", 100), ("chang e", "嫦娥", 100)], &[]);
        assert!(!e.learn_raw("change"), "精确通不学");
        assert!(!e.learn_raw("zhe"), "不足 4 字母不学");
        assert!(e.learn_raw("zhegnshi"));
        assert!(!texts(&e, "zhegnshi").contains(&"zhegnshi".to_string()));
        assert!(e.learn_raw("zhegnshi"));
        assert_eq!(texts(&e, "zhegnshi")[0], "zhegnshi");
        e.forget("zhegnshi");
        assert!(!texts(&e, "zhegnshi").contains(&"zhegnshi".to_string()));
    }

    #[test]
    fn 残缺通时非档位0的完全匹配不抢首位_落第3位() {
        let e = with_english(&[("ku", "苦", 100), ("bei", "被", 100)], &[("kube", "kube", 2)]);
        let t = texts(&e, "kube");
        assert_eq!(t[2], "kube");
        assert!(t[..2].iter().all(|s| !s.is_ascii()));
    }

    #[test]
    fn 残缺通时档位0完全匹配占2条_补全接在第一条中文之后() {
        let e = with_english(
            &[("ku", "苦", 100), ("bei", "被", 100)],
            &[("kube", "Kube", 0), ("kube", "KUBE", 0), ("kubernetes", "Kubernetes", 0)],
        );
        let t = texts(&e, "kube");
        assert_eq!(&t[..2], ["Kube", "KUBE"]);
        assert!(!t[2].is_ascii(), "下标 2 为中文");
        assert_eq!(t[3], "Kubernetes");
    }

    #[test]
    fn 不通时英文与已有候选同文字_仍出满3条() {
        // 词典里有文字为 Playwright 的前缀词;英文的同文字补全被跳过,不占 3 条名额。
        let en = [("pawxyz", "Playwright", 0), ("pawxy", "pawxy", 2), ("pawxyz", "pawxyz", 2), ("pawxys", "pawxys", 2)];
        let e = with_english(&[("pa", "Playwright", 100)], &en);
        let t = texts(&e, "pawxy");
        assert_eq!(t.iter().filter(|s| ["pawxy", "pawxyz", "pawxys"].contains(&s.as_str())).count(), 3, "{t:?}");
    }

    #[test]
    fn 学过整段短语后变精确通_补全不再出现() {
        // 有意行为:学过即按精确通处理,档位 0 补全不再出。
        let mut e = with_english(&[("ku", "苦", 100), ("bei", "被", 100)], &[("kubernetes", "Kubernetes", 0)]);
        assert!(texts(&e, "kube").contains(&"Kubernetes".to_string()));
        e.learn("kube", "苦被");
        assert!(!texts(&e, "kube").contains(&"Kubernetes".to_string()));
    }
}
