//! 按键纠错:把输入只改一处(相邻键替换、删一个字母、相邻互换、补一个常漏的字母),供整段词兜底查询。

/// QWERTY 相邻键:同排左右、上下排斜邻。
fn neighbors(c: u8) -> &'static [u8] {
    match c {
        b'q' => b"wa",
        b'w' => b"qeas",
        b'e' => b"wrsd",
        b'r' => b"etdf",
        b't' => b"ryfg",
        b'y' => b"tugh",
        b'u' => b"yihj",
        b'i' => b"uojk",
        b'o' => b"ipkl",
        b'p' => b"ol",
        b'a' => b"qwsz",
        b's' => b"weadzx",
        b'd' => b"erfsxc",
        b'f' => b"rtdgcv",
        b'g' => b"tyfhvb",
        b'h' => b"yugjbn",
        b'j' => b"uihknm",
        b'k' => b"iojlm",
        b'l' => b"opk",
        b'z' => b"asx",
        b'x' => b"sdzc",
        b'c' => b"dfxv",
        b'v' => b"fgcb",
        b'b' => b"ghvn",
        b'n' => b"hjbm",
        b'm' => b"jkn",
        _ => b"",
    }
}

/// 漏打时可补的字母(韵母元音与 zh/ch/sh、ang/eng 里的 h、g)。
const INSERTS: &[u8] = b"aeiouhg";

/// 只改一处的变体:相邻键替换、删一个字母、相邻两字母互换、任意位置补 `INSERTS` 之一。
/// 去重,不含原串;输入为空或含小写字母以外的字符时返回空。
pub fn edits(input: &str) -> Vec<String> {
    let s = input.as_bytes();
    if s.is_empty() || !s.iter().all(u8::is_ascii_lowercase) {
        return Vec::new();
    }
    let mut out: Vec<Vec<u8>> = Vec::new();
    for i in 0..s.len() {
        for &n in neighbors(s[i]) {
            let mut v = s.to_vec();
            v[i] = n;
            out.push(v);
        }
        let mut v = s.to_vec();
        v.remove(i);
        out.push(v);
        if i + 1 < s.len() && s[i] != s[i + 1] {
            let mut v = s.to_vec();
            v.swap(i, i + 1);
            out.push(v);
        }
    }
    for i in 0..=s.len() {
        for &c in INSERTS {
            let mut v = s.to_vec();
            v.insert(i, c);
            out.push(v);
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.into_iter()
        .filter(|v| !v.is_empty() && v.as_slice() != s)
        .filter_map(|v| String::from_utf8(v).ok())
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(input: &str, want: &str) -> bool {
        edits(input).iter().any(|e| e == want)
    }

    #[test]
    fn 四类改法各能得到本意() {
        assert!(has("tuojian", "tuijian")); // 替换:o→i 相邻
        assert!(has("thuijian", "tuijian")); // 删除:多打的 h
        assert!(has("nie", "nei")); // 互换
        assert!(has("xiachenxu", "xiaochenxu")); // 插入:漏打的 o
        assert!(has("wom", "won")); // 替换:m→n 相邻
    }

    #[test]
    fn 不含原串_无重复_只换相邻键() {
        let v = edits("tuojian");
        assert!(!v.iter().any(|e| e == "tuojian"));
        let mut u = v.clone();
        u.sort();
        u.dedup();
        assert_eq!(u.len(), v.len());
        assert!(!has("ab", "ac")); // b 的邻键是 g h v n,不含 c
    }

    #[test]
    fn 非小写字母输入返回空() {
        assert!(edits("ni'hao").is_empty());
        assert!(edits("").is_empty());
    }
}
