//! 英文词表:键(全小写字母)→ 词,带档位;完全匹配与前缀补全。
//! 另记"回车原样上屏"的次数,满 `RAW_LEARN_MIN` 次的串以档位 0 当英文词。不涉及拼音。
use std::collections::{HashMap, HashSet};

/// 自带 IT 词表、用户 `en.txt`、回车学会的词。
pub const TIER_CUSTOM: u8 = 0;
/// rime-ice `en_ext.dict.yaml`。
pub const TIER_EXT: u8 = 1;
/// rime-ice `en.dict.yaml`。
pub const TIER_BASE: u8 = 2;
/// 输入至少这么多字母才做前缀补全。
pub const MIN_COMPLETE: usize = 4;
/// 回车原样上屏满这么多次,才把该串当英文词。
pub const RAW_LEARN_MIN: u32 = 2;
const MAX_COUNT: u32 = 1000;

struct Entry {
    key: String,
    word: String,
    tier: u8,
    /// 载入先后,作同档同长时的次序。
    seq: usize,
}

/// 一条英文候选。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit<'a> {
    pub word: &'a str,
    pub tier: u8,
}

#[derive(Default)]
pub struct English {
    /// 按 (键, 词) 排序。
    entries: Vec<Entry>,
    /// 回车原样上屏的串 → 次数。
    raw: HashMap<String, u32>,
}

fn valid_key(k: &str) -> bool {
    !k.is_empty() && k.bytes().all(|b| b.is_ascii_lowercase())
}

/// rime-ice 英文词库:跳过 `...` 之前的 YAML 头、`#` 注释与空行;行 `词⇥编码[⇥权重]`,
/// 键取编码转小写,含非字母的编码(如 `K8s`)跳过,权重忽略。返回 (键, 词)。
pub fn parse_rime(text: &str) -> Vec<(String, String)> {
    let body = text.split_once("\n...\n").map_or(text, |(_, rest)| rest);
    body.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut c = l.trim_end_matches('\r').split('\t');
            let (w, k) = (c.next()?, c.next()?.to_ascii_lowercase());
            (!w.is_empty() && valid_key(&k)).then(|| (k, w.to_string()))
        })
        .collect()
}

/// 自带/用户词表:一行 `词` 或 `词⇥键`,`#` 注释;无键时键 = 词去空格转小写。返回 (键, 词)。
pub fn parse_list(text: &str) -> Vec<(String, String)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut c = l.split('\t');
            let w = c.next()?.trim();
            let k = match c.next() {
                Some(k) => k.trim().to_ascii_lowercase(),
                None => w.replace(' ', "").to_ascii_lowercase(),
            };
            (!w.is_empty() && valid_key(&k)).then(|| (k, w.to_string()))
        })
        .collect()
}

/// 编译好的 `pyime.en`:行 `键⇥词⇥档位`,坏行跳过。
pub fn parse_compiled(text: &str) -> Vec<(String, String, u8)> {
    text.lines()
        .filter_map(|l| {
            let mut c = l.split('\t');
            let (k, w, t) = (c.next()?, c.next()?, c.next()?.trim().parse::<u8>().ok()?);
            (valid_key(k) && !w.is_empty()).then(|| (k.to_string(), w.to_string(), t))
        })
        .collect()
}

/// 按 档位 → 词长 → 先后 排序,同词只留第一条。
fn rank<'a>(mut v: Vec<(u8, usize, usize, &'a str)>) -> Vec<Hit<'a>> {
    v.sort();
    let mut seen = HashSet::new();
    v.into_iter().filter(|h| seen.insert(h.3)).map(|(tier, _, _, word)| Hit { word, tier }).collect()
}

