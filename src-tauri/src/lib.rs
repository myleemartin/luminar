//! 星星脱敏 —— 应用入口（Tauri 2）
//!
//! 分层结构：
//! ```text
//! commands  命令层：前后端接口、状态编排
//! parser    解析层：docx / pdf / txt / OCR
//! detect    识别层：规则引擎 + 人名 NER + 校验器
//! redact    脱敏层：三类策略、字符级替换
//! export    导出层：TXT / DOCX（保留排版）/ 脱敏 PDF / 合规日志
//! config    配置层：settings.json、rules.json
//! ```
//!
//! 安全边界：本程序不包含任何 HTTP 客户端，不发起网络请求，不写用户目录以外的位置
//! （除应用配置目录与用户主动选择的导出目录）。

pub mod commands;
pub mod config;
pub mod detect;
pub mod error;
pub mod export;
pub mod models;
pub mod parser;
pub mod redact;
pub mod state;

use std::fs;
use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::config::ConfigStore;
use crate::state::AppState;

/// 桌面端启动入口
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 配置目录：macOS ~/Library/Application Support/cn.luminar.app
            //           Windows %APPDATA%\cn.luminar.app
            let config_dir = app
                .path()
                .app_config_dir()
                .map_err(|e| format!("无法定位应用配置目录：{e}"))?;

            // 0.11 起软件改名为「星星脱敏 / Luminar」、标识符变更，自动迁移旧版配置与模型
            migrate_legacy_layout(&config_dir);

            let config = ConfigStore::init(config_dir.clone())?;

            // 模型目录：环境变量 > 用户配置目录 > 安装包内置
            let models_dir = resolve_models_dir(app, &config.paths.models_dir);
            ensure_ner_models(app, &models_dir);

            let pdfium_path = locate_pdfium(app);
            eprintln!("[启动] 配置目录：{}", config_dir.display());
            eprintln!("[启动] 模型目录：{}", models_dir.display());
            if has_ner_models(&models_dir) {
                eprintln!(
                    "[启动] 增强脱敏模型：{}",
                    models_dir.join("ner").join("ner.onnx").display()
                );
            } else {
                eprintln!("[启动] 增强脱敏模型缺失，「增强脱敏」开关将不可用");
            }
            if let Some(p) = &pdfium_path {
                eprintln!("[启动] pdfium：{}", p.display());
            } else {
                eprintln!("[启动] 未找到 pdfium，PDF 将退化为纯文本抽取（扫描件无法 OCR）");
            }

            app.manage(AppState::new(config, models_dir, pdfium_path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_paths,
            commands::get_settings,
            commands::save_settings,
            commands::list_rules,
            commands::save_rule,
            commands::delete_rule,
            commands::reset_rules,
            commands::list_categories,
            commands::test_rule,
            commands::ocr_status,
            commands::enhance_status,
            commands::default_export_dir,
            commands::import_documents,
            commands::get_document,
            commands::detect_document,
            commands::redetect_all,
            commands::reparse_document,
            commands::remove_document,
            commands::update_finding,
            commands::add_manual_finding,
            commands::remove_finding,
            commands::set_all_enabled,
            commands::redact_preview,
            commands::export_documents,
            commands::open_path,
        ])
        .run(tauri::generate_context!())
        .expect("星星脱敏启动失败");
}

// ---------------------------------------------------------------------------
// 旧版配置迁移
// ---------------------------------------------------------------------------

