//! Word 97-2003 二进制 `.doc` 解析（不依赖 LibreOffice）
//!
//! `.doc` 是 OLE 复合文档（CFB），正文不一定连续存放：
//! - `WordDocument` 流开头是 FIB（File Information Block）；
//! - `0Table` / `1Table` 流里的 CLX 保存 piece table；
//! - 每个 piece 记录字符区间和文件偏移，压缩 piece 是单字节代码页（中文为 GBK），
//!   未压缩 piece 是 UTF-16LE。
//!
//! 这里只提取**正文文字**，不保留版式；复杂/加密/Word 6-95 文档会返回错误，
//! 由 `legacy.rs` 回退到 LibreOffice 或 textutil。

use std::io::Read;
use std::path::Path;

use encoding_rs::GB18030;

use crate::error::{AppError, Result};
use crate::models::{DocKind, ParsedDoc};
use crate::parser::ParseOutput;

const CHARS_PER_PAGE: usize = 3500;

pub fn parse(path: &Path) -> Result<ParseOutput> {
    let mut comp = cfb::open(path)
        .map_err(|e| AppError::Unsupported(format!("不是 Word 97-2003 二进制文档：{e}")))?;
    let mut word = Vec::new();
    comp.open_stream("/WordDocument")
        .and_then(|mut s| s.read_to_end(&mut word))
        .map_err(|e| AppError::Unsupported(format!("读取 WordDocument 流失败：{e}")))?;

    if word.len() < 0x200 || u16_at(&word, 0)? != 0xA5EC {
        return Err(AppError::Unsupported("不是有效的 Word 97-2003 文档".into()));
    }
    let n_fib = u16_at(&word, 0x02)?;
    if n_fib < 0x00C1 {
        return Err(AppError::Unsupported(format!(
            "Word 6/95 旧文档（nFib=0x{n_fib:04X}）暂不支持原生解析，请另存为 .docx"
        )));
    }
    let flags = u16_at(&word, 0x0A)?;
    if flags & 0x0100 != 0 {
        return Err(AppError::Unsupported("文档已加密，无法读取正文".into()));
    }
    let complex = flags & 0x0004 != 0;
    let table_one = flags & 0x0200 != 0;
    let ext_char = flags & 0x1000 != 0;
    let fc_min = u32_at(&word, 0x18)? as usize;
    let ccp_text = u32_at(&word, 0x4C)? as usize;

    let text = if complex {
        let table_name = if table_one { "/1Table" } else { "/0Table" };
        let mut table = Vec::new();
        comp.open_stream(table_name)
            .and_then(|mut s| s.read_to_end(&mut table))
            .map_err(|e| AppError::Unsupported(format!("读取 {table_name} 流失败：{e}")))?;
        extract_complex(&word, &table, ccp_text)?
    } else {
        extract_simple(&word, fc_min, ccp_text, ext_char)?
    };

    let text = clean_text(&text);
    if text.trim().is_empty() {
        return Err(AppError::Unsupported("Word 文档中没有可提取的正文".into()));
    }
    let doc = ParsedDoc::from_blocks(
        text.split('\n').map(|s| s.to_string()).collect(),
        CHARS_PER_PAGE,
    );
    Ok(ParseOutput {
        doc,
        kind: DocKind::DocLegacy,
        needs_ocr: false,
        ocr_used: false,
        warnings: vec![
            "已按 Word 97-2003 二进制格式解析正文（不保留原排版，导出时生成新 Word）。".into(),
        ],
    })
}

fn extract_simple(word: &[u8], fc_min: usize, ccp_text: usize, ext_char: bool) -> Result<String> {
    if ccp_text == 0 {
        return Ok(String::new());
    }
    if ext_char {
        let end = fc_min.saturating_add(ccp_text.saturating_mul(2));
        if end > word.len() {
            return Err(AppError::Unsupported("Word 正文超出文件范围".into()));
        }
        let units: Vec<u16> = word[fc_min..end]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    } else {
        let end = fc_min.saturating_add(ccp_text);
        if end > word.len() {
            return Err(AppError::Unsupported("Word 正文超出文件范围".into()));
        }
        Ok(GB18030.decode(&word[fc_min..end]).0.to_string())
    }
}

