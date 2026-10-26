//! PDF 解析
//!
//! 两条路径：
//! 1. **pdfium**（首选）：逐页取字符级坐标，既能判定文字版/扫描版，也为导出脱敏 PDF
//!    提供精确定位；扫描页按 DPI 渲染后交给 OCR 引擎。
//! 2. **lopdf**（兜底）：pdfium 动态库缺失时仍可抽取文本，保证 TXT/Word 导出可用。
//!
//! PDF 文字层坐标单位为「点」（1/72 英寸），左上角原点，已在解析时完成 y 轴翻转。

use std::path::Path;

use image::RgbImage;
use pdfium_render::prelude::*;

use crate::error::{AppError, Result};
use crate::models::{CharBox, DocKind, LineBox, PageLayout, ParsedDoc};
use crate::parser::{OcrProgress, ParseOptions, ParseOutput};

/// 平均每页有效字符少于该值，判定为扫描件
const SCAN_CHARS_PER_PAGE: usize = 20;

/// 渲染后的页面
pub struct RenderedPage {
    pub image: RgbImage,
    pub width_pt: f32,
    pub height_pt: f32,
}

/// 绑定 pdfium 动态库：优先使用显式路径，其次尝试系统库
pub fn bind_pdfium(explicit: Option<&Path>) -> std::result::Result<Pdfium, String> {
    if let Some(p) = explicit {
        if p.exists() {
            match Pdfium::bind_to_library(p) {
                Ok(b) => return Ok(Pdfium::new(b)),
                Err(e) => eprintln!("[PDF] 加载指定 pdfium 失败 {}：{e}", p.display()),
            }
        }
    }
    match Pdfium::bind_to_system_library() {
        Ok(b) => Ok(Pdfium::new(b)),
        Err(e) => Err(format!("{e}")),
    }
}

/// pdfium 是否可用（用于设置面板提示）
pub fn pdfium_available(explicit: Option<&Path>) -> bool {
    bind_pdfium(explicit).is_ok()
}

pub fn parse(path: &Path, opts: &ParseOptions) -> Result<ParseOutput> {
    match parse_with_pdfium(path, opts) {
        Ok(out) => Ok(out),
        Err(err) => {
            let mut out = parse_with_lopdf(path)?;
            out.warnings.push(format!(
                "pdfium 未就绪（{err}）。已退化为纯文本抽取，扫描件无法 OCR、脱敏 PDF 无法导出。\
                 请将 libpdfium 放入应用资源目录，或在设置中指定路径。"
            ));
            Ok(out)
        }
    }
}

