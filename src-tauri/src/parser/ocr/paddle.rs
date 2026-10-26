//! PaddleOCR（PP-OCR）本地推理引擎
//!
//! 纯 Rust 实现（tract-onnx），不依赖 Python、不依赖 C++ 运行库，模型从本地目录加载：
//!
//! ```text
//! <配置目录>/models/
//!   ├── det.onnx          # 文本检测模型（DB）
//!   ├── rec.onnx          # 文字识别模型（CRNN + CTC）
//!   └── keys.txt          # 字典（ppocr_keys_v1.txt，每行一个字）
//! ```
//!
//! 流程：图像等比缩放 → DB 检测概率图 → 二值化 + 连通域 → 外扩 → 逐行裁剪 → CRNN 识别 → CTC 解码。
//!
//! 模型准备见 README「OCR 模型安装」一节（下载后可完全离线运行）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{imageops, RgbImage};
use tract_onnx::prelude::*;
use tract_onnx::tract_core::model::typed::TypedRunnableModel;

use crate::error::{AppError, Result};
use crate::parser::ocr::engine::{OcrEngine, OcrLine, OcrPageResult};

/// 检测网络固定输入边长（letterbox 到 960×960，32 的整数倍）
const DET_SIZE: usize = 960;
/// 概率图二值化阈值
const DET_BIN_THRESH: f32 = 0.30;
/// 文本框平均得分阈值
const BOX_THRESH: f32 = 0.50;
/// 识别输入高度（PP-OCR 标准 48）与固定宽度（右侧补零）
const REC_HEIGHT: usize = 48;
const REC_WIDTH: usize = 960;

const IMAGE_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGE_STD: [f32; 3] = [0.229, 0.224, 0.225];

#[derive(Clone, Copy, Debug)]
struct DetBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    score: f32,
}

pub struct PaddleOcr {
    det: Arc<TypedRunnableModel>,
    rec: Arc<TypedRunnableModel>,
    keys: Vec<String>,
}

impl PaddleOcr {
    /// 从模型目录加载；缺文件时返回可读的中文错误
    pub fn load(models_dir: &Path) -> Result<Self> {
        let det_path = find_model(
            models_dir,
            &[
                "det.onnx",
                "ch_PP-OCRv4_det_infer.onnx",
                "ch_PP-OCRv3_det_infer.onnx",
            ],
        )
        .ok_or_else(|| {
            AppError::Ocr(format!(
                "未找到 OCR 检测模型（det.onnx），请把模型放入：{}",
                models_dir.display()
            ))
        })?;
        let rec_path = find_model(
            models_dir,
            &[
                "rec.onnx",
                "ch_PP-OCRv4_rec_infer.onnx",
                "ch_PP-OCRv3_rec_infer.onnx",
            ],
        )
        .ok_or_else(|| {
            AppError::Ocr(format!(
                "未找到 OCR 识别模型（rec.onnx），请把模型放入：{}",
                models_dir.display()
            ))
        })?;
        let keys_path = find_model(models_dir, &["keys.txt", "ppocr_keys_v1.txt"])
            .ok_or_else(|| AppError::Ocr("未找到 OCR 字典 keys.txt".into()))?;

        // ONNX 模型输入是动态维度，tract 需要显式的输入事实才能定图；
        // 这里固定为推理时实际使用的尺寸（检测 960×960，识别 48×480）。
        let det = load_plan(&det_path, &[1, 3, DET_SIZE, DET_SIZE])?;
        let rec = load_plan(&rec_path, &[1, 3, REC_HEIGHT, REC_WIDTH])?;

        let keys_text = std::fs::read_to_string(&keys_path)
            .map_err(|e| AppError::Ocr(format!("读取字典失败：{e}")))?;
        let mut keys: Vec<String> = keys_text
            .lines()
            .map(|l| l.trim_end_matches(['\r', '\n']).to_string())
            .collect();
        // 兼容字典首行带 BOM 的情况
        if let Some(first) = keys.first_mut() {
            *first = first.trim_start_matches('\u{feff}').to_string();
        }
        if keys.is_empty() {
            return Err(AppError::Ocr("OCR 字典为空".into()));
        }

        Ok(PaddleOcr { det, rec, keys })
    }

    /// 文本检测：返回原图坐标系下的文本框
    fn detect(&self, img: &RgbImage) -> Result<Vec<DetBox>> {
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return Ok(vec![]);
        }

