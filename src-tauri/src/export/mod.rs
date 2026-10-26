//! 导出层门面：按用户选择的格式生成文件，并同步输出合规日志
//!
//! 单文档导出流程：解析结果 + 命中清单 → 脱敏文本 → 各格式落盘 → 留痕日志。

pub mod audit;
pub mod docx;
pub mod pdf;
pub mod txt;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};
use crate::models::{DocKind, ExportedFile, Finding, ParsedDoc};
use crate::redact::redact_text;

/// 一次导出任务的输入
pub struct ExportJob<'a> {
    pub doc: &'a ParsedDoc,
    pub findings: &'a [Finding],
    pub source_path: &'a Path,
    pub source_kind: DocKind,
    pub file_name: &'a str,
    pub file_size: u64,
    pub sha256: &'a str,
    pub out_dir: &'a Path,
    pub formats: &'a [String],
    pub pdfium_path: Option<PathBuf>,
    pub pdf_dpi: u32,
    pub write_audit: bool,
}

pub struct ExportResult {
    pub files: Vec<ExportedFile>,
    pub audit_files: Vec<ExportedFile>,
    pub warnings: Vec<String>,
    pub total: usize,
}

pub fn export(job: &ExportJob<'_>) -> Result<ExportResult> {
    // 导入后源文件可能被替换/修改，坐标与命中会失效；导出前重新校验指纹
    if job.sha256.len() == 64 {
        let actual = sha256_file(job.source_path)
            .ok_or_else(|| AppError::Export("无法读取源文件，可能已被移动或删除".into()))?;
        if actual != job.sha256 {
            return Err(AppError::Export(
                "源文件在导入后已被修改，为避免错位漏脱敏，已中止导出；请重新导入该文件".into(),
            ));
        }
    }

    fs::create_dir_all(job.out_dir)?;
    let redaction = redact_text(job.doc.text.as_str(), job.findings);
    let stem = sanitize(&file_stem(job.file_name));
    let stem = if stem.is_empty() {
        "文书".to_string()
    } else {
        stem
    };

    let mut files = Vec::new();
    let mut warnings = Vec::new();
    // 实际成功导出的格式（用于日志，不把失败的格式写进去）
    let mut done_formats: Vec<String> = Vec::new();

    for format in job.formats {
        match format.as_str() {
            "txt" => {
                let path = unique_path(job.out_dir, &format!("{stem}_脱敏"), "txt");
                txt::write_txt(&path, &redaction.text)?;
                files.push(exported(&path, "脱敏文本 TXT"));
                done_formats.push(format.clone());
            }
            "docx" => {
                let path = unique_path(job.out_dir, &format!("{stem}_脱敏"), "docx");
                // 只对真正的 .docx 保留原排版；.docm 可能带 VBA 宏，改为重新生成，
                // 避免把宏和原始元数据一起带进脱敏交付件。
                let preserve = job.source_kind == DocKind::Docx
                    && job
                        .source_path
                        .extension()
                        .map(|e| e.eq_ignore_ascii_case("docx"))
                        .unwrap_or(false);
                if preserve {
                    if let Err(e) =
                        docx::export_preserving(job.source_path, job.doc, job.findings, &path)
                    {
                        warnings.push(format!("保留原排版失败（{e}），已改为重新生成 Word"));
                        docx::export_generated(job.doc, job.findings, &path)?;
                    }
                } else {
                    docx::export_generated(job.doc, job.findings, &path)?;
                }
                files.push(exported(&path, "脱敏 Word"));
                done_formats.push(format.clone());
            }
            "pdf" => {
                let path = unique_path(job.out_dir, &format!("{stem}_脱敏"), "pdf");
                if job.source_kind.is_pdf() && !job.findings.iter().any(|f| f.enabled) {
                    // 没有任何命中：原样复制，避免因缺少坐标而误报失败
                    std::fs::copy(job.source_path, &path)?;
                    files.push(exported(&path, "脱敏 PDF（无命中，原样复制）"));
                    done_formats.push(format.clone());
                    continue;
                }
                match pdf::export(
                    job.source_path,
                    job.doc,
                    job.findings,
                    &path,
                    job.pdf_dpi,
                    job.pdfium_path.as_deref(),
                ) {
                    Ok(mut w) => {
                        warnings.append(&mut w);
                        files.push(exported(&path, "脱敏 PDF"));
                        done_formats.push(format.clone());
                    }
                    Err(e) => {
                        // PDF 失败不影响其它格式，返回警告由前端提示
                        warnings.push(format!("脱敏 PDF 生成失败：{e}"));
                    }
                }
            }
            other => warnings.push(format!("未知导出格式：{other}")),
        }
    }

    if files.is_empty() {
        return Err(AppError::Export(
            warnings
                .first()
                .cloned()
                .unwrap_or_else(|| "没有任何格式导出成功".into()),
        ));
    }

    let mut audit_files = Vec::new();
    if job.write_audit {
        let entries = audit::build_entries(job.doc, job.findings, &redaction.segments);
        let report = audit::build_report(
            job.file_name,
            job.source_path,
            job.file_size,
            job.sha256,
            job.doc.page_count,
            job.formats.to_vec(),
            entries,
        );
        let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let json_path = unique_path(job.out_dir, &format!("{stem}_脱敏日志_{stamp}"), "json");
        let txt_path = unique_path(job.out_dir, &format!("{stem}_脱敏日志_{stamp}"), "txt");
        match audit::write_json(&json_path, &report)
            .and_then(|_| audit::write_text(&txt_path, &report))
        {
            Ok(()) => {
                audit_files.push(exported(&json_path, "脱敏日志 JSON"));
                audit_files.push(exported(&txt_path, "脱敏日志 TXT"));
            }
            Err(e) => warnings.push(format!(
                "脱敏文件已生成，但合规日志写入失败：{e}（请检查目录权限）"
            )),
        }
    }

    Ok(ExportResult {
        files,
        audit_files,
        warnings,
        total: redaction.total(),
    })
}

