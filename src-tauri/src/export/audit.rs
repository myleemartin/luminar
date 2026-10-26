//! 合规脱敏日志（卷宗留痕）
//!
//! 每份文档导出时同步生成 `.json`（结构化，便于归档检索）与 `.txt`（可直接打印附卷）。
//! 记录字段：文件名、处理时间、源文件指纹（SHA-256）、页码、原文片段、分类、
//! 脱敏方式、置信度、命中来源，满足《个人信息保护法》留痕与内部复核要求。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::error::{AppError, Result};
use crate::models::{Finding, ParsedDoc, RedactionSegment, APP_VERSION};

#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub index: usize,
    pub page: usize,
    pub category: String,
    pub category_label: String,
    pub original: String,
    pub replacement: String,
    pub mode: String,
    pub confidence: f32,
    pub source: String,
    pub rule_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    pub tool: String,
    pub version: String,
    pub generated_at: String,
    pub file_name: String,
    pub source_path: String,
    pub file_size: u64,
    pub sha256: String,
    pub page_count: usize,
    pub total: usize,
    pub exported_formats: Vec<String>,
    pub entries: Vec<AuditEntry>,
}

/// 由脱敏片段生成留痕条目
pub fn build_entries(
    doc: &ParsedDoc,
    findings: &[Finding],
    segments: &[RedactionSegment],
) -> Vec<AuditEntry> {
    let by_range: HashMap<(usize, usize), &Finding> =
        findings.iter().map(|f| ((f.start, f.end), f)).collect();
    let chars: Vec<char> = doc.text.chars().collect();

    segments
        .iter()
        .enumerate()
        .map(|(i, seg)| {
            let original: String = chars
                .get(seg.start..seg.end)
                .map(|s| s.iter().collect())
                .unwrap_or_default();
            let f = by_range.get(&(seg.start, seg.end));
            let page = doc
                .pages
                .iter()
                .find(|p| seg.start >= p.start && seg.start <= p.end)
                .map(|p| p.page)
                .unwrap_or(1);
            AuditEntry {
                index: i + 1,
                page,
                category: seg.category.clone(),
                category_label: seg.category_label.clone(),
                original: original.chars().take(200).collect(),
                replacement: seg.replacement.clone(),
                mode: seg.mode.clone(),
                confidence: f.map(|f| f.confidence).unwrap_or(1.0),
                source: f
                    .map(|f| f.source.label())
                    .unwrap_or("人工标记")
                    .to_string(),
                rule_name: f.map(|f| f.rule_name.clone()).unwrap_or_default(),
            }
        })
        .collect()
}

/// 生成报告对象
#[allow(clippy::too_many_arguments)]
pub fn build_report(
    file_name: &str,
    source_path: &Path,
    file_size: u64,
    sha256: &str,
    page_count: usize,
    exported_formats: Vec<String>,
    entries: Vec<AuditEntry>,
) -> AuditReport {
    AuditReport {
        tool: "星星脱敏".into(),
        version: APP_VERSION.into(),
        generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        file_name: file_name.to_string(),
        source_path: source_path.to_string_lossy().to_string(),
        file_size,
        sha256: sha256.to_string(),
        page_count,
        total: entries.len(),
        exported_formats,
        entries,
    }
}

/// 写出 JSON 日志
pub fn write_json(path: &Path, report: &AuditReport) -> Result<()> {
    let text = serde_json::to_string_pretty(report)
        .map_err(|e| AppError::Export(format!("日志序列化失败：{e}")))?;
    fs::write(path, text)?;
    Ok(())
}

/// 写出纯文本日志（便于打印、附卷）
pub fn write_text(path: &Path, report: &AuditReport) -> Result<()> {
    let mut s = String::new();
    s.push_str("══════════════════════════════════════════════════\n");
    s.push_str("              星星脱敏 · 脱敏留痕记录\n");
    s.push_str("══════════════════════════════════════════════════\n");
    s.push_str(&format!("文件名：{}\n", report.file_name));
    s.push_str(&format!("源文件：{}\n", report.source_path));
    s.push_str(&format!("文件大小：{} 字节\n", report.file_size));
    s.push_str(&format!("文件指纹（SHA-256）：{}\n", report.sha256));
    s.push_str(&format!("处理时间：{}\n", report.generated_at));
    s.push_str(&format!("处理工具：{} v{}\n", report.tool, report.version));
    s.push_str(&format!(
        "导出格式：{}\n",
        report.exported_formats.join("、")
    ));
    s.push_str(&format!("脱敏条目：{} 条\n", report.total));
    s.push_str("──────────────────────────────────────────────────\n");
    s.push_str("序号\t页码\t分类\t原文片段\t脱敏后\t方式\t置信度\t来源\n");
    for e in &report.entries {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\n",
            e.index,
            e.page,
            e.category_label,
            sanitize(&e.original),
            sanitize(&e.replacement),
            e.mode,
            e.confidence,
            e.source
        ));
    }
    s.push_str("──────────────────────────────────────────────────\n");
    s.push_str("说明：本记录由星星脱敏在本机离线生成，未上传任何数据。\n");
    fs::write(path, s)?;
    Ok(())
}

/// 日志单行化：把换行、制表符替换掉，避免破坏表格结构
fn sanitize(s: &str) -> String {
    s.replace(['\n', '\r', '\t'], " ").trim().to_string()
}
