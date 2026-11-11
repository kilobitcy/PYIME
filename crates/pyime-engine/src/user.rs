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
/// 个人搭配条目上限;满了先淘汰最旧的一次性条目,仍满则不收新条目(已有的仍累加)。
const MAX_COLLOCATIONS: usize = 100_000;
/// 个人搭配满额时一次淘汰的最旧一次性条目数。
const EVICT_BATCH: usize = 1000;
/// 单项计数上限,防止历史无限放大。
const MAX_COUNT: u32 = 1000;
/// 老化:计数为 1 且这么多天没再用的词对与搭配被淘汰。
pub const AGE_DAYS: u32 = 60;
/// "今天"早于此日(2026-11-01)视为时钟不可信,不淘汰。
pub const DAY_FLOOR: u32 = 20758;

/// 一条学习记录:次数与最后使用日(自 1970-01-01 起的天数)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    n: u32,
    day: u32,
}

#[derive(Default)]
pub struct UserModel {
    pairs: HashMap<(String, String), Entry>,
    words: HashMap<String, u32>,
    /// 各模糊规则被接受的次数(下标同 `fuzzy::variants` 的规则位:声母 0–4,韵母 5–9)。
    fuzzy: [u32; 10],
    /// (上文末尾 1–3 个字, 紧随其后的词) → 次数与最后使用日。
    next: HashMap<(String, String), Entry>,
    /// 今天(自 1970-01-01 起的天数),学习时记为最后使用日;由调用方设置。
    today: u32,
}

impl UserModel {
    /// 设置"今天",此后的学习以它为最后使用日。
    pub fn set_today(&mut self, today: u32) {
        self.today = today;
    }

    /// 记一次选择:用户键入 `typed`,上屏了 `word`。
    pub fn learn(&mut self, typed: &str, word: &str) {
        let today = self.today;
        let p = self.pairs.entry((typed.to_string(), word.to_string())).or_insert(Entry { n: 0, day: today });
        p.n = (p.n + 1).min(MAX_COUNT);
        p.day = today;
        let w = self.words.entry(word.to_string()).or_insert(0);
        *w = (*w + 1).min(MAX_COUNT);
    }

