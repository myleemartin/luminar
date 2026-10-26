//! 领域数据模型
//!
//! 设计原则：
//! 1. 全文统一成一个字符串 `ParsedDoc::text`，所有敏感片段用「字符下标」定位，
//!    这样识别、复核、脱敏、导出共用一套坐标，避免多坐标系转换出错。
//! 2. 字符下标按 Unicode 标量（Rust `char`）计数，前端用 `Array.from()` 同步计数。
//! 3. 规则、脱敏策略全部可序列化，方便持久化为本地 JSON。

use serde::{Deserialize, Serialize};

/// 软件版本号（用于脱敏日志留痕）
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// 文档
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    /// 文字版 PDF（有文本层）
    PdfText,
    /// 扫描件 PDF（无文本层，需要 OCR）
    PdfScan,
    /// Word 文档
    Docx,
    /// 纯文本
    Txt,
    /// RTF 富文本
    Rtf,
    /// HTML / HTM
    Html,
    /// OFD 版式文档
    Ofd,
    /// 旧版 Word / WPS（经 LibreOffice/textutil 转换）
    DocLegacy,
}

impl DocKind {
    pub fn label(self) -> &'static str {
        match self {
            DocKind::PdfText => "文字 PDF",
            DocKind::PdfScan => "扫描 PDF",
            DocKind::Docx => "Word 文档",
            DocKind::Txt => "纯文本",
            DocKind::Rtf => "RTF 文档",
            DocKind::Html => "HTML 文档",
            DocKind::Ofd => "OFD 文档",
            DocKind::DocLegacy => "Word/WPS 旧格式",
        }
    }

    pub fn is_pdf(self) -> bool {
        matches!(self, DocKind::PdfText | DocKind::PdfScan)
    }

    pub fn id(self) -> &'static str {
        match self {
            DocKind::PdfText => "pdf_text",
            DocKind::PdfScan => "pdf_scan",
            DocKind::Docx => "docx",
            DocKind::Txt => "txt",
            DocKind::Rtf => "rtf",
            DocKind::Html => "html",
            DocKind::Ofd => "ofd",
            DocKind::DocLegacy => "doc_legacy",
        }
    }
}

/// 页面在全文中的位置（用于分页预览）
#[derive(Debug, Clone, Serialize)]
pub struct PageRange {
    /// 1 基页码
    pub page: usize,
    pub start: usize,
    pub end: usize,
    /// 展示用标题，例如「第 3 页」
    pub label: String,
}

/// 文本块（段落 / 页内文本 / OCR 行），保留它便于导出 Word 时对齐原始段落
#[derive(Debug, Clone, Serialize)]
pub struct TextBlock {
    pub page: usize,
    pub start: usize,
    pub end: usize,
    pub kind: String,
}

/// 单个字符在 PDF 页面上的坐标（单位：点，左上角原点）
#[derive(Debug, Clone, Copy)]
pub struct CharBox {
    pub ch: char,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// OCR 得到的一行文字及其坐标
#[derive(Debug, Clone)]
pub struct LineBox {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// PDF 页面的版面信息，导出脱敏 PDF 时用来定位黑框
#[derive(Debug, Default, Clone)]
pub struct PageLayout {
    pub page: usize,
    pub width: f32,
    pub height: f32,
    /// 字符级坐标（文字 PDF）
    pub chars: Vec<CharBox>,
    /// 行级坐标（扫描件 OCR）
    pub lines: Vec<LineBox>,
}

/// 解析后的文档
#[derive(Debug, Clone)]
pub struct ParsedDoc {
    /// 全文，各文本块以 `\n` 连接
    pub text: String,
    pub blocks: Vec<TextBlock>,
    pub pages: Vec<PageRange>,
    pub page_count: usize,
    pub layouts: Vec<PageLayout>,
}

impl ParsedDoc {
    /// 由「每页一段文字」构造文档：页与页之间用换行连接
    pub fn from_pages(pages_text: Vec<String>) -> Self {
        let mut text = String::new();
        let mut pages = Vec::new();
        for (i, p) in pages_text.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            let start = text.chars().count();
            text.push_str(p);
            let end = start + p.chars().count();
            pages.push(PageRange {
                page: i + 1,
                start,
                end,
                label: format!("第 {} 页", i + 1),
            });
        }
        let page_count = pages.len();
        ParsedDoc {
            text,
            blocks: Vec::new(),
            pages,
            page_count,
            layouts: Vec::new(),
        }
    }

