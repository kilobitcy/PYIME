//! 读取 librime-octagram 的 `.gram` 语法模型(LGPL-3.0,rime-octagram-data):
//! Darts-clone 双数组 trie,键是"上文+词"的紧凑编码,值是 ln(次数)×10000。
//! 这里只实现只读查询,用于给相邻词的搭配加分。

const HEADER: &[u8] = b"Rime::Grammar/";
const ARRAY_START: usize = 0x2c; // 元数据 0x28 处是自相对的偏移指针,其值恒为 4
const VALUE_SCALE: f64 = 10000.0;
/// 与 octagram 默认一致:搭配最长 4 个字,上文最多取 3 个字。
const MAX_CONTEXT: usize = 3;
/// 搭配总长不足 3 字且未覆盖整个查询的"弱搭配"额外减分(octagram 默认 -24 对 -12)。
const WEAK_PENALTY: f64 = 12.0;
const MIN_COLLOCATION: usize = 3;

pub struct Gram {
    data: Vec<u8>,
    units: usize,
}

/// octagram 的字符编码(见 gram_encoding.cc)。
fn encode(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for ch in s.chars() {
        let u = ch as u32;
        if u < 0x80 {
            out.push(if u == 0 { 0xE0 } else { u as u8 });
        } else if (0x4000..0xA000).contains(&u) {
            if u & 0xFF == 0 {
                out.extend([0xE1, ((u >> 8) + 0x40) as u8]);
            } else {
                out.extend([((u >> 8) + 0x40) as u8, (u & 0xFF) as u8]);
            }
        } else {
            let (mut bits, mut v) = (32i32, u);
            while bits > 0 && v & 0xFE00_0000 == 0 {
                bits -= 7;
                v <<= 7;
            }
            let n = (bits + 6) / 7;
            out.push(0xE0 | n as u8);
            for _ in 0..n {
                out.push((((v >> 25) & 0x7F) | 0x80) as u8);
                v <<= 7;
            }
        }
    }
    out
}

/// `encode` 的逆过程;遇到非法序列返回 None。
pub fn decode(bytes: &[u8]) -> Option<String> {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        let u = match b {
            0..=0x7F => u32::from(b),
            0xE0 => 0,
            0xE1 => {
                let n = *bytes.get(i)?;
                i += 1;
                (u32::from(n).checked_sub(0x40)?) << 8
            }
            0x80..=0xDF => {
                let n = *bytes.get(i)?;
                i += 1;
                ((u32::from(b) - 0x40) << 8) | u32::from(n)
            }
            _ => return None, // 码点 ≥U+A000 的长编码丢失了位置信息,不还原(词表里也几乎没有)
        };
        out.push(char::from_u32(u)?);
    }
    Some(out)
}

impl Gram {
    pub fn from_bytes(data: Vec<u8>) -> Result<Gram, String> {
        if data.len() < ARRAY_START || !data.starts_with(HEADER) {
            return Err("不是 octagram 语法模型".into());
        }
        let units = u32::from_le_bytes(data[0x24..0x28].try_into().unwrap()) as usize;
        if ARRAY_START + units * 4 > data.len() {
            return Err("语法模型被截断".into());
        }
        Ok(Gram { data, units })
    }

    fn unit(&self, i: usize) -> Option<u32> {
        (i < self.units).then(|| u32::from_le_bytes(self.data[ARRAY_START + i * 4..][..4].try_into().unwrap()))
    }

    /// 沿键走到的节点;键不在 trie 里则为 None。
    fn traverse(&self, key: &[u8]) -> Option<usize> {
        let (mut id, mut unit) = (0usize, self.unit(0)?);
        for &c in key {
            id ^= offset(unit) ^ c as usize;
            unit = self.unit(id)?;
            if label(unit) != u32::from(c) {
                return None;
            }
        }
        Some(id)
    }

    /// 从 `node` 出发,`word` 的各个前缀命中的 (字节长度, 值)。
    fn prefix_matches(&self, node: usize, word: &[u8]) -> Vec<(usize, u32)> {
        let mut out = Vec::new();
        let Some(mut unit) = self.unit(node) else { return out };
        let mut id = node ^ offset(unit);
        for (i, &c) in word.iter().enumerate() {
            id ^= c as usize;
            let Some(u) = self.unit(id) else { break };
            if label(u) != u32::from(c) {
                break;
            }
            unit = u;
            id ^= offset(unit);
            if has_leaf(unit) {
                if let Some(v) = self.unit(id) {
                    out.push((i + 1, v & 0x7FFF_FFFF));
                }
            }
        }
        out
    }