    /// 撤销一次 `learn`(各计数减一,归零即删除)。
    pub fn unlearn(&mut self, typed: &str, word: &str) {
        dec_entry(&mut self.pairs, &(typed.to_string(), word.to_string()));
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
            dec_entry(&mut self.next, &(chars[chars.len() - k..].iter().collect::<String>(), word.to_string()));
        }
    }

    /// 删除 `word` 的全部个人学习:各拼音下的精确层、词层,以及以它为后继的搭配。词库不受影响。
    pub fn forget(&mut self, word: &str) {
        self.pairs.retain(|(_, w), _| w != word);
        self.words.remove(word);
        self.next.retain(|(_, w), _| w != word);
    }

    /// 淘汰计数为 1 且 `AGE_DAYS` 天未再用的词对与搭配,返回 (词对, 搭配) 删除条数;删词对时词层扣 1。
    /// 今天早于 `DAY_FLOOR` 不淘汰;时钟倒退(记录日晚于今天)按距今 0 天算。
    pub fn prune(&mut self) -> (usize, usize) {
        let today = self.today;
        if today < DAY_FLOOR {
            return (0, 0);
        }
        let stale = |e: &Entry| e.n == 1 && today.saturating_sub(e.day) >= AGE_DAYS;
        let gone: Vec<(String, String)> = self.pairs.iter().filter(|(_, e)| stale(e)).map(|(k, _)| k.clone()).collect();
        for k in &gone {
            self.pairs.remove(k);
            dec(&mut self.words, &k.1);
        }
        let before = self.next.len();
        self.next.retain(|_, e| !stale(e));
        (gone.len(), before - self.next.len())
    }

    /// 记一次搭配:`prev` 是光标前已有的文字,其后上屏了 `word`。上文末尾 1–3 个字各记一条。
    pub fn learn_next(&mut self, prev: &str, word: &str) {
        let chars: Vec<char> = prev.chars().collect();
        let today = self.today;
        for k in 1..=chars.len().min(MAX_CONTEXT) {
            let key = (chars[chars.len() - k..].iter().collect::<String>(), word.to_string());
            if self.next.len() >= MAX_COLLOCATIONS && !self.next.contains_key(&key) {
                self.evict_next();
                if self.next.len() >= MAX_COLLOCATIONS {
                    continue;
                }
            }
            let e = self.next.entry(key).or_insert(Entry { n: 0, day: today });
            e.n = (e.n + 1).min(MAX_COUNT);
            e.day = today;
        }
    }

    /// 个人搭配满额时,淘汰最后使用日最早的至多 `EVICT_BATCH` 条计数为 1 的条目。
    fn evict_next(&mut self) {
        let mut ones: Vec<(u32, (String, String))> = self.next.iter().filter(|(_, e)| e.n == 1).map(|(k, e)| (e.day, k.clone())).collect();
        if ones.len() > EVICT_BATCH {
            ones.select_nth_unstable_by_key(EVICT_BATCH - 1, |o| o.0);
            ones.truncate(EVICT_BATCH);
        }
        for (_, k) in ones {
            self.next.remove(&k);
        }
    }

    /// 上文 `prev` 之后接 `word` 的个人搭配加分(≥ 0):取上文各长度中次数最多者,对数且封顶。
    pub fn collocation(&self, prev: &str, word: &str) -> f64 {
        let chars: Vec<char> = prev.chars().collect();
        let best = (1..=chars.len().min(MAX_CONTEXT))
            .filter_map(|k| self.next.get(&(chars[chars.len() - k..].iter().collect::<String>(), word.to_string())).map(|e| e.n))
            .max();
        best.map_or(0.0, |n| (COLLOCATION_SLOPE * f64::from(n + 1).ln()).min(COLLOCATION_CAP))
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
        if let Some(e) = self.pairs.get(&(typed.to_string(), word.to_string())) {
            b += PAIR_BASE + PAIR_SLOPE * f64::from(e.n).ln();
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

    /// 序列化为 `拼音⇥词⇥次数⇥日` 行,便于存盘。
    pub fn export(&self) -> String {
        let mut lines: Vec<String> = self.pairs.iter().map(|((t, w), e)| format!("{t}\t{w}\t{}\t{}", e.n, e.day)).collect();
        lines.sort();
        lines.extend(self.next.iter().map(|((p, w), e)| format!("@ng\t{p}\t{w}\t{}\t{}", e.n, e.day)));
        lines.sort();
        lines.extend(self.fuzzy.iter().enumerate().filter(|(_, &n)| n > 0).map(|(i, n)| format!("@fuzzy\t{i}\t{n}")));
        lines.join("\n")
    }

    /// 读回 `export` 的结果;坏行跳过。词层计数由精确层累加得出。
    /// 缺日期列(旧格式)或日期无法解析时,最后使用日取 `today`。
    /// 上文含英文字母的搭配(旧版选英文候选后把 est 之类记作上文)丢弃。
    pub fn import(text: &str, today: u32) -> UserModel {
        let mut m = UserModel { today, ..UserModel::default() };
        let day = |d: Option<&str>| d.and_then(|d| d.parse::<u32>().ok()).unwrap_or(today);
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("@ng\t") {
                let c: Vec<&str> = rest.split('\t').collect();
                if let [p, w, n, more @ ..] = c.as_slice() {
                    if more.len() > 1 || p.bytes().any(|b| b.is_ascii_alphabetic()) {
                        continue;
                    }
                    if let Ok(n) = n.parse::<u32>() {
                        m.next.insert((p.to_string(), w.to_string()), Entry { n: n.min(MAX_COUNT), day: day(more.first().copied()) });
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
            m.pairs.insert((t.to_string(), w.to_string()), Entry { n, day: day(c.next()) });
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

/// 学习记录次数减一,归零删除;不存在则不动。日期不回退。
fn dec_entry<K: std::hash::Hash + Eq>(map: &mut HashMap<K, Entry>, key: &K) {
    if let Some(e) = map.get_mut(key) {
        e.n -= 1;
        if e.n == 0 {
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
        let back = UserModel::import(&m.export(), 0);
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
        let back = UserModel::import(&m.export(), 0);
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
        let back = UserModel::import(&m.export(), 0);
        assert_eq!(back.bonus("ni", "你"), m.bonus("ni", "你"));
        assert_eq!(UserModel::import("坏行\n", 0).export(), "");
    }

    #[test]
    fn 读回时丢掉上文含英文字母的个人搭配() {
        // 用户真实数据:选过英文候选 test/code 后记下了 est→编号、e全局→配只中
        let m = UserModel::import("@ng\test\t编号\t1\n@ng\te全局\t配只中\t1\n@ng\t全局\t配置\t2\n", 0);
        assert_eq!(m.export(), "@ng\t全局\t配置\t2\t0");
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

    #[test]
    fn 学习记下当天_导出带日期_导入保留日期() {
        let mut m = UserModel::default();
        m.set_today(20800);
        m.learn("ni", "你");
        m.learn_next("我", "你");
        assert_eq!(m.export(), "@ng\t我\t你\t1\t20800\nni\t你\t1\t20800");
        m.set_today(20810);
        m.learn("ni", "你");
        assert!(m.export().contains("ni\t你\t2\t20810"));
        let back = UserModel::import(&m.export(), 20900);
        assert_eq!(back.export(), m.export());
    }

    #[test]
    fn 旧格式缺日期按读入当天() {
        let m = UserModel::import("ni\t你\t3\n@ng\t我\t你\t2\n@ng\t我\t他\t1\t坏\n", 20800);
        assert_eq!(m.export(), "@ng\t我\t他\t1\t20800\n@ng\t我\t你\t2\t20800\nni\t你\t3\t20800");
    }

    #[test]
    fn 一次性且满60天才淘汰_词层随之扣减() {
        let t = DAY_FLOOR + 100;
        let text = format!(
            "ni\t你\t1\t{}\nni\t泥\t1\t{}\nhao\t好\t2\t{}\nnihao\t你\t1\t{}\n@ng\t我\t你\t1\t{}\n@ng\t我\t泥\t1\t{}\n@ng\t我\t好\t2\t{}",
            t - 60, t - 59, t - 200, t - 59, t - 60, t - 59, t - 200
        );
        let mut m = UserModel::import(&text, t);
        assert_eq!(m.prune(), (1, 1));
        assert!(!m.chose("ni", "你") && m.chose("ni", "泥") && m.chose("hao", "好") && m.chose("nihao", "你"));
        assert_eq!(m.bonus("zzz", "你"), WORD_BASE); // 词层 2 次扣成 1 次:WORD_BASE + ln1
        assert_eq!(m.collocation("我", "你"), 0.0);
        assert!(m.collocation("我", "泥") > 0.0 && m.collocation("我", "好") > 0.0);
    }

    #[test]
    fn 读入当天的旧行不淘汰_时钟不可信或倒退不淘汰() {
        let t = DAY_FLOOR + 100;
        let mut m = UserModel::import("ni\t你\t1\n@ng\t我\t你\t1\n", t);
        assert_eq!(m.prune(), (0, 0));
        let mut m = UserModel::import(&format!("ni\t你\t1\t{}", t + 500), t); // 记录日晚于今天
        assert_eq!(m.prune(), (0, 0));
        let mut m = UserModel::import("ni\t你\t1\t0", DAY_FLOOR - 1);
        assert_eq!(m.prune(), (0, 0));
    }

    #[test]
    fn 搭配满额时淘汰最旧的一次性条目再收新的() {
        let mut m = UserModel::default();
        m.set_today(DAY_FLOOR);
        m.learn_next("甲", "旧0");
        m.learn_next("甲", "旧0"); // 计数 2,不淘汰
        for i in 1..1500 {
            m.learn_next("甲", &format!("旧{i}"));
        }
        m.set_today(DAY_FLOOR + 1);
        for i in 1500..MAX_COLLOCATIONS {
            m.learn_next("甲", &format!("新{i}"));
        }
        assert_eq!(m.next.len(), MAX_COLLOCATIONS);
        m.learn_next("甲", "来了");
        assert_eq!(m.next.len(), MAX_COLLOCATIONS - EVICT_BATCH + 1);
        assert!(m.collocation("甲", "来了") > 0.0 && m.collocation("甲", "旧0") > 0.0);
        assert_eq!((1..1500).filter(|i| m.collocation("甲", &format!("旧{i}")) > 0.0).count(), 499);
        assert!((1500..MAX_COLLOCATIONS).all(|i| m.collocation("甲", &format!("新{i}")) > 0.0));
    }

    #[test]
    fn 搭配满额且无一次性条目时仍拒收() {
        let mut m = UserModel::default();
        for i in 0..MAX_COLLOCATIONS {
            let w = format!("词{i}");
            m.learn_next("甲", &w);
            m.learn_next("甲", &w);
        }
        m.learn_next("甲", "来了");
        assert_eq!(m.next.len(), MAX_COLLOCATIONS);
        assert_eq!(m.collocation("甲", "来了"), 0.0);
    }
}
