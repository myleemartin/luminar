//! 脱敏 PDF 导出
//!
//! 安全策略：**把页面渲染成图片后，用黑色实心矩形覆盖敏感区域**，重新组装为图片型 PDF。
//! 与「删除文字层」相比，这种做法不会残留可提取的原文字符（不存在复制泄露风险），
//! 是卷宗对外交付的通用做法。代价是导出件不可检索，若需保留文字层请在导出后另行 OCR。
//!
//! 坐标来源：
//! - 文字版 PDF：解析阶段保存的字符级坐标，按行合并成矩形
//! - 扫描件：OCR 行框，按字符占比估算水平范围

use std::path::Path;

use image::{Rgb, RgbImage};
use lopdf::{dictionary, Document, Object, Stream};

use crate::error::{AppError, Result};
use crate::models::{Finding, PageLayout, ParsedDoc};
use crate::parser::pdf::{bind_pdfium, render_page};

/// 一页的最终图片与尺寸
struct RasterPage {
    jpeg: Vec<u8>,
    width_px: u32,
    height_px: u32,
    width_pt: f32,
    height_pt: f32,
}

/// 导出脱敏 PDF
///
/// 逐页「渲染 → 涂黑 → 编码 JPEG」，不在内存里保留整本页图，
/// 100 页以上的扫描卷宗也不会把内存打爆。
pub fn export(
    source: &Path,
    doc: &ParsedDoc,
    findings: &[Finding],
    out: &Path,
    dpi: u32,
    pdfium_path: Option<&Path>,
) -> Result<Vec<String>> {
    let has_enabled = findings.iter().any(|f| f.enabled);
    if has_enabled
        && doc
            .layouts
            .iter()
            .all(|l| l.chars.is_empty() && l.lines.is_empty())
    {
        return Err(AppError::Export(
            "该文档没有可用的页面坐标，无法安全生成脱敏 PDF（请使用 TXT / Word 导出）".into(),
        ));
    }

    let mut warnings = Vec::new();
    let pdfium = bind_pdfium(pdfium_path).map_err(AppError::Pdf)?;
    let pdf = pdfium
        .load_pdf_from_file(source, None)
        .map_err(|e| AppError::Pdf(format!("pdfium 无法打开 PDF：{e}")))?;
    let actual_pages = pdf.pages().len() as usize;
    if actual_pages != doc.pages.len() {
        if has_enabled {
            // 页面数与导入时不一致，坐标可能整体错位，宁可失败也不输出
            return Err(AppError::Export(format!(
                "PDF 页面数（{}）与导入时（{}）不一致，请重新导入后再导出",
                actual_pages,
                doc.pages.len()
            )));
        }
        warnings.push(format!(
            "PDF 页面数（{}）与解析结果（{}）不一致，已按实际页面导出",
            actual_pages,
            doc.pages.len()
        ));
    }

    let mut rasters: Vec<RasterPage> = Vec::with_capacity(actual_pages);
    for i in 0..actual_pages {
        let page = pdf
            .pages()
            .get(i as u16)
            .map_err(|e| AppError::Pdf(format!("第 {} 页不可读：{e}", i + 1)))?;
        let width_pt = page.width().value;
        let height_pt = page.height().value;
        let mut image = render_page(&page, dpi)?;

        let page_findings: Vec<&Finding> = match doc.pages.get(i) {
            Some(range) => findings
                .iter()
                .filter(|f| f.enabled && f.end > range.start && f.start < range.end)
                .collect(),
            None => Vec::new(),
        };

        if !page_findings.is_empty() {
            let layout = doc
                .layouts
                .get(i)
                .filter(|l| !l.chars.is_empty() || !l.lines.is_empty())
                .ok_or_else(|| {
                    // 宁可失败也不能输出一页“看似脱敏、实则原样”的 PDF
                    AppError::Export(format!(
                        "第 {} 页缺少坐标信息，无法安全遮盖（请改用 TXT/Word 导出）",
                        i + 1
                    ))
                })?;
            let range = &doc.pages[i];
            let sx = if width_pt > 0.0 {
                image.width() as f32 / width_pt
            } else {
                1.0
            };
            let sy = if height_pt > 0.0 {
                image.height() as f32 / height_pt
            } else {
                1.0
            };
            for f in page_findings {
                let local_start = f.start.max(range.start) - range.start;
                let local_end = f.end.min(range.end) - range.start;
                for rect in boxes_for(layout, local_start, local_end) {
                    paint_rect(&mut image, rect, sx, sy);
                }
            }
        }

        let jpeg = encode_jpeg(&image)?;
        rasters.push(RasterPage {
            jpeg,
            width_px: image.width(),
            height_px: image.height(),
            width_pt,
            height_pt,
        });
        // 页图在循环结束时释放，内存中只保留压缩后的 JPEG
        drop(image);
    }

    write_pdf(&rasters, out)?;
    Ok(warnings)
}

