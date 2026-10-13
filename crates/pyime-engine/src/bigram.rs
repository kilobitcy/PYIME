//! 二元词频表:fst 映射 "前词+后词" → ln(次数)×100。由 pyime-gram-prune 从 octagram 模型裁出。
use fst::{Map, MapBuilder};

const MAGIC: &[u8; 8] = b"PYBIGR01";

/// 写出;`items` 无需有序。
pub fn write(items: &[(String, u32)], path: &str) -> Result<(), String> {
    let mut sorted: Vec<&(String, u32)> = items.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted.dedup_by(|a, b| a.0 == b.0);
    let mut b = MapBuilder::memory();
    for (k, v) in sorted {
        b.insert(k, u64::from(*v)).map_err(|e| e.to_string())?;
    }
    let body = b.into_inner().map_err(|e| e.to_string())?;
    let mut out = MAGIC.to_vec();
    out.extend(body);
    std::fs::write(path, out).map_err(|e| e.to_string())
}

pub struct Bigram {
    map: Map<Vec<u8>>,
}

impl Bigram {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Bigram, String> {
        if bytes.len() < 8 || &bytes[..8] != MAGIC {
            return Err("不是 PYIME 二元词频表".into());
        }
        Ok(Bigram { map: Map::new(bytes[8..].to_vec()).map_err(|e| e.to_string())? })
    }

    /// 前词 `prev` 后接 `word` 的 ln 次数,未收录为 None。
    pub fn get(&self, prev: &str, word: &str) -> Option<f64> {
        self.map.get(format!("{prev}{word}")).map(|v| v as f64 / 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 写出再读回() {
        let path = std::env::temp_dir().join("pyime_bigram_test.bin");
        write(&[("再追加".into(), 1365), ("我爱你".into(), 1590)], path.to_str().unwrap()).unwrap();
        let b = Bigram::from_bytes(std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(b.get("再", "追加"), Some(13.65));
        assert_eq!(b.get("在", "追加"), None);
        assert!(Bigram::from_bytes(b"garbage!".to_vec()).is_err());
    }
}
