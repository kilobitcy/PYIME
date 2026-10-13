//! 词典编译器:`pyime-dict-build <输出文件> <dict.yaml>...`
use pyime_engine::dict::DictBuilder;
use pyime_engine::rime;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((out, inputs)) = args.split_first().filter(|(_, i)| !i.is_empty()) else {
        eprintln!("用法: pyime-dict-build <输出文件> <dict.yaml>...");
        std::process::exit(2);
    };
    let mut builder = DictBuilder::default();
    for path in inputs {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("读取 {path} 失败: {e}");
            std::process::exit(1);
        });
        eprintln!("{path}: 导入 {} 条", rime::import(&text, &mut builder));
    }
    let bytes = builder.build().unwrap_or_else(|e| {
        eprintln!("构建失败: {e}");
        std::process::exit(1);
    });
    std::fs::write(out, &bytes).expect("写出词典失败");
    eprintln!("已写出 {out}({} 字节)", bytes.len());
}
