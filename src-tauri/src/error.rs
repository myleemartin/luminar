//! 统一错误类型
//!
//! 所有对外命令都返回 `Result<T, AppError>`，并实现 `Serialize`，
//! 前端 catch 到的是中文字符串，可直接提示给律师用户。

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("不支持的文件格式：{0}")]
    Unsupported(String),

    #[error("PDF 解析失败：{0}")]
    Pdf(String),

    #[error("Word 文档解析失败：{0}")]
    Docx(String),

    #[error("OCR 识别失败：{0}")]
    Ocr(String),

    #[error("规则配置有误：{0}")]
    Rule(String),

    #[error("导出失败：{0}")]
    Export(String),

    #[error("配置读写失败：{0}")]
    Config(String),

    #[error("找不到该文档，请重新导入")]
    NoDocument,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn other(msg: impl Into<String>) -> Self {
        AppError::Other(msg.into())
    }
}

// Tauri 要求错误类型实现 Serialize，前端才能拿到中文错误信息
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(format!("{e:#}"))
    }
}