    /// 由段落列表构造（TXT / DOCX），并做虚拟分页：约 3500 字一页
    pub fn from_blocks(paragraphs: Vec<String>, chars_per_page: usize) -> Self {
        let text = paragraphs.join("\n");
        let mut blocks = Vec::with_capacity(paragraphs.len());
        let mut cursor = 0usize;
        for p in &paragraphs {
            let len = p.chars().count();
            blocks.push(TextBlock {
                page: 0, // 稍后回填
                start: cursor,
                end: cursor + len,
                kind: "paragraph".into(),
            });
            cursor += len + 1; // +1 为连接符 \n
        }

        // 虚拟分页：按段落边界累计字符数，避免把一段切两半
        let mut pages = Vec::new();
        let total = text.chars().count();
        let mut page_start = 0usize;
        let mut acc = 0usize;
        let mut page_no = 1usize;
        for b in &blocks {
            let seg = b.end.saturating_sub(b.start) + 1;
            if acc > 0 && acc + seg > chars_per_page {
                let end = b.start.saturating_sub(1).min(total);
                pages.push(PageRange {
                    page: page_no,
                    start: page_start,
                    end,
                    label: format!("第 {} 页", page_no),
                });
                page_no += 1;
                page_start = b.start;
                acc = 0;
            }
            acc += seg;
        }
        if total > page_start || pages.is_empty() {
            pages.push(PageRange {
                page: page_no,
                start: page_start,
                end: total,
                label: format!("第 {} 页", page_no),
            });
        }
        // 回填每个段落所属页码
        for b in blocks.iter_mut() {
            b.page = pages
                .iter()
                .find(|p| b.start >= p.start && b.start <= p.end)
                .map(|p| p.page)
                .unwrap_or(1);
        }
        ParsedDoc {
            text,
            blocks,
            page_count: pages.len(),
            pages,
            layouts: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 识别结果
// ---------------------------------------------------------------------------

/// 脱敏方式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    /// 掩码遮蔽：张*、138****8888
    Mask,
    /// 占位标签替换：{{姓名}}
    Placeholder,
    /// 直接删除
    Delete,
}

impl StrategyKind {
    pub fn id(self) -> &'static str {
        match self {
            StrategyKind::Mask => "mask",
            StrategyKind::Placeholder => "placeholder",
            StrategyKind::Delete => "delete",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            StrategyKind::Mask => "掩码遮蔽",
            StrategyKind::Placeholder => "占位标签",
            StrategyKind::Delete => "直接删除",
        }
    }
}

/// 一条规则 / 一条命中记录使用的脱敏策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub kind: StrategyKind,
    /// 掩码字符（单个字符）
    #[serde(default = "default_mask_char")]
    pub mask_char: String,
    /// 保留前 N 个字符（如手机号前 3 位）
    #[serde(default)]
    pub keep_prefix: usize,
    /// 保留后 N 个字符
    #[serde(default)]
    pub keep_suffix: usize,
    /// 占位模板，支持 {type} 与 {index}
    #[serde(default = "default_placeholder")]
    pub placeholder: String,
}

fn default_mask_char() -> String {
    "*".to_string()
}
fn default_placeholder() -> String {
    "【{type}】".to_string()
}

impl Default for StrategyConfig {
    fn default() -> Self {
        StrategyConfig {
            kind: StrategyKind::Mask,
            mask_char: "*".into(),
            keep_prefix: 0,
            keep_suffix: 0,
            placeholder: default_placeholder(),
        }
    }
}

/// 命中来源
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingSource {
    /// 正则规则
    Regex,
    /// 关键词规则
    Keyword,
    /// 内置 NER 人名识别
    Ner,
    /// 同名传播（同一姓名在全文再次出现）
    Propagated,
    /// 人工框选追加
    Manual,
    /// 本地小模型（增强识别）
    Model,
}

impl FindingSource {
    pub fn id(&self) -> &'static str {
        match self {
            FindingSource::Regex => "regex",
            FindingSource::Keyword => "keyword",
            FindingSource::Ner => "ner",
            FindingSource::Propagated => "propagated",
            FindingSource::Manual => "manual",
            FindingSource::Model => "model",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            FindingSource::Regex => "规则识别",
            FindingSource::Keyword => "关键词",
            FindingSource::Ner => "人名识别",
            FindingSource::Propagated => "同名传播",
            FindingSource::Manual => "人工标记",
            FindingSource::Model => "模型识别",
        }
    }
}

