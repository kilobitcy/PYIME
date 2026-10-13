//! 统计 .gram 里的键:数量、长度分布、值分布。用法:pyime-gram-stat <x.gram> [输出键文件]
use pyime_engine::gram::Gram;
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let gram = Gram::from_bytes(std::fs::read(&args[1]).expect("读取失败")).expect("格式错误");
    let mut out = args.get(2).map(|p| std::io::BufWriter::new(std::fs::File::create(p).unwrap()));
    let (mut total, mut by_len, mut by_val) = (0u64, [0u64; 12], [0u64; 24]);
    gram.for_each(|k, v| {
        total += 1;
        by_len[k.chars().count().min(11)] += 1;
        by_val[(v as usize).min(23)] += 1;
        if let Some(o) = out.as_mut() {
            writeln!(o, "{k}\t{v:.4}").unwrap();
        }
    });
    println!("键总数 {total}\n按字数 {by_len:?}\n按 ln 次数 {by_val:?}");
}