    /// 字符串 `s` 作为整条 n-gram 键的 ln(次数),不存在则 None。
    pub fn count(&self, s: &str) -> Option<f64> {
        let key = encode(s);
        let node = self.traverse(&key)?;
        let unit = self.unit(node)?;
        has_leaf(unit).then(|| self.unit(node ^ offset(unit))).flatten().map(|v| f64::from(v & 0x7FFF_FFFF) / VALUE_SCALE)
    }

    /// 深度优先枚举全部键,回调 (键字符串, ln 次数)。仅供离线裁剪工具使用。
    pub fn for_each(&self, mut f: impl FnMut(&str, f64)) {
        let mut key: Vec<u8> = Vec::new();
        // 栈元素:(节点, 下一个要试的子标签 0..=256)
        let mut stack: Vec<(usize, u32)> = vec![(0, 0)];
        while let Some(top) = stack.last_mut() {
            let (node, next) = *top;
            let Some(unit) = self.unit(node) else {
                stack.pop();
                key.pop();
                continue;
            };
            if next == 0 {
                if has_leaf(unit) && !key.is_empty() {
                    if let (Some(v), Some(text)) = (self.unit(node ^ offset(unit)), decode(&key)) {
                        f(&text, f64::from(v & 0x7FFF_FFFF) / VALUE_SCALE);
                    }
                }
                top.1 = 1;
                continue;
            }
            if next > 255 {
                stack.pop();
                key.pop();
                continue;
            }
            top.1 += 1;
            let child = node ^ offset(unit) ^ next as usize;
            if self.unit(child).is_some_and(|u| label(u) == next) {
                key.push(next as u8);
                stack.push((child, 0));
            }
        }
    }

    /// `context`(前文)之后接 `word` 的搭配加分(自然对数域,无搭配为 0)。
    pub fn bonus(&self, context: &str, word: &str) -> f64 {
        let ctx: Vec<char> = context.chars().rev().take(MAX_CONTEXT).collect::<Vec<_>>().into_iter().rev().collect();
        let w: Vec<char> = word.chars().take(MAX_CONTEXT).collect();
        if ctx.is_empty() || w.is_empty() {
            return 0.0;
        }
        // 词前缀各长度对应的编码字节数
        let w_bytes: Vec<usize> = (1..=w.len()).map(|k| encode(&w[..k].iter().collect::<String>()).len()).collect();
        let word_query = encode(&w.iter().collect::<String>());
        let mut best = 0.0f64;
        for start in 0..ctx.len() {
            let suffix: String = ctx[start..].iter().collect();
            let Some(node) = self.traverse(&encode(&suffix)) else { continue };
            for (len, value) in self.prefix_matches(node, &word_query) {
                let Some(k) = w_bytes.iter().position(|&b| b == len).map(|i| i + 1) else { continue };
                let whole = start == 0 && len == word_query.len();
                let weak = ctx.len() - start + k < MIN_COLLOCATION && !whole;
                best = best.max(f64::from(value) / VALUE_SCALE - if weak { WEAK_PENALTY } else { 0.0 });
            }
        }
        best
    }
}

fn offset(u: u32) -> usize {
    ((u >> 10) << ((u & 0x200) >> 6)) as usize
}
fn label(u: u32) -> u32 {
    u & (0x8000_0000 | 0xFF)
}
fn has_leaf(u: u32) -> bool {
    (u >> 8) & 1 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 编码与_octagram_一致() {
        assert_eq!(encode("a"), [b'a']);
        assert_eq!(encode("再"), [(0x518D >> 8) as u8 + 0x40, 0x8D]);
        assert_eq!(encode("一"), [0xE1, (0x4E >> 0) + 0x40]); // U+4E00 低字节为 0
    }

    #[test]
    fn 解码是编码的逆过程() {
        for s in ["再追加", "a\u{4e00}b"] {
            assert_eq!(decode(&encode(s)).as_deref(), Some(s));
        }
    }

    #[test]
    fn 非模型文件报错() {
        assert!(Gram::from_bytes(b"garbage".to_vec()).is_err());
    }
}