/// 一条敏感信息命中记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    /// 全文起始字符下标（含）
    pub start: usize,
    /// 全文结束字符下标（不含）
    pub end: usize,
    /// 原文片段
    pub text: String,
    /// 分类 id，如 id_card
    pub category: String,
    /// 分类中文名，如 身份证号
    pub category_label: String,
    /// 置信度 0~1
    pub confidence: f32,
    pub source: FindingSource,
    pub rule_id: Option<String>,
    pub rule_name: String,
    /// 是否参与脱敏（人工可取消勾选）
    pub enabled: bool,
    pub strategy: StrategyConfig,
}

impl Finding {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

// ---------------------------------------------------------------------------
// 规则
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// 正则表达式
    #[default]
    Regex,
    /// 关键词列表
    Keyword,
    /// 内置人名识别（不可自定义表达式）
    Ner,
}

impl RuleKind {
    pub fn label(self) -> &'static str {
        match self {
            RuleKind::Regex => "正则表达式",
            RuleKind::Keyword => "关键词列表",
            RuleKind::Ner => "内置智能识别",
        }
    }
}

/// 校验器：识别到候选后做二次校验，降低误报
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Validator {
    #[default]
    None,
    /// 18 位身份证校验位
    IdCard,
    /// 银行卡 Luhn 校验
    Luhn,
    /// 统一社会信用代码校验
    Uscc,
}

impl Validator {
    pub fn id(self) -> &'static str {
        match self {
            Validator::None => "none",
            Validator::IdCard => "id_card",
            Validator::Luhn => "luhn",
            Validator::Uscc => "uscc",
        }
    }
}

/// 一条识别规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// 规则 id；手写 JSON 时可省略，加载时自动生成
    #[serde(default = "default_rule_id")]
    pub id: String,
    pub name: String,
    /// 分类 id
    #[serde(default = "default_category")]
    pub category: String,
    /// 分类中文名（自定义分类时使用）
    #[serde(default = "default_category_label")]
    pub category_label: String,
    #[serde(default)]
    pub kind: RuleKind,
    /// 正则表达式（kind = regex 时使用）
    #[serde(default)]
    pub pattern: String,
    /// 关键词列表（kind = keyword 时使用）
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 取第几个捕获组作为命中片段，0 表示整段
    #[serde(default)]
    pub capture_group: usize,
    /// 基础置信度
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    #[serde(default)]
    pub strategy: StrategyConfig,
    #[serde(default)]
    pub validator: Validator,
    /// 冲突时的优先级，越大越优先
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 是否要求前后不是字母数字（防止从长串数字中截取）
    #[serde(default)]
    pub require_boundary: bool,
    #[serde(default)]
    pub case_insensitive: bool,
    /// 是否为内置规则
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub description: String,
}

fn default_true() -> bool {
    true
}