impl English {
    /// 由 (键, 词, 档位) 建表;同键同词留最小档位,输入顺序作同档同长时的次序。
    pub fn new(items: Vec<(String, String, u8)>) -> English {
        let mut entries: Vec<Entry> = items.into_iter().enumerate().map(|(seq, (key, word, tier))| Entry { key, word, tier, seq }).collect();
        entries.sort_by(|a, b| (&a.key, &a.word, a.tier, a.seq).cmp(&(&b.key, &b.word, b.tier, b.seq)));
        entries.dedup_by(|cur, kept| cur.key == kept.key && cur.word == kept.word);
        English { entries, raw: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 写成 `pyime.en`:每行 `键⇥词⇥档位`,按载入先后。
    pub fn export(&self) -> String {
        let mut v: Vec<&Entry> = self.entries.iter().collect();
        v.sort_by_key(|e| e.seq);
        v.iter().map(|e| format!("{}\t{}\t{}\n", e.key, e.word, e.tier)).collect()
    }

    /// `input` 的英文候选:(完全匹配, 补全);补全只在 `input` ≥ `MIN_COMPLETE` 字母时给出。
    /// 回车学会的串(次数 ≥ `RAW_LEARN_MIN`)按档位 0 参与。
    pub fn lookup(&self, input: &str) -> (Vec<Hit<'_>>, Vec<Hit<'_>>) {
        let complete = input.len() >= MIN_COMPLETE;
        let (mut exact, mut more) = (Vec::new(), Vec::new());
        let start = self.entries.partition_point(|e| e.key.as_str() < input);
        for e in self.entries[start..].iter().take_while(|e| e.key.starts_with(input)) {
            let h = (e.tier, e.word.len(), e.seq, e.word.as_str());
            if e.key == input {
                exact.push(h);
            } else if complete {
                more.push(h);
            }
        }
        for (w, &n) in self.raw.iter().filter(|&(w, &n)| n >= RAW_LEARN_MIN && w.starts_with(input)) {
            let h = (TIER_CUSTOM, w.len(), usize::MAX - n as usize, w.as_str());
            if w == input {
                exact.push(h);
            } else if complete {
                more.push(h);
            }
        }
        (rank(exact), rank(more))
    }

    /// 记一次回车原样上屏的串。
    pub fn learn_raw(&mut self, s: &str) {
        let n = self.raw.entry(s.to_string()).or_insert(0);
        *n = (*n + 1).min(MAX_COUNT);
    }

    /// 删除回车学到的 `word`;返回是否删了。词表本身不受影响。
    pub fn forget(&mut self, word: &str) -> bool {
        self.raw.remove(word).is_some()
    }

    /// 回车计数存盘文本:`词⇥次数` 行,存 `en_learned.txt`。
    pub fn export_raw(&self) -> String {
        let mut lines: Vec<String> = self.raw.iter().map(|(w, n)| format!("{w}\t{n}")).collect();
        lines.sort();
        lines.join("\n")
    }

    /// 读回 `export_raw` 的结果;坏行跳过。
    /// 旧版把首字母大写的词(大写字母已放行给应用)记成去掉首字母的残片(Johnson → ohnson),
    /// 读回时把词表外的残片并到"某字母 + 残片"上:优先已学的整词,其次词表里的键。
    pub fn import_raw(&mut self, text: &str) {
        for (w, n) in text.lines().filter_map(|l| l.split_once('\t')) {
            if let (true, Ok(n)) = (valid_key(w), n.trim().parse::<u32>()) {
                self.raw.insert(w.to_string(), n.min(MAX_COUNT));
            }
        }
        let mut frags: Vec<String> = self.raw.keys().filter(|w| !self.has_key(w)).cloned().collect();
        frags.sort();
        for w in frags {
            let whole = ('a'..='z').map(|c| format!("{c}{w}"));
            let learned = whole.clone().filter_map(|k| self.raw.get(&k).map(|&n| (n, k))).max();
            let Some(target) = learned.map(|(_, k)| k).or_else(|| whole.into_iter().find(|k| self.has_key(k))) else { continue };
            if let Some(n) = self.raw.remove(&w) {
                let t = self.raw.entry(target).or_insert(0);
                *t = (*t + n).min(MAX_COUNT);
            }
        }
    }

    /// 词表里是否有键 `k`。
    fn has_key(&self, k: &str) -> bool {
        let i = self.entries.partition_point(|e| e.key.as_str() < k);
        self.entries.get(i).is_some_and(|e| e.key == k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(k: &str, w: &str) -> (String, String) {
        (k.to_string(), w.to_string())
    }

    fn words<'a>(h: &[Hit<'a>]) -> Vec<&'a str> {
        h.iter().map(|h| h.word).collect()
    }

    fn table() -> English {
        let mut items: Vec<(String, String, u8)> = parse_list("subagent\nKubernetes\nClaude Code\n").into_iter().map(|(k, w)| (k, w, TIER_CUSTOM)).collect();
        let rime = "---\nname: en\n...\nhell\thell\t2\nhe'll\thell\t1\nkube\tkube\napple\tapple\napplication\tapplication\nApple\tApple\n";
        items.extend(parse_rime(rime).into_iter().map(|(k, w)| (k, w, TIER_BASE)));
        English::new(items)
    }

    #[test]
    fn rime_跳过yaml头_注释_空行与含非字母的编码_第三列忽略() {
        let v = parse_rime("---\nname: en\n...\n# 注\nhell\thell\t2\nhe'll\thell\t1\nKubernetes\tK8s\nREADME.md\treadme.md\n\nGitHub\tGitHub\n");
        assert_eq!(v, vec![s("hell", "hell"), s("hell", "he'll"), s("github", "GitHub")]);
    }

    #[test]
    fn 词表行_无键时去空格转小写_显式键_跳过注释与非法键() {
        let v = parse_list("# 注释\nClaude Code\nC++\tcpp\nC#\n\nsubagent\r\n");
        assert_eq!(v, vec![s("claudecode", "Claude Code"), s("cpp", "C++"), s("subagent", "subagent")]);
    }

    #[test]
    fn 编译格式往返一致() {
        let e = table();
        let back = English::new(parse_compiled(&e.export()));
        assert_eq!(back.len(), e.len());
        assert_eq!(back.export(), e.export());
    }

    #[test]
    fn 完全匹配与补全按档位_词长_先后排序() {
        let e = table();
        let (ex, more) = e.lookup("kube");
        assert_eq!(words(&ex), vec!["kube"]);
        assert_eq!(words(&more), vec!["Kubernetes"]);
        let (ex, more) = e.lookup("appl");
        assert!(ex.is_empty());
        assert_eq!(words(&more), vec!["apple", "Apple", "application"]);
        assert_eq!(words(&e.lookup("hell").0), vec!["hell", "he'll"]);
        assert_eq!(words(&e.lookup("claudecode").0), vec!["Claude Code"]);
    }

    #[test]
    fn 不足4字母不补全() {
        let t = table();
        let (ex, more) = t.lookup("kub");
        assert!(ex.is_empty() && more.is_empty());
    }

    #[test]
    fn 同键同词留最小档位() {
        let e = English::new(vec![("kube".into(), "kube".into(), TIER_BASE), ("kube".into(), "kube".into(), TIER_CUSTOM)]);
        assert_eq!(e.len(), 1);
        assert_eq!(e.lookup("kube").0, vec![Hit { word: "kube", tier: TIER_CUSTOM }]);
    }

    #[test]
    fn 回车学满2次才出现_可存取_可删除() {
        let mut e = table();
        e.learn_raw("zhegnshi");
        assert!(e.lookup("zhegnshi").0.is_empty());
        e.learn_raw("zhegnshi");
        assert_eq!(e.lookup("zhegnshi").0, vec![Hit { word: "zhegnshi", tier: TIER_CUSTOM }]);
        assert_eq!(words(&e.lookup("zhegn").1), vec!["zhegnshi"]);
        let mut f = table();
        f.import_raw(&e.export_raw());
        assert_eq!(words(&f.lookup("zhegnshi").0), vec!["zhegnshi"]);
        assert!(e.forget("zhegnshi"));
        assert!(!e.forget("zhegnshi"));
        assert!(e.lookup("zhegnshi").0.is_empty());
    }

    #[test]
    fn 读回时把丢了大写首字母的残片并回整词() {
        // 用户真实数据:Johnson/Claude/Upwork 的大写首字母曾被放行给应用,回车学到 ohnson、laude、pwork
        let mut e = English::new(vec![("claude".into(), "Claude".into(), TIER_CUSTOM)]);
        e.import_raw("ohnson\t10\njohnson\t2\nlaude\t1\npwork\t1\nupwork\t3\nrguo\t1\n");
        assert_eq!(e.export_raw(), "claude\t1\njohnson\t12\nrguo\t1\nupwork\t4");
        // 词表里有的词即使另有 c+词 也不并:lay 与 play 都是正常词
        let mut e = English::new(vec![("lay".into(), "lay".into(), TIER_BASE)]);
        e.import_raw("lay\t2\nplay\t2\n");
        assert_eq!(e.export_raw(), "lay\t2\nplay\t2");
    }

    #[test]
    fn 解析时剥掉_utf8_bom() {
        assert_eq!(parse_list("\u{feff}subagent\n"), vec![s("subagent", "subagent")]);
    }
}
