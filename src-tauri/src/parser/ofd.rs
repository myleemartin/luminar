//! OFD 解析：提取版式文档中的文字
//!
//! OFD 是国产版式文档标准（zip + XML），法院、税务、电子发票、部分律师函常见。
//! 这里读取 `Doc_*/Pages/Page_*/Content.xml` 中的 `<ofd:TextCode>`，按 Y/X 坐标
//! 重组成行；不做渲染，主要用于 TXT/Word 脱敏导出。
//!
//! 限制：纯矢量路径（曲线转文字）或扫描图片型 OFD 无法提取文字，会给出提示。

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use roxmltree::Document;
use zip::ZipArchive;

use crate::error::{AppError, Result};
use crate::models::{DocKind, ParsedDoc};
use crate::parser::ParseOutput;

const MAX_XML_SIZE: u64 = 100 * 1024 * 1024;

pub fn parse(path: &Path) -> Result<ParseOutput> {
    let file = File::open(path)?;
    let mut zip = ZipArchive::new(file)
        .map_err(|e| AppError::Other(format!("不是有效的 OFD（zip 容器打开失败）：{e}")))?;

    let mut pages: Vec<(usize, String)> = Vec::new();
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index(i) else { continue };
        let name = entry.name().to_string();
        if !name.starts_with("Doc_")
            || !name.contains("/Pages/Page_")
            || !name.ends_with("/Content.xml")
        {
            continue;
        }
        let page_no = extract_number(&name, "Page_").unwrap_or(i);
        pages.push((page_no, name));
    }
    if pages.is_empty() {
        return Err(AppError::Unsupported(
            "OFD 中没有找到页面内容（可能是不支持的 OFD 子格式）".into(),
        ));
    }
    pages.sort_by_key(|(n, _)| *n);

    let mut page_texts = Vec::new();
    let mut warnings = Vec::new();
    let mut total_chars = 0usize;
    for (idx, (_, name)) in pages.iter().enumerate() {
        let mut xml = String::new();
        let mut entry = zip
            .by_name(name)
            .map_err(|e| AppError::Other(format!("读取 OFD 页面失败：{e}")))?;
        if entry.size() > MAX_XML_SIZE {
            warnings.push(format!("第 {} 页 XML 超过 100MB，已跳过", idx + 1));
            page_texts.push(String::new());
            continue;
        }
        entry
            .read_to_string(&mut xml)
            .map_err(|e| AppError::Other(format!("读取 OFD 页面 XML 失败：{e}")))?;
        let text = extract_page_text(&xml, &mut warnings, idx + 1);
        total_chars += text.chars().filter(|c| !c.is_whitespace()).count();
        page_texts.push(text);
    }

    if total_chars == 0 {
        warnings.push(
            "OFD 中没有提取到文字：可能是扫描图片型 OFD 或纯矢量图形，需要先转为 PDF/图片再 OCR。"
                .to_string(),
        );
    }
    if page_texts.is_empty() {
        page_texts.push(String::new());
    }
    let doc = ParsedDoc::from_pages(page_texts);
    Ok(ParseOutput {
        doc,
        kind: DocKind::Ofd,
        needs_ocr: false,
        ocr_used: false,
        warnings,
    })
}

/// 按 TextCode 的 X/Y 坐标重组成行；没有坐标时按文档顺序输出
fn extract_page_text(xml: &str, warnings: &mut Vec<String>, page_no: usize) -> String {
    let doc = match Document::parse(xml) {
        Ok(d) => d,
        Err(e) => {
            warnings.push(format!("第 {page_no} 页 OFD XML 解析失败：{e}"));
            return String::new();
        }
    };

    #[derive(Default)]
    struct Item {
        x: f32,
        y: f32,
        text: String,
        has_pos: bool,
    }
    let mut items: Vec<Item> = Vec::new();
    for node in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "TextCode")
    {
        let text: String = node.text().unwrap_or("").trim().to_string();
        if text.is_empty() {
            continue;
        }
        let x = node
            .attribute("X")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0);
        let y = node
            .attribute("Y")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0);
        items.push(Item {
            x,
            y,
            text,
            has_pos: node.attribute("X").is_some() && node.attribute("Y").is_some(),
        });
    }
    if items.is_empty() {
        // 有些 OFD 用 TextObject 包裹文字，尝试直接收集 TextCode 以外的文本节点
        let fallback: String = doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "TextCode")
            .filter_map(|n| n.text())
            .collect::<Vec<_>>()
            .join("");
        return fallback;
    }

    if !items.iter().any(|i| i.has_pos) {
        return items
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }

    // 按 Y 分行（容差 1.0），行内按 X 排序；Y 坐标向下增大
    let mut lines: BTreeMap<i64, Vec<&Item>> = BTreeMap::new();
    for item in &items {
        let key = (item.y * 2.0).round() as i64; // 0.5 单位容差
        lines.entry(key).or_default().push(item);
    }
    let mut out = Vec::new();
    for (_, mut line) in lines {
        line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let text: String = line.iter().map(|i| i.text.as_str()).collect();
        if !text.trim().is_empty() {
            out.push(text);
        }
    }
    out.join("\n")
}

fn extract_number(name: &str, marker: &str) -> Option<usize> {
    let pos = name.rfind(marker)? + marker.len();
    let digits: String = name[pos..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn extract_ofd_text() {
        let dir =
            std::env::temp_dir().join(format!("luminar_ofd_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.ofd");
        {
            let file = File::create(&path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zip.start_file("OFD.xml", opts).unwrap();
            zip.write_all(
                r#"<?xml version="1.0"?><ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016"/>"#
                    .as_bytes(),
            )
            .unwrap();
            zip.start_file("Doc_0/Pages/Page_0/Content.xml", opts)
                .unwrap();
            zip.write_all(
                r#"<?xml version="1.0"?>
                <ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
                  <ofd:Content>
                    <ofd:TextCode X="10" Y="10">原告：</ofd:TextCode>
                    <ofd:TextCode X="40" Y="10">张三</ofd:TextCode>
                    <ofd:TextCode X="10" Y="30">被告：李四</ofd:TextCode>
                  </ofd:Content>
                </ofd:Page>"#
                    .as_bytes(),
            )
            .unwrap();
            zip.finish().unwrap();
        }
        let out = parse(&path).expect("解析 OFD 失败");
        assert!(out.doc.text.contains("张三"), "实际：{}", out.doc.text);
        assert!(out.doc.text.contains("被告：李四"));
        assert_eq!(out.kind, DocKind::Ofd);
    }
}