/// 计算某段文字在页面上的矩形（单位：点，左上角原点）
fn boxes_for(layout: &PageLayout, start: usize, end: usize) -> Vec<(f32, f32, f32, f32)> {
    let mut rects = Vec::new();

    // 文字版 PDF：字符级坐标，按行合并
    if !layout.chars.is_empty() {
        let n = layout.chars.len();
        let end = end.min(n);
        let mut cur: Option<(f32, f32, f32, f32)> = None;
        for idx in start.min(n)..end {
            let c = layout.chars[idx];
            if c.w <= 0.0 || c.h <= 0.0 {
                continue;
            }
            match cur {
                None => cur = Some((c.x, c.y, c.x + c.w, c.y + c.h)),
                Some((x0, y0, x1, y1)) => {
                    let h = (y1 - y0).max(c.h).max(1.0);
                    if (c.y - y0).abs() > h * 0.6 {
                        rects.push((x0, y0, x1 - x0, y1 - y0));
                        cur = Some((c.x, c.y, c.x + c.w, c.y + c.h));
                    } else {
                        cur = Some((
                            x0.min(c.x),
                            y0.min(c.y),
                            x1.max(c.x + c.w),
                            y1.max(c.y + c.h),
                        ));
                    }
                }
            }
        }
        if let Some((x0, y0, x1, y1)) = cur {
            rects.push((x0, y0, x1 - x0, y1 - y0));
        }
        return rects;
    }

    // 扫描件：OCR 行框，按字符占比估算
    let mut offset = 0usize;
    for line in &layout.lines {
        let line_len = line.text.chars().count();
        let line_start = offset;
        let line_end = offset + line_len;
        let inter_start = start.max(line_start);
        let inter_end = end.min(line_end);
        if inter_end > inter_start && line_len > 0 {
            let f0 = (inter_start - line_start) as f32 / line_len as f32;
            let f1 = (inter_end - line_start) as f32 / line_len as f32;
            rects.push((line.x + line.w * f0, line.y, line.w * (f1 - f0), line.h));
        }
        offset = line_end + 1; // 行间换行符
    }
    rects
}

/// 在图片上涂黑（点坐标 → 像素坐标）
fn paint_rect(image: &mut RgbImage, rect: (f32, f32, f32, f32), sx: f32, sy: f32) {
    let (x, y, w, h) = rect;
    // 字符的 tight bounds 只包住字形本体，略去上下伸展部分；
    // 按字高比例外扩，确保整行文字被完全覆盖（不留可辨认的笔画）。
    let h_px = (h * sy).abs().max(8.0);
    let pad_y = (h_px * 0.35).max(2.5);
    let pad_x = (h_px * 0.10).max(1.5);
    let x0 = ((x * sx) - pad_x).floor().max(0.0) as u32;
    let y0 = ((y * sy) - pad_y).floor().max(0.0) as u32;
    let x1 = (((x + w) * sx) + pad_x).ceil().max(0.0) as u32;
    let y1 = (((y + h) * sy) + pad_y).ceil().max(0.0) as u32;
    let x1 = x1.min(image.width());
    let y1 = y1.min(image.height());
    for py in y0..y1 {
        for px in x0..x1 {
            image.put_pixel(px, py, Rgb([0, 0, 0]));
        }
    }
}

fn encode_jpeg(image: &RgbImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
    encoder
        .encode_image(image)
        .map_err(|e| AppError::Export(format!("页面图片编码失败：{e}")))?;
    Ok(buf)
}

/// 用 lopdf 组装图片型 PDF
fn write_pdf(pages: &[RasterPage], out: &Path) -> Result<()> {
    let mut pdf = Document::with_version("1.5");
    let pages_id = pdf.new_object_id();
    let mut kids: Vec<Object> = Vec::with_capacity(pages.len());

    for page in pages {
        // 页面尺寸异常时退回 A4，避免生成 0×0 的坏 PDF
        let width_pt = if page.width_pt > 1.0 {
            page.width_pt
        } else {
            595.0
        };
        let height_pt = if page.height_pt > 1.0 {
            page.height_pt
        } else {
            842.0
        };
        let image_id = pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => page.width_px as i64,
                "Height" => page.height_px as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            page.jpeg.clone(),
        ));

        let content = format!(
            "q {} 0 0 {} 0 0 cm /Im0 Do Q",
            page.width_pt, page.height_pt
        );
        let content_id = pdf.add_object(Stream::new(dictionary! {}, content.into_bytes()));

        let page_id = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(width_pt),
                Object::Real(height_pt),
            ],
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Im0" => image_id },
            },
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }

    pdf.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages.len() as i64,
        }),
    );
    let catalog_id = pdf.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    pdf.trailer.set("Root", catalog_id);
    pdf.save(out)?;
    Ok(())
}
