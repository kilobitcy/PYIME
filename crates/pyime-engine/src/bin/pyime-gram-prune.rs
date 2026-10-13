//! 把 octagram 导出的键值表(pyime-gram-stat 的输出)裁成"词+词"的二元表。
//! 用法:pyime-gram-prune <词表.txt(每行一词)> <键值.tsv> <最小值> [输出.bin]
//! 只保留能拆成两个词(两侧都在词表里)且值不低于下限的键。
use std::collections::HashSet;
use std::io::{BufRead, BufReader};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let vocab: HashSet<String> =
        std::fs::read_to_string(&a[1]).expect("词表").lines().map(str::to_string).collect();
    let min: f64 = a[3].parse().expect("最小值");
    let mut hist = [0u64; 30];
    let mut kept: Vec<(String, u32)> = Vec::new();
    for line in BufReader::new(std::fs::File::open(&a[2]).expect("tsv")).lines() {
        let line = line.unwrap();
        let Some((k, v)) = line.split_once('\t') else { continue };
        let Ok(v) = v.parse::<f64>() else { continue };
        if v < min {
            continue;
        }
        let idx: Vec<usize> = k.char_indices().map(|(i, _)| i).skip(1).collect();
        if idx.iter().any(|&i| vocab.contains(&k[..i]) && vocab.contains(&k[i..])) {
            hist[(v as usize).min(29)] += 1;
            kept.push((k.to_string(), (v * 100.0).round() as u32));
        }
    }
    println!("保留 {} 条;按 ln 次数分布 {hist:?}", kept.len());
    if let Some(out) = a.get(4) {
        pyime_engine::bigram::write(&kept, out).expect("写出");
    }
}
