//! Word（.docx）解析
//!
//! 采用「zip 解包 + roxmltree 只读解析」：
//! - 读取 `word/document.xml`，按 `<w:p>` 段落抽取 `<w:t>` 文本
//! - 记录每个 `<w:t>` 在 XML 中的字节区间，导出时可「定点改写」，
//!   从而保留原文档的段落属性（pPr）、表格、页眉页脚、图片等排版
//!
//! 限制：被脱敏的段落会合并为第一个 run 的字体格式（不影响段落级排版）。

use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::Path;

use roxmltree::Document;

use crate::error::{AppError, Result};
use crate::models::{ParsedDoc, TextBlock};
use crate::parser::ParseOutput;

/// 段落内一个 `<w:t>` 节点
#[derive(Debug, Clone)]
pub struct TextNode {
    /// 该节点在 document.xml 中的完整字节区间（含起止标签）
    pub range: Range<usize>,
    pub text: String,
}

/// 段落信息
#[derive(Debug, Clone)]
pub struct DocxParagraph {
    pub text: String,
    pub nodes: Vec<TextNode>,
    /// 段落样式 id，如 Heading1
    pub style: String,
}

/// TXT / DOCX 的虚拟分页大小
const CHARS_PER_PAGE: usize = 3000;

pub fn parse(path: &Path) -> Result<ParseOutput> {
    let xml = read_document_xml(path)?;
    let paragraphs = extract_paragraphs(&xml)?;

    let texts: Vec<String> = paragraphs.iter().map(|p| p.text.clone()).collect();
    let mut doc = ParsedDoc::from_blocks(texts, CHARS_PER_PAGE);

    // 依据 Word 段落样式标记标题，供导出时保留层级
    for (i, p) in paragraphs.iter().enumerate() {
        if let Some(block) = doc.blocks.get_mut(i) {
            if is_heading(&p.style) {
                block.kind = "heading".into();
            }
        }
    }

    // 空文档保护：全空说明可能是损坏或仅有图片的文档
    let mut warnings = Vec::new();
    for part in extra_text_parts(path) {
        warnings.push(format!(
            "文档的 {part} 中也有文字，当前仅识别正文；如含敏感信息请另行处理或转为 PDF"
        ));
    }
    if doc.text.trim().is_empty() {
        warnings
            .push("文档中没有可提取的文字（可能整篇为图片，请另存为 PDF 后按扫描件处理）".into());
    }

    Ok(ParseOutput {
        doc,
        kind: crate::models::DocKind::Docx,
        needs_ocr: false,
        ocr_used: false,
        warnings,
    })
}

fn is_heading(style: &str) -> bool {
    let s = style.to_ascii_lowercase();
    s.starts_with("heading") || style.starts_with("标题") || s == "title"
}

/// 读取 docx 内的 `word/document.xml`
pub fn read_document_xml(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Docx(format!("无法打开 docx（可能已加密或损坏）：{e}")))?;
    let mut entry = zip
        .by_name("word/document.xml")
        .map_err(|_| AppError::Docx("docx 结构异常：缺少 word/document.xml".into()))?;
    // 防止恶意/异常文档解压出超大 XML 导致内存耗尽
    if entry.size() > 200 * 1024 * 1024 {
        return Err(AppError::Docx(
            "word/document.xml 超过 200MB，拒绝解析".into(),
        ));
    }
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|e| AppError::Docx(format!("读取 word/document.xml 失败：{e}")))?;
    Ok(xml)
}

/// 解析全部段落（文档顺序），含每个 `<w:t>` 的字节区间
pub fn extract_paragraphs(xml: &str) -> Result<Vec<DocxParagraph>> {
    let doc = Document::parse(xml).map_err(|e| AppError::Docx(format!("XML 解析失败：{e}")))?;
    let mut out = Vec::new();

    for p in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "p")
    {
        let mut nodes = Vec::new();
        for t in p
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "t")
        {
            // 只取直接的文本内容（w:t 内一般只有一个文本节点）
            let text: String = t
                .children()
                .filter(|c| c.is_text())
                .filter_map(|c| c.text())
                .collect();
            if text.is_empty() {
                continue;
            }
            nodes.push(TextNode {
                range: t.range(),
                text,
            });
        }

        // 段落样式
        let style = p
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "pPr")
            .and_then(|p_pr| {
                p_pr.children()
                    .find(|n| n.is_element() && n.tag_name().name() == "pStyle")
            })
            .and_then(|s| s.attribute("val"))
            .unwrap_or("")
            .to_string();

        out.push(DocxParagraph {
            text: nodes.iter().map(|n| n.text.as_str()).collect(),
            nodes,
            style,
        });
    }

    Ok(out)
}

/// 检查页眉/页脚/脚注中是否含文字（这些部件当前不参与脱敏，只做提示）
fn extra_text_parts(path: &Path) -> Vec<String> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else {
            continue;
        };
        let name = entry.name().to_string();
        let is_extra = name.starts_with("word/header")
            || name.starts_with("word/footer")
            || name == "word/footnotes.xml"
            || name == "word/endnotes.xml";
        if !is_extra || !name.ends_with(".xml") {
            continue;
        }
        if entry.size() > 20 * 1024 * 1024 {
            continue;
        }
        let mut xml = String::new();
        if entry.read_to_string(&mut xml).is_err() {
            continue;
        }
        let Ok(doc) = Document::parse(&xml) else {
            continue;
        };
        let text: String = doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "t")
            .filter_map(|t| t.text())
            .collect();
        // 只有页码/符号的页眉不提示，出现姓名、案号等文字才提醒
        if text.chars().any(|c| c.is_alphabetic()) {
            found.push(name);
        }
    }
    found
}

/// 供导出层使用：把 ParsedDoc 的块与段落一一对应
pub fn paragraph_texts(xml: &str) -> Result<Vec<String>> {
    Ok(extract_paragraphs(xml)?
        .into_iter()
        .map(|p| p.text)
        .collect())
}

/// 过滤出真正的正文块（用于调试）
#[allow(dead_code)]
pub fn non_empty_blocks(doc: &ParsedDoc) -> Vec<&TextBlock> {
    doc.blocks.iter().filter(|b| b.end > b.start).collect()
}
