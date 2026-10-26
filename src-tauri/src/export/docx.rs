//! DOCX 导出
//!
//! 两种路径：
//! 1. **保留排版**（源文件是 docx）：重新解析 `word/document.xml`，只改写受影响的
//!    `<w:t>` 文本节点，段落属性、表格、图片、页眉页脚、样式表全部原样保留。
//! 2. **重新生成**（源文件是 PDF/TXT）：用 docx-rs 生成一个干净的新文档。

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use docx_rs::{Docx, Paragraph, Run};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{AppError, Result};
use crate::models::{Finding, ParsedDoc};
use crate::parser::docx as docx_parser;
use crate::redact::redact_range;

/// 保留原始排版的导出
pub fn export_preserving(
    source: &Path,
    doc: &ParsedDoc,
    findings: &[Finding],
    out: &Path,
) -> Result<()> {
    let xml = docx_parser::read_document_xml(source)?;
    let paragraphs = docx_parser::extract_paragraphs(&xml)?;

    if paragraphs.len() != doc.blocks.len() {
        return Err(AppError::Export(format!(
            "段落数不一致（{} vs {}），已改用重新生成方式",
            paragraphs.len(),
            doc.blocks.len()
        )));
    }

    let mut numbering = HashMap::new();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();

    for (i, para) in paragraphs.iter().enumerate() {
        if para.nodes.is_empty() {
            continue;
        }
        let block = &doc.blocks[i];
        // 段落纯文本与块文本必须一致；若文档在导入后被改动过，
        // 定点改写会错位，直接报错让上层回退为“重新生成 Word”（安全但丢格式）。
        if para.text != paragraph_text(doc, block.start, block.end) {
            return Err(AppError::Export(
                "原文档在导入后发生变化，无法按原排版定点改写".into(),
            ));
        }
        let red = redact_range(&doc.text, findings, block.start, block.end, &mut numbering);
        if red.text == para.text {
            continue;
        }
        // 第一个 <w:t> 承载整段脱敏文本，其余清空；
        // 段落属性 pPr 不动，因此对齐、缩进、编号、字号全部保留。
        edits.push((
            para.nodes[0].range.clone(),
            format!(
                "<w:t xml:space=\"preserve\">{}</w:t>",
                escape_xml(&red.text)
            ),
        ));
        for n in &para.nodes[1..] {
            edits.push((
                n.range.clone(),
                "<w:t xml:space=\"preserve\"></w:t>".to_string(),
            ));
        }
    }

    // 从后往前替换，避免字节区间位移
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut new_xml = xml;
    for (range, replacement) in edits {
        if range.end <= new_xml.len() {
            new_xml.replace_range(range, &replacement);
        }
    }

    rewrite_zip(source, out, &new_xml)
}

fn paragraph_text(doc: &ParsedDoc, start: usize, end: usize) -> String {
    doc.text
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

/// 用新的 document.xml 重打包 docx，其余部件逐字节复制
fn rewrite_zip(source: &Path, out: &Path, new_document_xml: &str) -> Result<()> {
    let file = File::open(source)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| AppError::Export(format!("打开源 docx 失败：{e}")))?;
    let target = File::create(out)?;
    let mut writer = ZipWriter::new(target);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::Export(format!("读取 docx 条目失败：{e}")))?;
        let name = entry.name().to_string();
        if entry.is_dir() {
            writer
                .add_directory(name, options)
                .map_err(|e| AppError::Export(format!("写入目录失败：{e}")))?;
            continue;
        }
        writer
            .start_file(name.clone(), options)
            .map_err(|e| AppError::Export(format!("写入条目失败：{e}")))?;
        if name == "word/document.xml" {
            writer.write_all(new_document_xml.as_bytes())?;
        } else {
            std::io::copy(&mut entry, &mut writer)?;
        }
    }
    writer
        .finish()
        .map_err(|e| AppError::Export(format!("打包 docx 失败：{e}")))?;
    Ok(())
}

/// 重新生成 docx（PDF/TXT 源）
pub fn export_generated(doc: &ParsedDoc, findings: &[Finding], out: &Path) -> Result<()> {
    let red = crate::redact::redact_text(&doc.text, findings);
    let mut docx = Docx::new();
    for line in red.text.split('\n') {
        docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(line)));
    }
    let file = File::create(out)?;
    let mut file = file;
    docx.build()
        .pack(&mut file)
        .map_err(|e| AppError::Export(format!("生成 docx 失败：{e}")))?;
    file.flush()?;
    Ok(())
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}
