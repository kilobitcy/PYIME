//! 符号表:候选文字精确命中时,在其后附加的符号(`上` → `↑`、`二` → `② Ⅱ`)。
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Symbols {
    map: HashMap<String, Vec<String>>,
}

impl Symbols {
    /// 解析 `词⇥符号1 符号2` 文本:去文件头 BOM,`#` 注释与空行跳过,缺制表符或无符号的行跳过,
    /// 同一词重复出现时以首行为准。
    pub fn parse(text: &str) -> Symbols {
        let mut map = HashMap::new();
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((word, syms)) = line.split_once('\t') else { continue };
            let syms: Vec<String> = syms.split_whitespace().map(str::to_string).collect();
            let word = word.trim();
            if word.is_empty() || syms.is_empty() {
                continue;
            }
            map.entry(word.to_string()).or_insert(syms);
        }
        Symbols { map }
    }

    /// 该词对应的符号(按表内顺序),没有则为空。
    pub fn get(&self, word: &str) -> &[String] {
        self.map.get(word).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 解析注释空行坏行() {
        let s = Symbols::parse("# 注释\n\n上\t↑\n坏行没有制表符\n下\t\n二\t② Ⅱ ⑵\n");
        assert_eq!(s.get("上"), ["↑"]);
        assert_eq!(s.get("二"), ["②", "Ⅱ", "⑵"]);
        assert!(s.get("下").is_empty());
        assert!(s.get("坏行没有制表符").is_empty());
        assert!(s.get("左").is_empty());
    }

    #[test]
    fn 去_bom_与_crlf_重复词以首行为准() {
        let s = Symbols::parse("\u{feff}上\t↑\r\n上\t⇧\r\n");
        assert_eq!(s.get("上"), ["↑"]);
    }

    #[test]
    fn 自带符号表可解析() {
        let s = Symbols::parse(include_str!("../../../installer/symbols.txt"));
        assert_eq!(s.get("上"), ["↑"]);
        assert_eq!(s.get("二"), ["②", "Ⅱ"]);
    }
}