fn extract_complex(word: &[u8], table: &[u8], ccp_text: usize) -> Result<String> {
    let fc_clx = u32_at(word, 0x01A2)? as usize;
    let lcb_clx = u32_at(word, 0x01A6)? as usize;
    if lcb_clx == 0 || fc_clx.saturating_add(lcb_clx) > table.len() {
        return Err(AppError::Unsupported("Word piece table 位置异常".into()));
    }
    let clx = &table[fc_clx..fc_clx + lcb_clx];
    let mut pos = 0usize;
    // 跳过 Prc（0x01 + cbGrpprl + 数据），直到 Pcdt（0x02）
    while pos < clx.len() && clx[pos] == 0x01 {
        if pos + 3 > clx.len() {
            return Err(AppError::Unsupported("Word CLX 结构异常".into()));
        }
        let cb = u16::from_le_bytes([clx[pos + 1], clx[pos + 2]]) as usize;
        pos = pos.saturating_add(3 + cb);
    }
    if pos + 5 > clx.len() || clx[pos] != 0x02 {
        return Err(AppError::Unsupported("Word CLX 缺少 piece table".into()));
    }
    let lcb = u32::from_le_bytes([clx[pos + 1], clx[pos + 2], clx[pos + 3], clx[pos + 4]]) as usize;
    let plc_start = pos + 5;
    if plc_start.saturating_add(lcb) > clx.len() || lcb < 4 {
        return Err(AppError::Unsupported("Word piece table 长度异常".into()));
    }
    let plc = &clx[plc_start..plc_start + lcb];
    let n = (lcb - 4) / 12;
    if n == 0 {
        return Err(AppError::Unsupported("Word piece table 为空".into()));
    }
    let cps = 4 * (n + 1);
    if cps > plc.len() {
        return Err(AppError::Unsupported("Word CP 数组长度异常".into()));
    }

    let mut out = String::new();
    for i in 0..n {
        let cp_start = u32_at(plc, i * 4)? as usize;
        let cp_end = u32_at(plc, (i + 1) * 4)? as usize;
        if cp_start >= ccp_text {
            break;
        }
        let cp_end = cp_end.min(ccp_text);
        if cp_end <= cp_start {
            continue;
        }
        let pcd = &plc[cps + i * 8..cps + i * 8 + 8];
        let raw_fc = u32::from_le_bytes([pcd[2], pcd[3], pcd[4], pcd[5]]);
        let compressed = raw_fc & 0x4000_0000 != 0;
        let mut fc = (raw_fc & 0x3FFF_FFFF) as usize;
        let count = cp_end - cp_start;
        if compressed {
            fc /= 2;
            if fc.saturating_add(count) > word.len() {
                return Err(AppError::Unsupported("Word 压缩片段超出文件范围".into()));
            }
            out.push_str(&GB18030.decode(&word[fc..fc + count]).0);
        } else {
            let bytes = count.saturating_mul(2);
            if fc.saturating_add(bytes) > word.len() {
                return Err(AppError::Unsupported("Word 文本片段超出文件范围".into()));
            }
            let units: Vec<u16> = word[fc..fc + bytes]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            out.push_str(&String::from_utf16_lossy(&units));
        }
    }
    Ok(out)
}

/// 清理 Word 控制字符：段落、单元格、分页、域代码等
fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_field = false;
    for ch in s.chars() {
        match ch {
            '\r' | '\n' => {
                if !in_field && !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            '\u{0007}' => out.push('\t'), // 单元格/行结束
            '\u{000B}' | '\u{000C}' => out.push('\n'),
            '\u{0001}' | '\u{0008}' => out.push(' '), // 图片/图形占位
            '\u{0013}' => in_field = true,            // 域代码开始
            '\u{0014}' | '\u{0015}' => in_field = false,
            '\u{001E}' => out.push('-'),
            '\u{001F}' => {}
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => {
                if !in_field {
                    out.push(c);
                }
            }
        }
    }
    out.replace("\n\n\n", "\n\n")
        .trim_end_matches('\n')
        .trim_start()
        .to_string()
}

fn u16_at(data: &[u8], off: usize) -> Result<u16> {
    if off + 2 > data.len() {
        return Err(AppError::Unsupported("Word 文档结构被截断".into()));
    }
    Ok(u16::from_le_bytes([data[off], data[off + 1]]))
}

fn u32_at(data: &[u8], off: usize) -> Result<u32> {
    if off + 4 > data.len() {
        return Err(AppError::Unsupported("Word 文档结构被截断".into()));
    }
    Ok(u32::from_le_bytes([
        data[off],
        data[off + 1],
        data[off + 2],
        data[off + 3],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_synthetic_word97_doc() {
        // 用 cfb 写入一个最小 Word 97 结构：FIB + 连续 UTF-16 正文。
        // 真实 Word 的 piece table 版本由开发期 doc_probe 在真实 .doc 上验证。
        let dir =
            std::env::temp_dir().join(format!("luminar_doc_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.doc");
        let text = "原告：张三，电话13812345678。\r";
        let units: Vec<u16> = text.encode_utf16().collect();
        let fc_min = 0x600usize;
        let mut word = vec![0u8; fc_min + units.len() * 2];
        word[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
        word[2..4].copy_from_slice(&0x00C1u16.to_le_bytes());
        word[0x0A..0x0C].copy_from_slice(&0x1000u16.to_le_bytes()); // fExtChar
        word[0x18..0x1C].copy_from_slice(&(fc_min as u32).to_le_bytes());
        word[0x4C..0x50].copy_from_slice(&(units.len() as u32).to_le_bytes());
        for (i, u) in units.iter().enumerate() {
            let off = fc_min + i * 2;
            word[off..off + 2].copy_from_slice(&u.to_le_bytes());
        }
        {
            let mut comp = cfb::create(&path).unwrap();
            comp.create_stream("/WordDocument")
                .unwrap()
                .write_all(&word)
                .unwrap();
            comp.flush().unwrap();
        }
        let out = parse(&path).expect("原生 .doc 解析失败");
        assert!(out.doc.text.contains("张三"), "实际：{}", out.doc.text);
        assert!(out.doc.text.contains("13812345678"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
