//! 读取 Rime 的 `*.dict.yaml`(词⇥拼音⇥权重),导入 `DictBuilder`。
use crate::dict::DictBuilder;
use crate::pinyin::canonical;

/// 缺省权重(如 others 词库没有权重列):低于任何显式权重 ≥1 的条目。
const DEFAULT_WEIGHT: u32 = 0;

/// 解析一行词条,返回 (键, 词, 权重)。注释、空行、格式不符、拼音含非小写字母的行返回 None。
pub fn parse_line(line: &str) -> Option<(String, &str, u32)> {
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut cols = line.split('\t');
    let (word, pinyin) = (cols.next()?, cols.next()?);
    let syllables: Vec<&str> = pinyin.split_whitespace().map(canonical).collect();
    let valid = !syllables.is_empty()
        && syllables.iter().all(|s| s.bytes().all(|b| b.is_ascii_lowercase()));
    if word.is_empty() || !valid {
        return None;
    }
    let weight = cols.next().and_then(|w| w.trim().parse::<u64>().ok());
    let weight = weight.map_or(DEFAULT_WEIGHT, |w| w.min(u64::from(u32::MAX)) as u32);
    Some((syllables.join(" "), word, weight))
}

/// 导入整份 dict.yaml 文本,跳过 `...` 之前的 YAML 头;返回导入的条数。
pub fn import(text: &str, builder: &mut DictBuilder) -> usize {
    let body = text.split_once("\n...\n").map_or(text, |(_, rest)| rest);
    let mut n = 0;
    for line in body.lines() {
        if let Some((key, word, weight)) = parse_line(line) {
            builder.add(&key, word, weight);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::Dict;

    #[test]
    fn 解析带权重的行() {
        assert_eq!(parse_line("阿基里斯\ta ji li si\t1370"), Some(("a ji li si".into(), "阿基里斯", 1370)));
    }

    #[test]
    fn 缺权重用默认值() {
        assert_eq!(parse_line("空落落\tkong luo luo"), Some(("kong luo luo".into(), "空落落", 0)));
    }

    #[test]
    fn 跳过注释空行和非法拼音() {
        assert_eq!(parse_line("# 注释"), None);
        assert_eq!(parse_line(""), None);
        assert_eq!(parse_line("A\tA\t1"), None);
        assert_eq!(parse_line("〇〇八\t100"), None); // 腾讯词库:无拼音列
    }

    #[test]
    fn 导入跳过yaml头() {
        let text = "---\nname: x\nsort: by_weight\n...\n你好\tni hao\t10\n你\tni\t5\n";
        let mut b = DictBuilder::default();
        assert_eq!(import(text, &mut b), 2);
        let d = Dict::from_bytes(b.build().unwrap()).unwrap();
        assert_eq!(d.get("ni hao")[0].word, "你好");
    }

    #[test]
    fn 词库里的_lue_键规范为_lve() {
        assert_eq!(parse_line("略\tlue\t100"), Some(("lve".into(), "略", 100)));
    }
}