fn exported(path: &Path, label: &str) -> ExportedFile {
    ExportedFile {
        path: path.to_string_lossy().to_string(),
        label: label.to_string(),
        bytes: fs::metadata(path).map(|m| m.len()).unwrap_or(0),
    }
}

fn file_stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string())
}

/// 去掉文件系统不允许的字符
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .trim_end_matches(['.', ' '])
        .to_string()
}

/// 同名文件自动追加序号，绝不覆盖律师已有文件
fn unique_path(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{base}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base}_{n}.{ext}"));
        n += 1;
        if n > 999 {
            // 极端情况下（同一目录上千个同名文件）用时间戳兜底，绝不覆盖已有文件
            let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S%.3f");
            candidate = dir.join(format!("{base}_{stamp}.{ext}"));
            break;
        }
    }
    candidate
}

/// 计算文件 SHA-256（导出前校验源文件是否被替换）
fn sha256_file(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ParsedDoc;

    #[test]
    fn refuses_export_when_source_changed() {
        let dir =
            std::env::temp_dir().join(format!("luminar_hash_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("a.txt");
        std::fs::write(&source, "hello").unwrap();
        let out = dir.join("out");
        let doc = ParsedDoc::from_pages(vec!["hello".to_string()]);
        let formats = vec!["txt".to_string()];
        let job = ExportJob {
            doc: &doc,
            findings: &[],
            source_path: &source,
            source_kind: DocKind::Txt,
            file_name: "a.txt",
            file_size: 5,
            sha256: &"0".repeat(64),
            out_dir: &out,
            formats: &formats,
            pdfium_path: None,
            pdf_dpi: 150,
            write_audit: false,
        };
        let err = match export(&job) {
            Ok(_) => panic!("源文件变化时应拒绝导出"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("已被修改"),
            "错误信息应说明原因：{err}"
        );
    }
}
