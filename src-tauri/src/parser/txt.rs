//! 纯文本解析：自动识别编码（UTF-8 / UTF-16 BOM / GB18030）

use std::fs;
use std::path::Path;

use encoding_rs::{GB18030, UTF_16BE, UTF_16LE};

use crate::error::Result;
use crate::models::ParsedDoc;
use crate::parser::ParseOutput;

/// 虚拟分页大小（字符数）：纯文本没有物理页，按段落边界切页便于预览
const CHARS_PER_PAGE: usize = 3500;

pub fn parse(path: &Path) -> Result<ParseOutput> {
    let bytes = fs::read(path)?;
    let text = decode_bytes(&bytes);
    let text = normalize_newlines(&text);

    // 去掉 BOM 后按行切段；空行保留为独立段落，维持原文版式
    let paragraphs: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    let doc = ParsedDoc::from_blocks(paragraphs, CHARS_PER_PAGE);

    Ok(ParseOutput {
        doc,
        kind: crate::models::DocKind::Txt,
        needs_ocr: false,
        ocr_used: false,
        warnings: vec![],
    })
}

/// 编码探测：BOM 优先，其次 UTF-8 合法性，最后按 GB18030（兼容 GBK/GB2312）解码
pub fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).to_string();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (cow, _, _) = UTF_16LE.decode(bytes);
        return cow.to_string();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (cow, _, _) = UTF_16BE.decode(bytes);
        return cow.to_string();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let (cow, _, _) = GB18030.decode(bytes);
    cow.to_string()
}

pub fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// UTF-8 BOM 写入：Windows 记事本打开中文不乱码
pub const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_bom() {
        let bytes = [0xEF, 0xBB, 0xBF, b'a', b'b'];
        assert_eq!(decode_bytes(&bytes), "ab");
    }

    #[test]
    fn test_gbk() {
        // “中文” 的 GBK 编码
        let bytes = [0xD6, 0xD0, 0xCE, 0xC4];
        assert_eq!(decode_bytes(&bytes), "中文");
    }
}
