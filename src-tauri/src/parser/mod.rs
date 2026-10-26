//! 文件解析层
//!
//! 职责：把 docx / PDF / TXT 统一解析成「全文 + 页范围 + 版面坐标」的 [`ParsedDoc`]，
//! 下游识别、脱敏、导出只依赖 `ParsedDoc`，与具体格式解耦。
//!
//! - PDF：`pdfium-render` 提供字符级坐标与页面渲染，`lopdf` 作为无 pdfium 时的文本兜底
//! - DOCX：zip + roxmltree 解析 `word/document.xml`，保留原始 XML 以便导出时定点改写
//! - TXT：自动识别 UTF-8 / UTF-16 / GB18030 编码

pub mod doc_binary;
pub mod docx;
pub mod html;
pub mod legacy;
pub mod ocr;
pub mod ofd;
pub mod pdf;
pub mod rtf;
pub mod txt;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::error::{AppError, Result};
use crate::models::{DocKind, ParsedDoc};

pub use ocr::OcrEngine;

/// OCR 引擎的惰性加载器：只有真的遇到扫描件时才调用。
///
/// PaddleOCR 模型首次使用需要读盘 + 图优化（数秒），如果每次导入都加载，
/// 会让纯文字文书也白白等待；因此这里把“加载”推迟到扫描页出现时。
pub type OcrProvider = Arc<dyn Fn() -> crate::error::Result<Arc<dyn OcrEngine>> + Send + Sync>;

/// OCR 页级进度（通过 Tauri 事件推给前端）
#[derive(Debug, Clone, Serialize)]
pub struct OcrProgress {
    /// 当前文件（仅文件名，避免把完整路径暴露到界面）
    pub file_name: String,
    /// 已处理页数
    pub current: usize,
    /// 本次需要 OCR 的总页数（混合型 PDF 只统计无文字层页面）
    pub total: usize,
    /// loading / page / done / error
    pub phase: String,
}

/// 进度回调；解析层不依赖 Tauri，由命令层注入
pub type ProgressFn = Arc<dyn Fn(OcrProgress) + Send + Sync>;

/// 解析选项
#[derive(Clone)]
pub struct ParseOptions {
    /// 已就绪的 OCR 引擎（测试或命令行直接注入时使用）
    pub ocr: Option<Arc<dyn OcrEngine>>,
    /// 惰性加载器（应用运行时使用）
    pub ocr_provider: Option<OcrProvider>,
    /// OCR 渲染 DPI
    pub ocr_dpi: u32,
    /// pdfium 动态库路径（应用层探测后传入；None 表示尝试系统库）
    pub pdfium_path: Option<PathBuf>,
    /// OCR 页级进度回调
    pub progress: Option<ProgressFn>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            ocr: None,
            ocr_provider: None,
            ocr_dpi: 200,
            pdfium_path: None,
            progress: None,
        }
    }
}

/// 解析结果
pub struct ParseOutput {
    pub doc: ParsedDoc,
    pub kind: DocKind,
    /// 是否检测到扫描件（需要 OCR 才能识别）
    pub needs_ocr: bool,
    /// 本次是否实际使用了 OCR
    pub ocr_used: bool,
    pub warnings: Vec<String>,
}

/// 按扩展名分发解析
pub fn parse_file(path: &Path, opts: &ParseOptions) -> Result<ParseOutput> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "pdf" => pdf::parse(path, opts),
        "docx" | "docm" => docx::parse(path),
        "txt" | "md" | "text" => txt::parse(path),
        "rtf" => rtf::parse(path),
        "html" | "htm" | "xhtml" => html::parse(path),
        "ofd" => ofd::parse(path),
        "doc" | "wps" => legacy::parse(path),
        "" => Err(AppError::Unsupported("文件没有扩展名，无法判断格式".into())),
        other => Err(AppError::Unsupported(format!(
            ".{other} 不在支持范围内，目前支持 docx/docm/doc/wps、PDF、RTF、OFD、HTML、TXT"
        ))),
    }
}

/// 安全截断：按字符数取前 n 个字符（用于日志摘要）
pub fn truncate_chars(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}
