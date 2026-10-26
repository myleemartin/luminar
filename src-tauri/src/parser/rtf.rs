//! RTF 解析：提取正文文本
//!
//! 很多法院/仲裁机构和老版 Word 会导出 `.rtf`。RTF 是 7-bit 控制字 + 代码页转义，
//! 这里实现一个只取正文的轻量解析器：
//! - 跳过字体表、样式表、图片、页眉页脚等目标组；
//! - 支持 `\uN` Unicode 与 `\'hh` 代码页字节（中文 Word 常为 GBK）；
//! - `\par` / `\line` / `\page` 转成换行，保留段落。

use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::models::{DocKind, ParsedDoc};
use crate::parser::txt::{decode_bytes, normalize_newlines};
use crate::parser::ParseOutput;

const CHARS_PER_PAGE: usize = 3500;

pub fn parse(path: &Path) -> Result<ParseOutput> {
    let bytes = fs::read(path)?;
    let raw = String::from_utf8_lossy(&bytes);
    let text = normalize_newlines(&extract_text(&raw));
    let paragraphs: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    let doc = ParsedDoc::from_blocks(paragraphs, CHARS_PER_PAGE);
    let mut warnings = Vec::new();
    if doc.text.trim().is_empty() {
        warnings.push("RTF 中没有提取到正文文字，可能是纯图片或加密文档。".to_string());
    }
    Ok(ParseOutput {
        doc,
        kind: DocKind::Rtf,
        needs_ocr: false,
        ocr_used: false,
        warnings,
    })
}

/// 需要跳过的 RTF 目标组
fn is_destination(word: &str) -> bool {
    matches!(
        word,
        "fonttbl"
            | "colortbl"
            | "stylesheet"
            | "info"
            | "pict"
            | "object"
            | "header"
            | "headerl"
            | "headerr"
            | "headerf"
            | "footer"
            | "footerl"
            | "footerr"
            | "footerf"
            | "footnote"
            | "annotation"
            | "comment"
            | "listtable"
            | "listoverridetable"
            | "generator"
            | "themedata"
            | "datastore"
            | "xmlnstbl"
            | "latentstyles"
            | "rsidtbl"
            | "filetbl"
            | "revtbl"
            | "fldinst"
            | "bkmkstart"
            | "bkmkend"
    )
}

struct Extractor {
    out: String,
    bytes: Vec<u8>,
    stack: Vec<bool>,
    skip: bool,
    /// `\uN` 之后要跳过的 fallback 字符数
    uc_skip: usize,
}

impl Extractor {
    fn flush(&mut self) {
        if self.bytes.is_empty() {
            return;
        }
        let text = decode_bytes(&self.bytes);
        if !self.skip {
            self.out.push_str(&text);
        }
        self.bytes.clear();
    }

    fn push_char(&mut self, c: char) {
        self.flush();
        if !self.skip {
            self.out.push(c);
        }
    }
}