fn parse_with_pdfium(path: &Path, opts: &ParseOptions) -> Result<ParseOutput> {
    let pdfium = bind_pdfium(opts.pdfium_path.as_deref()).map_err(AppError::Pdf)?;
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| AppError::Pdf(format!("pdfium 无法打开 PDF：{e}")))?;

    let page_count = doc.pages().len() as usize;
    if page_count == 0 {
        return Err(AppError::Pdf("PDF 中没有页面".into()));
    }

    let mut warnings = Vec::new();
    let mut page_texts: Vec<String> = Vec::with_capacity(page_count);
    let mut layouts: Vec<PageLayout> = Vec::with_capacity(page_count);

    for i in 0..page_count {
        let mut layout = PageLayout {
            page: i + 1,
            ..Default::default()
        };
        let page = match doc.pages().get(i as u16) {
            Ok(p) => p,
            Err(e) => {
                warnings.push(format!("第 {} 页读取失败：{e}", i + 1));
                page_texts.push(String::new());
                layouts.push(layout);
                continue;
            }
        };
        let page_w = page.width().value;
        let page_h = page.height().value;
        layout.width = page_w;
        layout.height = page_h;

        let mut text = String::new();
        let mut chars: Vec<CharBox> = Vec::new();
        match page.text() {
            Ok(text_page) => {
                let mut last: Option<CharBox> = None;
                for c in text_page.chars().iter() {
                    let ch = match c.unicode_char() {
                        Some(ch) => ch,
                        None => continue,
                    };
                    if ch == '\r' || ch == '\u{0}' {
                        continue;
                    }
                    let rect = c.tight_bounds().or_else(|_| c.loose_bounds()).ok();
                    let cb = rect.map(|r| CharBox {
                        ch,
                        x: r.left().value,
                        y: page_h - r.top().value,
                        w: (r.right().value - r.left().value).max(0.0),
                        h: (r.top().value - r.bottom().value).max(0.0),
                    });

                    if ch == '\n' {
                        if !text.ends_with('\n') {
                            text.push('\n');
                            chars.push(CharBox {
                                ch: '\n',
                                x: 0.0,
                                y: 0.0,
                                w: 0.0,
                                h: 0.0,
                            });
                        }
                        last = None;
                        continue;
                    }

                    // 依据坐标补换行：仅在「回到行首 + 明显掉到下一行」时触发，
                    // 避免汉字字形高度差异导致把同一行切碎（会破坏手机号、住址等连续信息）。
                    if let (Some(prev), Some(cur)) = (last, cb) {
                        let moved_next_line =
                            cur.x + prev.w * 0.5 < prev.x && cur.y - prev.y > prev.h * 0.5;
                        if prev.h > 0.0 && moved_next_line && !text.ends_with('\n') {
                            text.push('\n');
                            chars.push(CharBox {
                                ch: '\n',
                                x: cur.x,
                                y: cur.y,
                                w: 0.0,
                                h: 0.0,
                            });
                        }
                    }

                    text.push(ch);
                    chars.push(cb.unwrap_or(CharBox {
                        ch,
                        x: 0.0,
                        y: 0.0,
                        w: 0.0,
                        h: 0.0,
                    }));
                    last = cb;
                }
            }
            Err(e) => warnings.push(format!("第 {} 页文字层读取失败：{e}", i + 1)),
        }

        // 去掉尾部空行，保持文本与字符坐标一一对应
        while text.ends_with('\n') {
            text.pop();
            chars.pop();
        }

        layout.chars = chars;
        page_texts.push(text);
        layouts.push(layout);
    }

    let page_chars: Vec<usize> = page_texts
        .iter()
        .map(|t| t.chars().filter(|c| !c.is_whitespace()).count())
        .collect();
    let total_chars: usize = page_chars.iter().sum();
    let needs_ocr = total_chars < page_count * SCAN_CHARS_PER_PAGE;
    // 混合型 PDF：整体有文字层，但夹着扫描页（常见于判决书 + 证据扫描件）
    let sparse_pages: Vec<usize> = page_chars
        .iter()
        .enumerate()
        .filter(|(_, n)| **n < 5)
        .map(|(i, _)| i)
        .collect();
    let ocr_targets: Vec<usize> = if needs_ocr {
        (0..page_count).collect()
    } else {
        sparse_pages.clone()
    };

    let mut ocr_used = false;
    if !ocr_targets.is_empty() {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let total_pages = ocr_targets.len();
        let report = |current: usize, phase: &str| {
            if let Some(cb) = &opts.progress {
                cb(OcrProgress {
                    file_name: file_name.clone(),
                    current,
                    total: total_pages,
                    phase: phase.to_string(),
                });
            }
        };
        report(0, "loading");

        // 扫描件：此时才加载 OCR 引擎（模型编译较慢，避免拖慢普通文档导入）
        let mut ocr_error: Option<String> = None;
        let engine = match opts.ocr.clone() {
            Some(engine) => Some(engine),
            None => match &opts.ocr_provider {
                Some(load) => match load() {
                    Ok(engine) => Some(engine),
                    Err(err) => {
                        ocr_error = Some(err.to_string());
                        None
                    }
                },
                None => None,
            },
        };

        match engine {
            Some(engine) if engine.is_ready() => {
                let mut done = 0usize;
                for i in ocr_targets.iter().copied() {
                    done += 1;
                    let page = match doc.pages().get(i as u16) {
                        Ok(p) => p,
                        Err(e) => {
                            warnings.push(format!("第 {} 页渲染失败：{e}", i + 1));
                            report(done, "page");
                            continue;
                        }
                    };
                    match render_page(&page, opts.ocr_dpi) {
                        Ok(image) => match engine.recognize(&image) {
                            Ok(result) => {
                                let layout = &mut layouts[i];
                                let sx = if result.width > 0 {
                                    layout.width / result.width as f32
                                } else {
                                    1.0
                                };
                                let sy = if result.height > 0 {
                                    layout.height / result.height as f32
                                } else {
                                    1.0
                                };
                                let mut text = String::new();
                                let mut lines = Vec::new();
                                for line in &result.lines {
                                    if !text.is_empty() {
                                        text.push('\n');
                                    }
                                    // OCR 常把数字识别成全角（２０２４），统一转半角后再交给规则引擎
                                    let normalized = normalize_ocr_text(&line.text);
                                    text.push_str(&normalized);
                                    lines.push(LineBox {
                                        text: normalized,
                                        x: line.x * sx,
                                        y: line.y * sy,
                                        w: line.w * sx,
                                        h: line.h * sy,
                                    });
                                }
                                page_texts[i] = text;
                                layout.lines = lines;
                                ocr_used = true;
                            }
                            Err(e) => warnings.push(format!("第 {} 页 OCR 失败：{e}", i + 1)),
                        },
                        Err(e) => warnings.push(format!("第 {} 页渲染失败：{e}", i + 1)),
                    }
                    report(done, "page");
                }
                report(total_pages, "done");
                if ocr_used {
                    let recognized: usize = page_texts
                        .iter()
                        .map(|t| t.chars().filter(|c| !c.is_whitespace()).count())
                        .sum();
                    if recognized == 0 {
                        warnings.push(
                            "已完成 OCR，但没有识别出文字：可能是空白页、图片过于模糊或模型不适配。"
                                .into(),
                        );
                    } else if needs_ocr {
                        warnings.push(format!(
                            "检测到扫描件，已使用「{}」完成 {} 页 OCR（本地离线）。",
                            engine.label(),
                            ocr_targets.len()
                        ));
                    } else {
                        warnings.push(format!(
                            "检测到 {} 页无文字层（混合型 PDF），已使用「{}」完成 OCR。",
                            ocr_targets.len(),
                            engine.label()
                        ));
                    }
                }
            }
            Some(engine) => {
                report(0, "error");
                warnings.push(format!(
                    "检测到 {} 页需要 OCR，但 OCR 引擎不可用：{}",
                    ocr_targets.len(),
                    engine.message()
                ));
            }
            None => {
                report(0, "error");
                if let Some(err) = ocr_error {
                    warnings.push(format!("部分页面需要 OCR，但引擎加载失败：{err}"));
                } else if needs_ocr {
                    warnings.push(
                        "检测到扫描件（无文字层），但未配置 OCR，无法提取文字。请安装 OCR 模型后重新导入。"
                            .into(),
                    );
                } else {
                    warnings.push(format!(
                        "有 {} 页没有文字层（混合型 PDF），但未配置 OCR，这些页面无法提取文字。",
                        ocr_targets.len()
                    ));
                }
            }
        }
    }

    let mut parsed = ParsedDoc::from_pages(page_texts);
    parsed.layouts = layouts;

    Ok(ParseOutput {
        doc: parsed,
        kind: if needs_ocr {
            DocKind::PdfScan
        } else {
            DocKind::PdfText
        },
        needs_ocr: needs_ocr || !sparse_pages.is_empty(),
        ocr_used,
        warnings,
    })
}

