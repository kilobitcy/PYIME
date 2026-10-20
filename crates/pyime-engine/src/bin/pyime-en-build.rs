//! 英文词表编译器:`pyime-en-build <输出 pyime.en> <en_it.txt> <en_ext.dict.yaml> <en.dict.yaml>`
//! 自带 IT 词表为档位 0,rime-ice en_ext 为 1、en 为 2;同键同词留最小档位。
use pyime_engine::english::{parse_list, parse_rime, English, TIER_BASE, TIER_CUSTOM, TIER_EXT};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [out, it, ext, base] = args.as_slice() else {
        eprintln!("用法: pyime-en-build <输出 pyime.en> <en_it.txt> <en_ext.dict.yaml> <en.dict.yaml>");
        std::process::exit(2);
    };
    let read = |p: &str| {
        std::fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("读取 {p} 失败: {e}");
            std::process::exit(1);
        })
    };
    let mut items = Vec::new();
    for (list, tier) in [(parse_list(&read(it)), TIER_CUSTOM), (parse_rime(&read(ext)), TIER_EXT), (parse_rime(&read(base)), TIER_BASE)] {
        eprintln!("档位 {tier}: {} 条", list.len());
        items.extend(list.into_iter().map(|(k, w)| (k, w, tier)));
    }
    let en = English::new(items);
    std::fs::write(out, en.export()).expect("写出英文词表失败");
    eprintln!("已写出 {out}({} 条)", en.len());
}