        // 等比缩放后居中 letterbox 到 960×960，避免拉伸变形
        let scale = (DET_SIZE as f32 / w as f32).min(DET_SIZE as f32 / h as f32);
        let nw = ((w as f32 * scale).round() as usize).clamp(1, DET_SIZE);
        let nh = ((h as f32 * scale).round() as usize).clamp(1, DET_SIZE);
        let offset_x = (DET_SIZE - nw) / 2;
        let offset_y = (DET_SIZE - nh) / 2;

        let resized = imageops::resize(img, nw as u32, nh as u32, imageops::FilterType::Triangle);
        let mut data = vec![0f32; 3 * DET_SIZE * DET_SIZE];
        for (x, y, p) in resized.enumerate_pixels() {
            let dx = x as usize + offset_x;
            let dy = y as usize + offset_y;
            for c in 0..3 {
                let v = p.0[c] as f32 / 255.0;
                data[c * DET_SIZE * DET_SIZE + dy * DET_SIZE + dx] =
                    (v - IMAGE_MEAN[c]) / IMAGE_STD[c];
            }
        }

        let input = Tensor::from_shape(&[1, 3, DET_SIZE, DET_SIZE], &data)
            .map_err(|e| AppError::Ocr(format!("构造检测输入失败：{e}")))?;
        let outputs = self
            .det
            .run(tvec!(input.into()))
            .map_err(|e| AppError::Ocr(format!("检测模型推理失败：{e}")))?;
        let arr = outputs[0]
            .to_plain_array_view::<f32>()
            .map_err(|e| AppError::Ocr(format!("读取检测输出失败：{e}")))?;
        let shape = arr.shape().to_vec();
        let (ph, pw) = match shape.len() {
            4 => (shape[2], shape[3]),
            3 => (shape[1], shape[2]),
            _ => return Err(AppError::Ocr(format!("检测模型输出维度异常：{shape:?}"))),
        };
        let prob: Vec<f32> = arr.iter().copied().collect();
        if pw != DET_SIZE || ph != DET_SIZE {
            return Err(AppError::Ocr(format!(
                "检测模型输出尺寸异常：{shape:?}（期望 {DET_SIZE}×{DET_SIZE}）"
            )));
        }

        if debug_enabled() {
            let mx = prob.iter().cloned().fold(f32::MIN, f32::max);
            let mn = prob.iter().cloned().fold(f32::MAX, f32::min);
            let over = prob.iter().filter(|v| **v > DET_BIN_THRESH).count();
            eprintln!(
                "[OCR] det prob min={mn:.3} max={mx:.3} mean={:.3} >thresh={over}",
                prob.iter().sum::<f32>() / prob.len().max(1) as f32
            );
        }

        // 二值化
        let mut mask = vec![false; pw * ph];
        for i in 0..pw * ph {
            mask[i] = prob[i] > DET_BIN_THRESH;
        }