fn extract_text(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut ex = Extractor {
        out: String::with_capacity(raw.len() / 3),
        bytes: Vec::new(),
        stack: Vec::new(),
        skip: false,
        uc_skip: 0,
    };
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];

        // \uN 的 fallback 字符（通常是 ? 或 \'hh）直接跳过
        if ex.uc_skip > 0 {
            ex.uc_skip -= 1;
            if c == '\\' && i + 1 < chars.len() {
                if chars[i + 1] == '\'' {
                    i = (i + 4).min(chars.len());
                } else {
                    i += 2;
                }
            } else {
                i += 1;
            }
            continue;
        }

        match c {
            '{' => {
                ex.flush();
                ex.stack.push(ex.skip);
                i += 1;
            }
            '}' => {
                ex.flush();
                ex.skip = ex.stack.pop().unwrap_or(false);
                i += 1;
            }
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    break;
                }
                let next = chars[i];
                match next {
                    '\'' => {
                        // \'hh：代码页字节，先累积，遇到普通字符再统一解码
                        if i + 2 < chars.len() {
                            let hex: String = [chars[i + 1], chars[i + 2]].iter().collect();
                            if let Ok(b) = u8::from_str_radix(&hex, 16) {
                                ex.bytes.push(b);
                            }
                            i += 3;
                        } else {
                            i += 1;
                        }
                    }
                    '\\' | '{' | '}' => {
                        ex.push_char(next);
                        i += 1;
                    }
                    '*' => {
                        ex.flush();
                        ex.skip = true;
                        i += 1;
                    }
                    '~' => {
                        ex.push_char(' ');
                        i += 1;
                    }
                    '_' => {
                        ex.push_char('-');
                        i += 1;
                    }
                    '-' => i += 1,
                    '\n' | '\r' => i += 1,
                    'u' => {
                        i += 1;
                        let (num, next_i) = read_number_no_space(&chars, i);
                        i = next_i;
                        ex.flush();
                        let code = if num < 0 { num + 65536 } else { num };
                        if let Some(ch) = char::from_u32(code as u32) {
                            if !ex.skip {
                                ex.out.push(ch);
                            }
                        }
                        // 处理 \ucN 的 fallback：
                        // - "\u24352?" / "\u24352\'3f" 后面有一个占位字符，需要跳过；
                        // - "\u24352 " 后面的空格只是控制字分隔符，不能误跳下一个正文字符。
                        if i < chars.len() && chars[i] == ' ' {
                            i += 1;
                            ex.uc_skip = 0;
                        } else if i < chars.len() && (chars[i] == '?' || chars[i] == '\\') {
                            ex.uc_skip = 1;
                        } else {
                            ex.uc_skip = 0;
                        }
                    }
                    _ if next.is_ascii_alphabetic() => {
                        let start = i;
                        while i < chars.len() && chars[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word: String = chars[start..i].iter().collect();
                        let (param, next_i) = read_number(&chars, i);
                        i = next_i;
                        ex.flush();
                        match word.as_str() {
                            "par" | "line" | "sect" | "page" => {
                                if !ex.skip {
                                    ex.out.push('\n');
                                }
                            }
                            "tab" => {
                                if !ex.skip {
                                    ex.out.push('\t');
                                }
                            }
                            "cell" => {
                                if !ex.skip {
                                    ex.out.push('\t');
                                }
                            }
                            "row" => {
                                if !ex.skip {
                                    ex.out.push('\n');
                                }
                            }
                            "uc" => ex.uc_skip = param.max(0) as usize,
                            "bin" => {
                                // \binN：后面 N 个原始字节不可见
                                let skip_n = param.max(0) as usize;
                                i = (i + skip_n).min(chars.len());
                            }
                            w if is_destination(w) => ex.skip = true,
                            _ => {}
                        }
                    }
                    _ => {
                        ex.push_char(next);
                        i += 1;
                    }
                }
            }
            '\r' | '\n' => i += 1,
            _ => {
                ex.push_char(c);
                i += 1;
            }
        }
    }
    ex.flush();
    ex.out
}

fn read_number_no_space(chars: &[char], start: usize) -> (i32, usize) {
    let mut i = start;
    let mut sign = 1i32;
    if i < chars.len() && chars[i] == '-' {
        sign = -1;
        i += 1;
    }
    let mut value = 0i32;
    let mut any = false;
    while i < chars.len() && chars[i].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(chars[i] as i32 - '0' as i32);
        any = true;
        i += 1;
    }
    (if any { value * sign } else { 0 }, i)
}

fn read_number(chars: &[char], start: usize) -> (i32, usize) {
    let mut i = start;
    let mut sign = 1i32;
    if i < chars.len() && chars[i] == '-' {
        sign = -1;
        i += 1;
    }
    let mut value = 0i32;
    let mut any = false;
    while i < chars.len() && chars[i].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(chars[i] as i32 - '0' as i32);
        any = true;
        i += 1;
    }
    // 控制字后的一个空格是分隔符，不输出
    if i < chars.len() && chars[i] == ' ' {
        i += 1;
    }
    (if any { value * sign } else { 0 }, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_unicode_and_par() {
        let rtf = r"{\rtf1\ansi\ansicpg936 {\fonttbl{\f0 SimSun;}}\f0 \u24352?\u19977?\par 第二段}";
        let text = extract_text(rtf);
        assert!(text.contains("张三"), "实际：{text}");
        assert!(text.contains("第二段"));
        assert!(text.contains('\n'));
    }

    #[test]
    fn extract_gbk_escapes() {
        // “张三”的 GBK：D5 C5 C8 FD
        let rtf = r"{\rtf1\ansi\ansicpg936 \'d5\'c5\'c8\'fd\par 合同}";
        let text = extract_text(rtf);
        assert!(text.contains("张三"), "实际：{text}");
        assert!(text.contains("合同"));
    }

    #[test]
    fn skip_font_table_and_pictures() {
        let rtf = r"{\rtf1{\fonttbl{\f0 Arial;}}{\pict\pngblip 89504e47}正文}";
        let text = extract_text(rtf);
        assert_eq!(text.trim(), "正文");
    }
}
