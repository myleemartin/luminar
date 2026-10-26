//! HTML / HTM 解析：去掉标签、脚本、样式，提取正文
//!
//! 法院文书、邮件导出、网页归档常见 `.html`；这里做纯文本提取，保留段落换行，
//! 不解码 CSS/JavaScript，也不访问外部资源。

use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::models::{DocKind, ParsedDoc};
use crate::parser::txt::{decode_bytes, normalize_newlines};
use crate::parser::ParseOutput;

const CHARS_PER_PAGE: usize = 3500;

pub fn parse(path: &Path) -> Result<ParseOutput> {
    let bytes = fs::read(path)?;
    let raw = decode_bytes(&bytes);
    let text = normalize_newlines(&html_to_text(&raw));
    let paragraphs: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    let doc = ParsedDoc::from_blocks(paragraphs, CHARS_PER_PAGE);
    let mut warnings = Vec::new();
    if doc.text.trim().is_empty() {
        warnings.push("HTML 中没有提取到正文文字。".to_string());
    }
    Ok(ParseOutput {
        doc,
        kind: DocKind::Html,
        needs_ocr: false,
        ocr_used: false,
        warnings,
    })
}

fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "br"
            | "li"
            | "ul"
            | "ol"
            | "tr"
            | "table"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "pre"
            | "hr"
    )
}

pub fn html_to_text(html: &str) -> String {
    let chars: Vec<char> = html.chars().collect();
    let mut out = String::with_capacity(html.len() / 2);
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '<' {
            // 注释
            if i + 3 < chars.len()
                && chars[i + 1] == '!'
                && chars[i + 2] == '-'
                && chars[i + 3] == '-'
            {
                if let Some(end) = find_seq(&chars, i + 4, &['-', '-', '>']) {
                    i = end + 3;
                    continue;
                }
            }
            // 标签
            if let Some(gt) = chars[i + 1..].iter().position(|c| *c == '>') {
                let end = i + 1 + gt;
                let inner: String = chars[i + 1..end].iter().collect();
                let trimmed = inner
                    .trim()
                    .trim_start_matches('/')
                    .trim_end_matches('/')
                    .to_ascii_lowercase();
                let tag = trimmed.split_whitespace().next().unwrap_or("");
                // script/style 内容整段跳过
                if tag == "script" || tag == "style" {
                    let close = format!("</{tag}");
                    if let Some(pos) = find_case_insensitive(&chars, end + 1, &close) {
                        if let Some(gt2) = chars[pos..].iter().position(|c| *c == '>') {
                            i = pos + gt2 + 1;
                            continue;
                        }
                    }
                    i = end + 1;
                    continue;
                }
                if is_block_tag(tag) && !out.ends_with('\n') {
                    out.push('\n');
                }
                if tag == "td" || tag == "th" {
                    out.push('\t');
                }
                i = end + 1;
                continue;
            }
            out.push('<');
            i += 1;
            continue;
        }
        if chars[i] == '&' {
            if let Some(semi) = chars[i + 1..].iter().take(12).position(|c| *c == ';') {
                let end = i + 1 + semi;
                let entity: String = chars[i + 1..end].iter().collect();
                if let Some(ch) = decode_entity(&entity) {
                    out.push(ch);
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    // 合并多余空行
    let mut collapsed = String::with_capacity(out.len());
    let mut newlines = 0usize;
    for c in out.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                collapsed.push(c);
            }
        } else {
            newlines = 0;
            collapsed.push(c);
        }
    }
    collapsed.trim().to_string()
}

fn find_seq(chars: &[char], from: usize, seq: &[char]) -> Option<usize> {
    if seq.is_empty() || from + seq.len() > chars.len() {
        return None;
    }
    (from..=chars.len() - seq.len()).find(|&i| chars[i..i + seq.len()] == *seq)
}

fn find_case_insensitive(chars: &[char], from: usize, needle: &str) -> Option<usize> {
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() || from + needle.len() > chars.len() {
        return None;
    }
    (from..=chars.len() - needle.len()).find(|&i| {
        chars[i..i + needle.len()]
            .iter()
            .zip(&needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

fn decode_entity(entity: &str) -> Option<char> {
    if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    if let Some(dec) = entity.strip_prefix('#') {
        return dec.parse::<u32>().ok().and_then(char::from_u32);
    }
    Some(match entity {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{00a0}',
        "copy" => '©',
        "reg" => '®',
        "hellip" => '…',
        "mdash" => '—',
        "ndash" => '–',
        "ldquo" => '“',
        "rdquo" => '”',
        "lsquo" => '‘',
        "rsquo" => '’',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tags_and_entities() {
        let html = r#"<html><head><style>p{color:red}</style></head>
        <body><h1>民事起诉状</h1><p>原告：张三&amp;李四</p><script>alert(1)</script>
        <table><tr><td>身份证号</td><td>420102198801011234</td></tr></table></body></html>"#;
        let text = html_to_text(html);
        assert!(text.contains("民事起诉状"));
        assert!(text.contains("原告：张三&李四"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
        assert!(text.contains("身份证号"));
    }
}