        // 连通域标记（8 邻域）
        let mut visited = vec![false; pw * ph];
        let mut boxes: Vec<DetBox> = Vec::new();
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for sy in 0..ph {
            for sx in 0..pw {
                let start = sy * pw + sx;
                if !mask[start] || visited[start] {
                    continue;
                }
                visited[start] = true;
                stack.push((sx, sy));
                let (mut min_x, mut max_x, mut min_y, mut max_y) = (sx, sx, sy, sy);
                let mut count = 0usize;
                let mut sum = 0f32;
                while let Some((cx, cy)) = stack.pop() {
                    count += 1;
                    sum += prob[cy * pw + cx];
                    min_x = min_x.min(cx);
                    max_x = max_x.max(cx);
                    min_y = min_y.min(cy);
                    max_y = max_y.max(cy);
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let nx = cx as i32 + dx;
                            let ny = cy as i32 + dy;
                            if nx < 0 || ny < 0 || nx >= pw as i32 || ny >= ph as i32 {
                                continue;
                            }
                            let idx = ny as usize * pw + nx as usize;
                            if mask[idx] && !visited[idx] {
                                visited[idx] = true;
                                stack.push((nx as usize, ny as usize));
                            }
                        }
                    }
                }
                if count < 8 {
                    continue;
                }
                let score = sum / count as f32;
                if score < BOX_THRESH * 0.6 {
                    continue;
                }
                let bw = (max_x - min_x + 1) as f32;
                let bh = (max_y - min_y + 1) as f32;
                boxes.push(DetBox {
                    x: min_x as f32,
                    y: min_y as f32,
                    w: bw,
                    h: bh,
                    score,
                });
            }
        }

        // DB 后处理：按面积/周长外扩，再映射回原图坐标
        let mut mapped: Vec<DetBox> = boxes
            .into_iter()
            .filter_map(|b| {
                let area = b.w * b.h;
                let perim = 2.0 * (b.w + b.h);
                if perim <= 0.0 {
                    return None;
                }
                let d = area * 1.5 / perim;
                let x = ((b.x - d - offset_x as f32) / scale).max(0.0);
                let y = ((b.y - d - offset_y as f32) / scale).max(0.0);
                let x2 = ((b.x + b.w + d - offset_x as f32) / scale).min(w as f32);
                let y2 = ((b.y + b.h + d - offset_y as f32) / scale).min(h as f32);
                if x2 - x < 4.0 || y2 - y < 4.0 {
                    return None;
                }
                Some(DetBox {
                    x,
                    y,
                    w: x2 - x,
                    h: y2 - y,
                    score: b.score,
                })
            })
            .collect();

        if debug_enabled() {
            eprintln!("[OCR] det boxes={}", mapped.len());
        }

        // 同一行的相邻框合并（字间距断开时避免逐字识别）
        mapped.sort_by(|a, b| {
            let ay = (a.y / 8.0) as i32;
            let by = (b.y / 8.0) as i32;
            ay.cmp(&by)
                .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        });
        let mut merged: Vec<DetBox> = Vec::new();
        for b in mapped {
            if let Some(last) = merged.last_mut() {
                let vy = (last.y + last.h).min(b.y + b.h) - last.y.max(b.y);
                let min_h = last.h.min(b.h);
                let h_gap = b.x - (last.x + last.w);
                if vy > min_h * 0.5 && h_gap > -min_h * 0.3 && h_gap < min_h * 2.5 {
                    let x2 = (last.x + last.w).max(b.x + b.w);
                    let y2 = (last.y + last.h).max(b.y + b.h);
                    let nx = last.x.min(b.x);
                    let ny = last.y.min(b.y);
                    last.w = x2 - nx;
                    last.h = y2 - ny;
                    last.x = nx;
                    last.y = ny;
                    last.score = last.score.max(b.score);
                    continue;
                }
            }
            merged.push(b);
        }

        merged.sort_by(|a, b| {
            let ay = (a.y / 12.0) as i32;
            let by = (b.y / 12.0) as i32;
            ay.cmp(&by)
                .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        });
        Ok(merged)
    }

    /// 单行识别，返回（文本，平均置信度）
    fn recognize_line(&self, crop: &RgbImage) -> Result<(String, f32)> {
        let (cw, ch) = crop.dimensions();
        if cw == 0 || ch == 0 {
            return Ok((String::new(), 0.0));
        }
        let ratio = cw as f32 / ch as f32;
        let rw = ((REC_HEIGHT as f32 * ratio).round() as usize).clamp(8, REC_WIDTH);

        let resized = imageops::resize(
            crop,
            rw as u32,
            REC_HEIGHT as u32,
            imageops::FilterType::Triangle,
        );
        // 右侧补 0（归一化后的灰色），保持固定宽度 480
        let mut data = vec![0f32; 3 * REC_WIDTH * REC_HEIGHT];
        for (x, y, p) in resized.enumerate_pixels() {
            for c in 0..3 {
                let v = p.0[c] as f32 / 255.0;
                data[c * REC_WIDTH * REC_HEIGHT + y as usize * REC_WIDTH + x as usize] =
                    (v - 0.5) / 0.5;
            }
        }

        let input = Tensor::from_shape(&[1, 3, REC_HEIGHT, REC_WIDTH], &data)
            .map_err(|e| AppError::Ocr(format!("构造识别输入失败：{e}")))?;
        let outputs = self
            .rec
            .run(tvec!(input.into()))
            .map_err(|e| AppError::Ocr(format!("识别模型推理失败：{e}")))?;
        let arr = outputs[0]
            .to_plain_array_view::<f32>()
            .map_err(|e| AppError::Ocr(format!("读取识别输出失败：{e}")))?;
        let shape = arr.shape().to_vec();
        if shape.len() != 3 {
            return Err(AppError::Ocr(format!("识别模型输出维度异常：{shape:?}")));
        }
        let (steps, classes) = (shape[1], shape[2]);
        let logits: Vec<f32> = arr.iter().copied().collect();

        let mut text = String::new();
        let mut conf_sum = 0f32;
        let mut conf_n = 0usize;
        let mut prev = 0usize;
        for t in 0..steps {
            let row = &logits[t * classes..(t + 1) * classes];
            // argmax
            let mut best = 0usize;
            let mut best_v = f32::NEG_INFINITY;
            for (i, v) in row.iter().enumerate() {
                if *v > best_v {
                    best_v = *v;
                    best = i;
                }
            }
            // 置信度：PP-OCR 的 ONNX 输出通常已带 softmax（值域 0~1），
            // 若判断为概率则直接取最大值，否则按 logits 做 softmax。
            let row_sum: f32 = row.iter().sum();
            let looks_like_prob =
                row_sum > 0.9 && row_sum < 1.1 && row.iter().all(|v| *v >= -1e-4 && *v <= 1.0001);
            let p = if looks_like_prob {
                best_v
            } else {
                let mut sum = 0f32;
                for v in row.iter() {
                    sum += (v - best_v).exp();
                }
                if sum > 0.0 {
                    1.0 / sum
                } else {
                    0.0
                }
            };

            if best != 0 && best != prev {
                if let Some(ch) = self.keys.get(best - 1) {
                    text.push_str(ch);
                    conf_sum += p;
                    conf_n += 1;
                }
            }
            prev = best;
        }

        let conf = if conf_n > 0 {
            conf_sum / conf_n as f32
        } else {
            0.0
        };
        Ok((text.trim().to_string(), conf))
    }
}

