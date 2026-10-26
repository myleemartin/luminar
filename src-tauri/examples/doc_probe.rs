// 开发期探针：验证 Word 97-2003 二进制解析
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("doc path");
    let out = luminar_lib::parser::doc_binary::parse(Path::new(&path))?;
    println!("kind={:?} chars={}", out.kind, out.doc.text.chars().count());
    println!("{}", out.doc.text.chars().take(300).collect::<String>());
    Ok(())
}
