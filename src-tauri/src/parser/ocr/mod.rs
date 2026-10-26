//! OCR 层门面：按配置选择引擎、探测模型、暴露状态
//!
//! 默认引擎 `paddle_onnx`：使用 PaddleOCR 的检测 + 识别 ONNX 模型本地推理；
//! 备选 `external`：调用本机已有的离线 OCR 命令；
//! `off`：关闭 OCR（扫描件仅提示，不提取文字）。

pub mod engine;
pub mod external;
#[cfg(feature = "ocr-paddle")]
pub mod paddle;

pub use engine::{OcrEngine, OcrLine, OcrPageResult};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::models::{OcrStatus, Settings};

/// 模型文件清单（与 paddle::load 保持一致）
const DET_NAMES: &[&str] = &[
    "det.onnx",
    "ch_PP-OCRv4_det_infer.onnx",
    "ch_PP-OCRv3_det_infer.onnx",
];
const REC_NAMES: &[&str] = &[
    "rec.onnx",
    "ch_PP-OCRv4_rec_infer.onnx",
    "ch_PP-OCRv3_rec_infer.onnx",
];
const KEY_NAMES: &[&str] = &["keys.txt", "ppocr_keys_v1.txt"];

fn find(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

/// 按设置创建 OCR 引擎
pub fn build_engine(settings: &Settings, models_dir: &Path) -> Result<Arc<dyn OcrEngine>> {
    match settings.ocr_engine.as_str() {
        "off" => Err(AppError::Ocr("OCR 已在设置中关闭".into())),
        "external" => {
            let engine = external::ExternalOcr::new(&settings.external_ocr_command);
            if !engine.is_ready() {
                return Err(AppError::Ocr(engine.message()));
            }
            Ok(Arc::new(engine))
        }
        _ => {
            #[cfg(feature = "ocr-paddle")]
            {
                let engine = paddle::PaddleOcr::load(models_dir)?;
                Ok(Arc::new(engine))
            }
            #[cfg(not(feature = "ocr-paddle"))]
            {
                let _ = models_dir;
                Err(AppError::Ocr(
                    "当前构建未启用 OCR 特性（编译时去掉 --no-default-features 即可）".into(),
                ))
            }
        }
    }
}

/// 引擎状态（不加载模型，避免设置面板卡顿）
pub fn status(settings: &Settings, models_dir: &Path) -> OcrStatus {
    let models_dir_s = models_dir.to_string_lossy().to_string();
    match settings.ocr_engine.as_str() {
        "off" => OcrStatus {
            engine: "off".into(),
            engine_label: "已关闭".into(),
            ready: false,
            message: "扫描件将不会被识别文字。".into(),
            models_dir: models_dir_s,
        },
        "external" => {
            let e = external::ExternalOcr::new(&settings.external_ocr_command);
            OcrStatus {
                engine: e.id().into(),
                engine_label: e.label(),
                ready: e.is_ready(),
                message: e.message(),
                models_dir: models_dir_s,
            }
        }
        _ => {
            let has_det = find(models_dir, DET_NAMES).is_some();
            let has_rec = find(models_dir, REC_NAMES).is_some();
            let has_keys = find(models_dir, KEY_NAMES).is_some();
            let ready = has_det && has_rec && has_keys;
            let message = if ready {
                "PaddleOCR 模型已就位，扫描件将自动 OCR（全程本地）。".to_string()
            } else {
                let mut missing = Vec::new();
                if !has_det {
                    missing.push("det.onnx");
                }
                if !has_rec {
                    missing.push("rec.onnx");
                }
                if !has_keys {
                    missing.push("keys.txt");
                }
                format!(
                    "缺少模型文件：{}。请放入 {} 后重启本程序。",
                    missing.join("、"),
                    models_dir.display()
                )
            };
            OcrStatus {
                engine: "paddle_onnx".into(),
                engine_label: "PaddleOCR 本地模型（PP-OCR）".into(),
                ready,
                message,
                models_dir: models_dir_s,
            }
        }
    }
}
