//! OCR 引擎抽象
//!
//! 设计为可插拔：默认使用 PaddleOCR 的 ONNX 模型本地推理（纯 Rust，无外部进程），
//! 也可以配置成本机已有的命令行 OCR（如 RapidOCR / PaddleOCR 的 CLI），全部离线。
//! 新增引擎只需实现 [`OcrEngine`] 并在 `ocr::create_engine` 注册。

use crate::error::Result;

/// 一行识别结果（坐标为像素，左上角原点）
#[derive(Debug, Clone)]
pub struct OcrLine {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// 单页 OCR 结果
#[derive(Debug, Clone, Default)]
pub struct OcrPageResult {
    pub lines: Vec<OcrLine>,
    pub width: u32,
    pub height: u32,
}

pub trait OcrEngine: Send + Sync {
    /// 引擎短 id
    fn id(&self) -> &'static str;
    /// 引擎中文名
    fn label(&self) -> String;
    /// 模型/命令是否就绪
    fn is_ready(&self) -> bool;
    /// 未就绪时的提示
    fn message(&self) -> String;
    /// 识别整页图片
    fn recognize(&self, image: &image::RgbImage) -> Result<OcrPageResult>;
}
