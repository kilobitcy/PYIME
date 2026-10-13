//! 词典:fst 索引(拼音键 → 词条块位置)+ 词条数据块。
//!
//! 文件布局:`PYDICT01` | fst 长度(u64 LE) | fst 字节 | 数据块。
//! 键是以空格分隔的音节串(如 `ni hao`);fst 的值 = 数据块偏移(高 48 位) | 词条数(低 16 位)。
//! 数据块里每条词:权重(u32 LE) | 词长(u16 LE) | UTF-8 词;同一键下按权重从高到低。
use fst::automaton::{Automaton, Str};
use fst::{IntoStreamer, Map, MapBuilder, Streamer};
use std::collections::BTreeMap;
use std::sync::Arc;

const MAGIC: &[u8; 8] = b"PYDICT01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub word: String,
    pub weight: u32,
}

#[derive(Default)]
pub struct DictBuilder {
    map: BTreeMap<String, BTreeMap<String, u32>>,
}

impl DictBuilder {
    /// 添加一条;同一(键,词)重复出现时保留较大的权重。
    pub fn add(&mut self, key: &str, word: &str, weight: u32) {
        let w = self.map.entry(key.to_string()).or_default().entry(word.to_string()).or_insert(0);
        *w = (*w).max(weight);
    }

    pub fn build(self) -> Result<Vec<u8>, String> {
        let mut blob = Vec::new();
        let mut fst = MapBuilder::memory();
        for (key, words) in self.map {
            let mut list: Vec<(String, u32)> = words.into_iter().collect();
            list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let count = u16::try_from(list.len()).map_err(|_| format!("键 {key:?} 词条过多"))?;
            let offset = blob.len() as u64;
            for (word, weight) in &list {
                let len = u16::try_from(word.len()).map_err(|_| format!("词过长: {word}"))?;
                blob.extend_from_slice(&weight.to_le_bytes());
                blob.extend_from_slice(&len.to_le_bytes());
                blob.extend_from_slice(word.as_bytes());
            }
            fst.insert(&key, offset << 16 | u64::from(count)).map_err(|e| e.to_string())?;
        }
        let index = fst.into_inner().map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(16 + index.len() + blob.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(index.len() as u64).to_le_bytes());
        out.extend_from_slice(&index);
        out.extend_from_slice(&blob);
        Ok(out)
    }
}

/// 共享同一份字节、只读取其中一段,供 fst 零拷贝使用。
struct Slice {
    data: Arc<Vec<u8>>,
    start: usize,
    end: usize,
}

impl AsRef<[u8]> for Slice {
    fn as_ref(&self) -> &[u8] {
        &self.data[self.start..self.end]
    }
}

pub struct Dict {
    index: Map<Slice>,
    data: Arc<Vec<u8>>,
    blob_start: usize,
}

impl Dict {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Dict, String> {
        if bytes.len() < 16 || &bytes[..8] != MAGIC {
            return Err("不是 PYIME 词典文件".into());
        }
        let fst_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
        let blob_start = 16usize.checked_add(fst_len).filter(|&e| e <= bytes.len()).ok_or("词典文件被截断")?;
        let data = Arc::new(bytes);
        let slice = Slice { data: data.clone(), start: 16, end: blob_start };
        let index = Map::new(slice).map_err(|e| e.to_string())?;
        Ok(Dict { index, data, blob_start })
    }

    fn read(&self, value: u64) -> Vec<Entry> {
        let (mut pos, count) = (self.blob_start + (value >> 16) as usize, (value & 0xffff) as usize);
        let b = &self.data;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let weight = u32::from_le_bytes(b[pos..pos + 4].try_into().unwrap());
            let len = u16::from_le_bytes(b[pos + 4..pos + 6].try_into().unwrap()) as usize;
            let word = String::from_utf8_lossy(&b[pos + 6..pos + 6 + len]).into_owned();
            out.push(Entry { word, weight });
            pos += 6 + len;
        }
        out
    }

    /// 精确查键,词条按权重从高到低。
    pub fn get(&self, key: &str) -> Vec<Entry> {
        self.index.get(key).map(|v| self.read(v)).unwrap_or_default()
    }

    /// 是否存在以 `prefix` 开头的键(不读取词条,用于剪枝)。
    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.index.search(Str::new(prefix).starts_with()).into_stream().next().is_some()
    }

    /// 以 `prefix` 开头的键(按字典序),最多 `limit` 个。
    pub fn prefix(&self, prefix: &str, limit: usize) -> Vec<(String, Vec<Entry>)> {
        let mut stream = self.index.search(Str::new(prefix).starts_with()).into_stream();
        let mut out = Vec::new();
        while let Some((key, value)) = stream.next() {
            if out.len() >= limit {
                break;
            }
            out.push((String::from_utf8_lossy(key).into_owned(), self.read(value)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Dict {
        let mut b = DictBuilder::default();
        b.add("ni hao", "你好", 500);
        b.add("ni hao", "拟好", 5);
        b.add("ni hao", "你好", 900); // 重复取较大权重
        b.add("ni", "你", 1000);
        b.add("ni men", "你们", 300);
        Dict::from_bytes(b.build().unwrap()).unwrap()
    }

    #[test]
    fn 精确查询按权重降序且去重() {
        let got = sample().get("ni hao");
        let words: Vec<_> = got.iter().map(|e| (e.word.as_str(), e.weight)).collect();
        assert_eq!(words, [("你好", 900), ("拟好", 5)]);
    }

    #[test]
    fn 未命中返回空() {
        assert!(sample().get("wo").is_empty());
    }

    #[test]
    fn 前缀查询按字典序并受上限约束() {
        let d = sample();
        let keys: Vec<_> = d.prefix("ni", 10).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["ni", "ni hao", "ni men"]);
        assert_eq!(d.prefix("ni", 2).len(), 2);
    }

    #[test]
    fn 损坏文件报错() {
        assert!(Dict::from_bytes(b"garbage".to_vec()).is_err());
        let mut bytes = DictBuilder::default().build().unwrap();
        bytes.truncate(10);
        assert!(Dict::from_bytes(bytes).is_err());
    }
}