/// lopdf 兜底：仅文本，无坐标
fn parse_with_lopdf(path: &Path) -> Result<ParseOutput> {
    let doc = lopdf::Document::load(path)
        .map_err(|e| AppError::Pdf(format!("lopdf 无法打开 PDF：{e}")))?;
    let mut page_numbers: Vec<u32> = doc.get_pages().keys().copied().collect();
    page_numbers.sort_unstable();
    if page_numbers.is_empty() {
        return Err(AppError::Pdf("PDF 中没有页面".into()));
    }

    let mut page_texts = Vec::with_capacity(page_numbers.len());
    let mut warnings = Vec::new();
    for (i, chunk) in doc
        .extract_text_chunks(&page_numbers)
        .into_iter()
        .enumerate()
    {
        match chunk {
            Ok(t) => page_texts.push(crate::parser::txt::normalize_newlines(&t)),
            Err(e) => {
                warnings.push(format!("第 {} 页文本抽取失败：{e}", i + 1));
                page_texts.push(String::new());
            }
        }
    }

    let page_count = page_texts.len();
    let total_chars: usize = page_texts
        .iter()
        .map(|t| t.chars().filter(|c| !c.is_whitespace()).count())
        .sum();
    let needs_ocr = total_chars < page_count * SCAN_CHARS_PER_PAGE;
    if needs_ocr {
        warnings.push("该 PDF 疑似扫描件且 pdfium 不可用，无法进行 OCR。".into());
    }

    let parsed = ParsedDoc::from_pages(page_texts);
    Ok(ParseOutput {
        doc: parsed,
        kind: if needs_ocr {
            DocKind::PdfScan
        } else {
            DocKind::PdfText
        },
        needs_ocr,
        ocr_used: false,
        warnings,
    })
}

/// OCR 文本归一化：全角英数字/标点 → 半角（长度保持不变，坐标不偏移）
fn normalize_ocr_text(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
            '\u{FF01}'..='\u{FF5E}' => {
                let code = c as u32 - 0xFEE0;
                char::from_u32(code).unwrap_or(c)
            }
            _ => c,
        })
        .collect()
}

pub(crate) fn render_page(page: &PdfPage<'_>, dpi: u32) -> Result<RgbImage> {
    let scale = (dpi.max(72) as f32) / 72.0;
    let config = PdfRenderConfig::new()
        .scale_page_by_factor(scale)
        // 避免 A0 图纸/超长扫描件在 150~200 DPI 下生成上亿像素导致 OOM
        .set_maximum_width(6000)
        .set_maximum_height(6000)
        .render_form_data(false)
        .render_annotations(true);
    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| AppError::Pdf(format!("页面渲染失败：{e}")))?;
    Ok(bitmap.as_image().to_rgb8())
}

/// 一次性渲染全部页面（导出脱敏 PDF 使用，避免重复加载文档）
pub fn render_all_pages(
    path: &Path,
    dpi: u32,
    pdfium_path: Option<&Path>,
) -> Result<Vec<RenderedPage>> {
    let pdfium = bind_pdfium(pdfium_path).map_err(AppError::Pdf)?;
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| AppError::Pdf(format!("pdfium 无法打开 PDF：{e}")))?;
    let page_count = doc.pages().len() as usize;
    let mut out = Vec::with_capacity(page_count);
    for i in 0..page_count {
        let page = doc
            .pages()
            .get(i as u16)
            .map_err(|e| AppError::Pdf(format!("第 {} 页不可读：{e}", i + 1)))?;
        let width_pt = page.width().value;
        let height_pt = page.height().value;
        let image = render_page(&page, dpi)?;
        out.push(RenderedPage {
            image,
            width_pt,
            height_pt,
        });
    }
    Ok(out)
}