fn default_rule_id() -> String {
    format!("custom_{}", uuid::Uuid::new_v4().simple())
}

fn default_category() -> String {
    "custom".to_string()
}

fn default_category_label() -> String {
    "自定义".to_string()
}

fn default_confidence() -> f32 {
    0.9
}

impl Rule {
    pub fn new_custom(name: &str, category: &str, label: &str, kind: RuleKind) -> Self {
        Rule {
            id: format!("custom_{}", uuid::Uuid::new_v4().simple()),
            name: name.to_string(),
            category: category.to_string(),
            category_label: label.to_string(),
            kind,
            pattern: String::new(),
            keywords: Vec::new(),
            capture_group: 0,
            confidence: 0.9,
            strategy: StrategyConfig {
                kind: StrategyKind::Placeholder,
                ..Default::default()
            },
            validator: Validator::None,
            priority: 10,
            enabled: true,
            require_boundary: false,
            case_insensitive: false,
            builtin: false,
            description: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 配置
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 置信度阈值：低于该值的自动识别结果不展示
    pub threshold: f32,
    /// OCR 渲染 DPI
    pub ocr_dpi: u32,
    /// OCR 引擎：paddle_onnx / external / off
    pub ocr_engine: String,
    /// 外部 OCR 命令模板，{image} 会被替换为图片路径
    pub external_ocr_command: String,
    /// 导出脱敏 PDF 的渲染 DPI
    pub pdf_export_dpi: u32,
    /// 默认掩码字符
    pub default_mask_char: String,
    /// 默认占位模板
    pub placeholder_template: String,
    /// 是否把已识别姓名在全文中的其他出现位置一并标记
    pub propagate_names: bool,
    /// 是否启用本地小模型增强识别（默认关闭）
    #[serde(default)]
    pub enhance_model: bool,
    /// 上次导出目录
    pub last_output_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            threshold: 0.75,
            ocr_dpi: 200,
            ocr_engine: "paddle_onnx".into(),
            external_ocr_command: String::new(),
            pdf_export_dpi: 150,
            default_mask_char: "*".into(),
            placeholder_template: "【{type}】".into(),
            propagate_names: true,
            enhance_model: false,
            last_output_dir: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 前端交互载荷
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DocSummary {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub kind: String,
    pub kind_label: String,
    pub page_count: usize,
    pub char_count: usize,
    pub file_size: u64,
    pub needs_ocr: bool,
    pub ocr_used: bool,
    pub finding_count: usize,
    pub enabled_count: usize,
    pub detected: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocPayload {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub kind: String,
    pub kind_label: String,
    pub page_count: usize,
    pub char_count: usize,
    pub file_size: u64,
    pub sha256: String,
    pub pages: Vec<PageRange>,
    pub text: String,
    pub findings: Vec<Finding>,
    pub needs_ocr: bool,
    pub ocr_used: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedactionSegment {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    pub category: String,
    pub category_label: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedactionPreview {
    pub text: String,
    pub segments: Vec<RedactionSegment>,
    pub total: usize,
    pub by_category: Vec<(String, String, usize)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportOutcome {
    pub docs: Vec<DocSummary>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportedFile {
    pub path: String,
    pub label: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportOutcome {
    pub doc_id: String,
    pub file_name: String,
    pub files: Vec<ExportedFile>,
    pub audit_path: String,
    pub warnings: Vec<String>,
    /// 本次实际脱敏条目数
    pub total: usize,
    /// 失败原因（成功为 None）
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OcrStatus {
    pub engine: String,
    pub engine_label: String,
    pub ready: bool,
    pub message: String,
    pub models_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnhanceStatus {
    pub ready: bool,
    pub message: String,
    pub model_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppPaths {
    pub config_dir: String,
    pub rules_file: String,
    pub settings_file: String,
    pub models_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryInfo {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleTestResult {
    pub matches: Vec<String>,
    pub count: usize,
    pub error: Option<String>,
}
