//! 应用运行时状态
//!
//! `AppState` 由 Tauri 托管；内部字段全部 `Arc<Mutex<..>>`，命令层可以克隆 `Ctx`
//! 后丢进 `spawn_blocking`，避免解析/识别/导出阻塞 UI 线程。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::config::ConfigStore;
use crate::detect::model_ner::{self, ModelNer};
use crate::error::{AppError, Result};
use crate::models::{DocKind, DocSummary, Finding, ParsedDoc};
use crate::parser::ocr::{self, OcrEngine};
use crate::parser::ParseOptions;

/// 单份文档的会话数据
pub struct DocumentSession {
    pub id: String,
    pub file_name: String,
    pub file_path: PathBuf,
    pub file_size: u64,
    pub sha256: String,
    /// 全文 Unicode 字符数（缓存，避免每次刷新列表都重新扫描全文）
    pub char_count: usize,
    pub kind: DocKind,
    pub doc: Arc<ParsedDoc>,
    pub findings: Vec<Finding>,
    pub needs_ocr: bool,
    pub ocr_used: bool,
    pub warnings: Vec<String>,
    pub detected: bool,
}

impl DocumentSession {
    pub fn summary(&self) -> DocSummary {
        DocSummary {
            id: self.id.clone(),
            file_name: self.file_name.clone(),
            file_path: self.file_path.to_string_lossy().to_string(),
            kind: self.kind.id().to_string(),
            kind_label: self.kind.label().to_string(),
            page_count: self.doc.page_count,
            char_count: self.char_count,
            file_size: self.file_size,
            needs_ocr: self.needs_ocr,
            ocr_used: self.ocr_used,
            finding_count: self.findings.len(),
            enabled_count: self.findings.iter().filter(|f| f.enabled).count(),
            detected: self.detected,
            warnings: self.warnings.clone(),
        }
    }
}

/// OCR 引擎缓存：配置签名 + 引擎实例（模型只加载/编译一次）
pub type OcrCache = Arc<Mutex<Option<(String, Arc<dyn OcrEngine>)>>>;

/// 增强识别模型缓存：目录 + 模型实例
pub type NerCache = Arc<Mutex<Option<(String, Arc<ModelNer>)>>>;

/// 可克隆的上下文
#[derive(Clone)]
pub struct Ctx {
    pub config: Arc<Mutex<ConfigStore>>,
    pub docs: Arc<Mutex<HashMap<String, DocumentSession>>>,
    pub ocr_cache: OcrCache,
    pub model_ner_cache: NerCache,
    pub models_dir: PathBuf,
    pub pdfium_path: Option<PathBuf>,
}

pub struct AppState {
    pub ctx: Ctx,
}

impl AppState {
    pub fn new(config: ConfigStore, models_dir: PathBuf, pdfium_path: Option<PathBuf>) -> Self {
        AppState {
            ctx: Ctx {
                config: Arc::new(Mutex::new(config)),
                docs: Arc::new(Mutex::new(HashMap::new())),
                ocr_cache: Arc::new(Mutex::new(None)),
                model_ner_cache: Arc::new(Mutex::new(None)),
                models_dir,
                pdfium_path,
            },
        }
    }
}

impl Ctx {
    /// 读取设置副本
    pub fn settings(&self) -> crate::models::Settings {
        self.config
            .lock()
            .map(|c| c.settings.clone())
            .unwrap_or_default()
    }

    /// 获取 OCR 引擎（按配置缓存，模型只加载一次）
    pub fn ocr_engine(&self) -> Result<Arc<dyn OcrEngine>> {
        let settings = self.settings();
        let key = format!(
            "{}|{}|{}",
            settings.ocr_engine,
            settings.external_ocr_command,
            self.models_dir.display()
        );
        {
            let cache = self
                .ocr_cache
                .lock()
                .map_err(|_| AppError::Other("OCR 缓存锁异常".into()))?;
            if let Some((k, engine)) = cache.as_ref() {
                if *k == key {
                    return Ok(engine.clone());
                }
            }
        }
        let engine = ocr::build_engine(&settings, &self.models_dir)?;
        let mut cache = self
            .ocr_cache
            .lock()
            .map_err(|_| AppError::Other("OCR 缓存锁异常".into()))?;
        *cache = Some((key, engine.clone()));
        Ok(engine)
    }

    /// 构造解析选项：OCR 引擎惰性加载，纯文字文档不会触发模型加载
    pub fn parse_options(&self) -> ParseOptions {
        let settings = self.settings();
        let ctx = self.clone();
        let provider: crate::parser::OcrProvider = Arc::new(move || ctx.ocr_engine());
        ParseOptions {
            ocr: None,
            ocr_provider: Some(provider),
            ocr_dpi: settings.ocr_dpi,
            pdfium_path: self.pdfium_path.clone(),
            progress: None,
        }
    }

    /// 本地增强识别模型（按目录缓存，加载一次后复用）
    pub fn model_ner(&self) -> Result<Arc<ModelNer>> {
        let dir = model_ner::model_dir(&self.models_dir);
        let key = dir.to_string_lossy().to_string();
        {
            let cache = self
                .model_ner_cache
                .lock()
                .map_err(|_| AppError::Other("增强识别缓存锁异常".into()))?;
            if let Some((k, model)) = cache.as_ref() {
                if *k == key {
                    return Ok(model.clone());
                }
            }
        }
        let model = Arc::new(ModelNer::load(&dir)?);
        let mut cache = self
            .model_ner_cache
            .lock()
            .map_err(|_| AppError::Other("增强识别缓存锁异常".into()))?;
        *cache = Some((key, model.clone()));
        Ok(model)
    }

    pub fn model_ner_ready(&self) -> bool {
        ModelNer::is_available(&model_ner::model_dir(&self.models_dir))
    }

    pub fn clear_ocr_cache(&self) {
        if let Ok(mut cache) = self.ocr_cache.lock() {
            *cache = None;
        }
    }
}