/// 从旧标识符的配置目录迁移规则、设置与 OCR 模型。
///
/// 支持两代旧目录：
/// - `cn.tuominxing.app`（0.10 旧品牌）
/// - `cn.guanzhu.legalredact`（更早版本）
/// 只在「新目录还没有 rules.json」时执行一次，绝不覆盖用户已有数据。
fn migrate_legacy_layout(new_dir: &Path) {
    if new_dir.join("rules.json").exists() {
        return;
    }
    let Some(parent) = new_dir.parent() else {
        return;
    };
    let old_dir = ["cn.tuominxing.app", "cn.guanzhu.legalredact"]
        .iter()
        .map(|name| parent.join(name))
        .find(|dir| dir.is_dir());
    let Some(old_dir) = old_dir else {
        return;
    };
    if let Err(e) = fs::create_dir_all(new_dir) {
        eprintln!("[启动] 创建新版配置目录失败：{e}");
        return;
    }

    let mut migrated = Vec::new();
    for name in ["rules.json", "settings.json"] {
        let src = old_dir.join(name);
        if src.is_file() && fs::copy(&src, new_dir.join(name)).is_ok() {
            migrated.push(name);
        }
    }
    let old_models = old_dir.join("models");
    if old_models.is_dir() {
        if let Err(e) = copy_dir_all(&old_models, &new_dir.join("models")) {
            eprintln!("[启动] 迁移旧版 OCR 模型失败：{e}");
        } else {
            migrated.push("models/");
        }
    }
    if !migrated.is_empty() {
        eprintln!(
            "[启动] 已从旧版目录迁移：{} → {}",
            old_dir.display(),
            new_dir.display()
        );
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 模型目录解析
// ---------------------------------------------------------------------------

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

fn has_any(dir: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| dir.join(n).is_file())
}

fn has_ocr_models(dir: &Path) -> bool {
    has_any(dir, DET_NAMES) && has_any(dir, REC_NAMES) && has_any(dir, KEY_NAMES)
}

/// 依次尝试：环境变量 → 用户配置目录 → 安装包内置 → 开发目录。
fn resolve_models_dir(app: &tauri::App, configured: &str) -> PathBuf {
    let configured = PathBuf::from(configured);
    for key in ["TUOMINXING_MODELS", "LEGAL_REDACT_MODELS"] {
        if let Ok(p) = std::env::var(key) {
            if !p.trim().is_empty() {
                return PathBuf::from(p);
            }
        }
    }
    if has_ocr_models(&configured) {
        return configured;
    }
    let mut bundled: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        bundled.push(res.join("resources").join("models"));
        bundled.push(res.join("models"));
    }
    bundled.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("models"),
    );
    bundled
        .into_iter()
        .find(|p| has_ocr_models(p))
        .unwrap_or(configured)
}

/// 老用户配置目录里可能只有 OCR 模型、没有新增的增强识别模型；
/// 启动时把安装包内置的 `models/ner` 复制过去，保证「增强识别」可用。
fn ensure_ner_models(app: &tauri::App, models_dir: &Path) {
    if has_ner_models(models_dir) {
        return;
    }
    let mut bundled: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        bundled.push(res.join("resources").join("models"));
        bundled.push(res.join("models"));
    }
    bundled.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("models"),
    );
    let Some(src_root) = bundled.into_iter().find(|p| has_ner_models(p)) else {
        eprintln!("[启动] 未找到内置增强识别模型，增强识别将不可用");
        return;
    };
    let src = src_root.join("ner");
    let dst = models_dir.join("ner");
    if src == dst {
        return;
    }
    match copy_dir_all(&src, &dst) {
        Ok(()) => eprintln!(
            "[启动] 已补充增强识别模型：{} → {}",
            src.display(),
            dst.display()
        ),
        Err(e) => eprintln!("[启动] 复制增强识别模型失败：{e}"),
    }
}

fn has_ner_models(dir: &Path) -> bool {
    dir.join("ner").join("ner.onnx").is_file() && dir.join("ner").join("vocab.txt").is_file()
}

// ---------------------------------------------------------------------------
// pdfium 定位
// ---------------------------------------------------------------------------

/// 依次在以下位置查找 pdfium 动态库：
/// 1. 环境变量 `TUOMINXING_PDFIUM`（兼容旧名 `LEGAL_REDACT_PDFIUM`）
/// 2. 应用资源目录（打包后的 `resources/`）
/// 3. 可执行文件同级目录（便携版）
/// 4. 开发目录 `src-tauri/resources/`
fn locate_pdfium(app: &tauri::App) -> Option<PathBuf> {
    let file = if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    };

    let mut candidates: Vec<PathBuf> = Vec::new();
    for key in ["TUOMINXING_PDFIUM", "LEGAL_REDACT_PDFIUM"] {
        if let Ok(p) = std::env::var(key) {
            if !p.trim().is_empty() {
                candidates.push(PathBuf::from(p));
            }
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join("resources").join(file));
        candidates.push(res.join(file));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(file));
            candidates.push(dir.join("../Resources/resources").join(file));
            candidates.push(dir.join("../Resources").join(file));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(file),
    );

    candidates.into_iter().find(|p| p.is_file())
}
