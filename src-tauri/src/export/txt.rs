//! TXT 导出：UTF-8（带 BOM），保证 Windows 记事本与 Word 打开不乱码

use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::parser::txt::UTF8_BOM;

pub fn write_txt(path: &Path, text: &str) -> Result<()> {
    let mut buf = Vec::with_capacity(text.len() + 3);
    buf.extend_from_slice(&UTF8_BOM);
    buf.extend_from_slice(text.as_bytes());
    fs::write(path, buf)?;
    Ok(())
}