impl OcrEngine for PaddleOcr {
    fn id(&self) -> &'static str {
        "paddle_onnx"
    }

    fn label(&self) -> String {
        "PaddleOCR 本地模型（PP-OCR）".into()
    }

    fn is_ready(&self) -> bool {
        true
    }

    fn message(&self) -> String {
        "PaddleOCR 模型已加载，全部推理在本机完成。".into()
    }

    fn recognize(&self, image: &RgbImage) -> Result<OcrPageResult> {
        let (width, height) = image.dimensions();
        let boxes = self.detect(image)?;
        let mut lines = Vec::new();
        for b in boxes {
            let x0 = b.x.floor().max(0.0) as u32;
            let y0 = b.y.floor().max(0.0) as u32;
            let x1 = ((b.x + b.w).ceil() as u32).min(width);
            let y1 = ((b.y + b.h).ceil() as u32).min(height);
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let crop = imageops::crop_imm(image, x0, y0, x1 - x0, y1 - y0).to_image();
            match self.recognize_line(&crop) {
                Ok((text, conf)) if !text.is_empty() && conf > 0.35 => {
                    if debug_enabled() {
                        eprintln!("[OCR] line conf={conf:.2} text={text}");
                    }
                    lines.push(OcrLine {
                        text,
                        x: b.x,
                        y: b.y,
                        w: b.w,
                        h: b.h,
                    });
                }
                Ok((text, conf)) => {
                    if debug_enabled() {
                        eprintln!(
                            "[OCR] skip conf={conf:.2} len={} text={text}",
                            text.chars().count()
                        );
                    }
                }
                Err(e) => {
                    // 单行失败不影响整页，仅记录日志
                    eprintln!("[OCR] 单行识别失败：{e}");
                }
            }
        }
        Ok(OcrPageResult {
            lines,
            width,
            height,
        })
    }
}

fn find_model(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    for name in names {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn load_plan(path: &Path, dims: &[usize]) -> Result<Arc<TypedRunnableModel>> {
    let mut model = tract_onnx::onnx()
        .model_for_path(path)
        .map_err(|e| AppError::Ocr(format!("加载模型 {} 失败：{e:?}", path.display())))?;
    // 动态维度需要显式输入事实，否则 tract 无法推导图类型
    model
        .set_input_fact(0, f32::fact(dims).into())
        .map_err(|e| AppError::Ocr(format!("设置模型输入维度失败：{e:?}")))?;
    let model = model
        .into_optimized()
        .map_err(|e| AppError::Ocr(format!("优化模型 {} 失败：{e:?}", path.display())))?
        .into_runnable()
        .map_err(|e| AppError::Ocr(format!("编译模型 {} 失败：{e:?}", path.display())))?;
    Ok(model)
}

/// 调试开关：新变量名优先，兼容旧名
fn debug_enabled() -> bool {
    std::env::var("TUOMINXING_DEBUG").is_ok() || std::env::var("LEGAL_REDACT_DEBUG").is_ok()
}
