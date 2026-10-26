//! Tauri 命令层（前后端唯一接口）
//!
//! 约定：
//! - 所有命令返回 `Result<T, AppError>`，前端 catch 到的是中文错误
//! - 耗时操作（解析、识别、导出、OCR）统一丢进 `spawn_blocking`，保持界面流畅
//! - 文档以 id 标识，全文只在导入时传一次，后续命令只传增量

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::detect::model_ner::{self, ModelNer};
use crate::detect::{DetectEngine, DetectionResult};
use crate::error::{AppError, Result};
use crate::export::{self, ExportJob};
use crate::models::*;
use crate::parser::{self, OcrProgress, ProgressFn};
use crate::redact;
use crate::state::{AppState, Ctx, DocumentSession};

// ---------------------------------------------------------------------------
// 配置 / 规则
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn app_paths(state: State<'_, AppState>) -> Result<AppPaths> {
    let config = state
        .ctx
        .config
        .lock()
        .map_err(|_| AppError::Other("配置锁异常".into()))?;
    Ok(config.paths.clone())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.ctx.settings())
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    let settings = settings.normalized();
    {
        let mut config = state
            .ctx
            .config
            .lock()
            .map_err(|_| AppError::Other("配置锁异常".into()))?;
        config.settings = settings.clone();
        config.save_settings()?;
    }
    state.ctx.clear_ocr_cache();
    Ok(settings)
}

#[tauri::command]
pub fn list_rules(state: State<'_, AppState>) -> Result<Vec<Rule>> {
    let config = state
        .ctx
        .config
        .lock()
        .map_err(|_| AppError::Other("配置锁异常".into()))?;
    Ok(config.rules.clone())
}

#[tauri::command]
pub fn save_rule(state: State<'_, AppState>, rule: Rule) -> Result<Vec<Rule>> {
    let mut config = state
        .ctx
        .config
        .lock()
        .map_err(|_| AppError::Other("配置锁异常".into()))?;
    config.upsert_rule(rule)?;
    Ok(config.rules.clone())
}

#[tauri::command]
pub fn delete_rule(state: State<'_, AppState>, id: String) -> Result<Vec<Rule>> {
    let mut config = state
        .ctx
        .config
        .lock()
        .map_err(|_| AppError::Other("配置锁异常".into()))?;
    config.delete_rule(&id)?;
    Ok(config.rules.clone())
}

#[tauri::command]
pub fn reset_rules(state: State<'_, AppState>) -> Result<Vec<Rule>> {
    let mut config = state
        .ctx
        .config
        .lock()
        .map_err(|_| AppError::Other("配置锁异常".into()))?;
    config.reset_rules()?;
    Ok(config.rules.clone())
}

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> Result<Vec<CategoryInfo>> {
    let config = state
        .ctx
        .config
        .lock()
        .map_err(|_| AppError::Other("配置锁异常".into()))?;
    Ok(config.categories())
}

#[tauri::command]
pub fn test_rule(rule: Rule, text: String) -> Result<RuleTestResult> {
    let mut probe = rule;
    probe.enabled = true;
    let mut matches = Vec::new();
    match probe.kind {
        RuleKind::Regex => {
            let re = regex::RegexBuilder::new(&probe.pattern)
                .case_insensitive(probe.case_insensitive)
                .build()
                .map_err(|e| AppError::Rule(format!("正则表达式不合法：{e}")))?;
            for caps in re.captures_iter(&text) {
                if let Some(m) = caps.get(probe.capture_group).or_else(|| caps.get(0)) {
                    matches.push(m.as_str().to_string());
                    if matches.len() >= 50 {
                        break;
                    }
                }
            }
        }
        RuleKind::Keyword => {
            let hay = if probe.case_insensitive {
                text.to_lowercase()
            } else {
                text.clone()
            };
            for kw in probe.keywords.iter().filter(|k| !k.trim().is_empty()) {
                let needle = if probe.case_insensitive {
                    kw.to_lowercase()
                } else {
                    kw.clone()
                };
                for _ in hay.match_indices(&needle) {
                    matches.push(kw.clone());
                    if matches.len() >= 50 {
                        break;
                    }
                }
            }
        }
        RuleKind::Ner => {
            for hit in crate::detect::ner::detect(&text) {
                matches.push(hit.name);
            }
        }
    }
    let count = matches.len();
    Ok(RuleTestResult {
        matches,
        count,
        error: None,
    })
}

#[tauri::command]
pub fn default_export_dir(app: AppHandle) -> Result<String> {
    let dir = app
        .path()
        .desktop_dir()
        .map_err(|e| AppError::Other(format!("无法定位桌面目录：{e}")))?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn enhance_status(state: State<'_, AppState>) -> EnhanceStatus {
    let dir = model_ner::model_dir(&state.ctx.models_dir);
    let ready = state.ctx.model_ner_ready();
    let message = if ready {
        "本地增强模型已就绪（约 12 MB，CPU 推理，首次启用会加载约 0.2–1 秒）".to_string()
    } else {
        format!("未找到增强识别模型：{}", dir.display())
    };
    EnhanceStatus {
        ready,
        message,
        model_dir: dir.display().to_string(),
    }
}

#[tauri::command]
pub fn ocr_status(state: State<'_, AppState>) -> Result<OcrStatus> {
    let settings = state.ctx.settings();
    Ok(crate::parser::ocr::status(&settings, &state.ctx.models_dir))
}

// ---------------------------------------------------------------------------
// 导入 / 读取
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn import_documents(
    state: State<'_, AppState>,
    paths: Vec<String>,
    app: AppHandle,
) -> Result<ImportOutcome> {
    let ctx = state.ctx.clone();
    let progress = ocr_progress_sink(app);
    tauri::async_runtime::spawn_blocking(move || {
        import_impl_with_progress(&ctx, paths, Some(progress))
    })
    .await
    .map_err(|e| AppError::Other(format!("导入任务异常：{e}")))?
}

/// 把 OCR 页级进度通过 Tauri 事件推给前端
fn ocr_progress_sink(app: AppHandle) -> ProgressFn {
    Arc::new(move |p: OcrProgress| {
        let _ = app.emit("ocr-progress", &p);
    })
}

fn import_impl_with_progress(
    ctx: &Ctx,
    paths: Vec<String>,
    progress: Option<ProgressFn>,
) -> Result<ImportOutcome> {
    let settings = ctx.settings();
    // 注意：先把规则克隆出来并释放配置锁，再调用 parse_options()。
    // parse_options 内部会再次读取 settings，若仍持有同一把 std::sync::Mutex 会死锁。
    let rules = {
        let config = ctx
            .config
            .lock()
            .map_err(|_| AppError::Other("配置锁异常".into()))?;
        config.rules.clone()
    };
    let mut opts = ctx.parse_options();
    opts.progress = progress;
    let engine = DetectEngine::build(&rules, settings.threshold, settings.propagate_names);
    let (enhance_model, model_warning) = load_enhance_model(ctx, &settings);

    let mut docs = Vec::new();
    let mut errors = Vec::new();

    for path_str in paths {
        let path = PathBuf::from(&path_str);
        if !path.is_file() {
            errors.push(format!("{path_str}：文件不存在"));
            continue;
        }
        // 已导入过的同一路径直接复用，避免重复解析
        if let Some(existing) = find_existing(ctx, &path) {
            docs.push(existing.summary());
            continue;
        }
        let t0 = std::time::Instant::now();
        eprintln!("[导入] 开始解析：{}", path.display());
        match build_session(
            &path,
            &opts,
            &engine,
            enhance_model.as_deref(),
            model_warning.as_deref(),
        ) {
            Ok(session) => {
                let summary = session.summary();
                eprintln!(
                    "[导入] 完成：{}（{:?}，{} 页，{} 字，{:?}）",
                    path.display(),
                    session.kind,
                    session.doc.page_count,
                    session.doc.text.chars().count(),
                    t0.elapsed()
                );
                docs.push(summary);
                let mut docs = ctx
                    .docs
                    .lock()
                    .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
                // 同一路径重新导入时移除旧会话，避免列表出现两份同名文件
                docs.retain(|_, s| s.file_path != path);
                docs.insert(session.id.clone(), session);
            }
            Err(e) => {
                eprintln!("[导入] 失败：{}：{e}", path.display());
                errors.push(format!("{}：{e}", file_name(&path)));
            }
        }
    }

    Ok(ImportOutcome { docs, errors })
}

/// 把跨段选区拆成「每段一个 [start, end)」，并过滤纯空白段
fn split_selection_ranges(chars: &[char], start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut segments: Vec<(usize, usize)> = Vec::new();
    let mut seg_start = start;
    for (offset, ch) in chars[start..end].iter().enumerate() {
        if *ch == '\n' {
            if offset > 0 {
                segments.push((seg_start, start + offset));
            }
            seg_start = start + offset + 1;
        }
    }
    if seg_start < end {
        segments.push((seg_start, end));
    }
    segments.retain(|(a, b)| chars[*a..*b].iter().any(|c| !c.is_whitespace()));
    segments
}

/// 合并识别结果：人工标记优先；自动结果与人工标记重叠时丢弃，避免清单重复
fn merge_manual_and_auto(manual: Vec<Finding>, auto: Vec<Finding>) -> Vec<Finding> {
    let mut merged = manual;
    for f in auto {
        if merged.iter().any(|m| m.start < f.end && f.start < m.end) {
            continue;
        }
        merged.push(f);
    }
    merged.sort_by_key(|f| f.start);
    merged
}

/// 按设置加载增强识别模型；失败时返回可展示的警告，不阻断导入
fn load_enhance_model(ctx: &Ctx, settings: &Settings) -> (Option<Arc<ModelNer>>, Option<String>) {
    if !settings.enhance_model {
        return (None, None);
    }
    match ctx.model_ner() {
        Ok(model) => (Some(model), None),
        Err(e) => (None, Some(format!("增强脱敏不可用：{e}"))),
    }
}

/// 规则识别 + 可选的本地模型识别，模型失败只降级为警告
fn detect_text(
    engine: &DetectEngine,
    text: &str,
    kind: DocKind,
    model: Option<&ModelNer>,
) -> DetectionResult {
    let mut warnings = Vec::new();
    let hits = if let Some(model) = model {
        match model.detect(text) {
            Ok(hits) => hits,
            Err(e) => {
                warnings.push(format!("增强脱敏模型推理失败：{e}"));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let mut result = engine.detect_with_model(text, kind, hits);
    result.warnings.extend(warnings);
    result
}

/// 构建会话并完成首次自动识别
fn build_session(
    path: &Path,
    opts: &parser::ParseOptions,
    engine: &DetectEngine,
    enhance_model: Option<&ModelNer>,
    model_warning: Option<&str>,
) -> Result<DocumentSession> {
    let size = fs::metadata(path)?.len();
    let sha256 = sha256_file(path);
    let parsed = parser::parse_file(path, opts)?;
    let detection = detect_text(engine, &parsed.doc.text, parsed.kind, enhance_model);

    let mut warnings = parsed.warnings.clone();
    if let Some(w) = model_warning {
        warnings.push(w.to_string());
    }
    warnings.extend(detection.warnings.iter().cloned());
    if size > 100 * 1024 * 1024 {
        warnings.push(format!(
            "文件较大（{:.0} MB），解析与 OCR 可能耗时较长；如只需部分页面，建议先拆分",
            size as f64 / 1024.0 / 1024.0
        ));
    }

    Ok(DocumentSession {
        id: uuid::Uuid::new_v4().to_string(),
        file_name: file_name(path),
        file_path: path.to_path_buf(),
        file_size: size,
        sha256,
        char_count: parsed.doc.text.chars().count(),
        kind: parsed.kind,
        doc: Arc::new(parsed.doc),
        findings: detection.findings,
        needs_ocr: parsed.needs_ocr,
        ocr_used: parsed.ocr_used,
        warnings,
        detected: true,
    })
}

fn find_existing(ctx: &Ctx, path: &Path) -> Option<DocumentSession> {
    // 先按「路径 + 大小」粗筛，避免为新文件白算一遍 SHA-256；
    // 大小相同再比对内容指纹，防止同名替换后复用旧识别结果。
    let size = fs::metadata(path).ok()?.len();
    {
        let docs = ctx.docs.lock().ok()?;
        if !docs
            .values()
            .any(|s| s.file_path == path && s.file_size == size)
        {
            return None;
        }
    }
    let hash = sha256_file(path);
    let docs = ctx.docs.lock().ok()?;
    docs.values()
        .find(|s| s.file_path == path && s.file_size == size && s.sha256 == hash)
        .map(|s| DocumentSession {
            id: s.id.clone(),
            file_name: s.file_name.clone(),
            file_path: s.file_path.clone(),
            file_size: s.file_size,
            sha256: s.sha256.clone(),
            char_count: s.char_count,
            kind: s.kind,
            doc: s.doc.clone(),
            findings: s.findings.clone(),
            needs_ocr: s.needs_ocr,
            ocr_used: s.ocr_used,
            warnings: s.warnings.clone(),
            detected: s.detected,
        })
}

#[tauri::command]
pub fn get_document(state: State<'_, AppState>, id: String) -> Result<DocPayload> {
    let docs = state
        .ctx
        .docs
        .lock()
        .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
    let s = docs.get(&id).ok_or(AppError::NoDocument)?;
    Ok(DocPayload {
        id: s.id.clone(),
        file_name: s.file_name.clone(),
        file_path: s.file_path.to_string_lossy().to_string(),
        kind: s.kind.id().to_string(),
        kind_label: s.kind.label().to_string(),
        page_count: s.doc.page_count,
        char_count: s.char_count,
        file_size: s.file_size,
        sha256: s.sha256.clone(),
        pages: s.doc.pages.clone(),
        text: s.doc.text.clone(),
        findings: s.findings.clone(),
        needs_ocr: s.needs_ocr,
        ocr_used: s.ocr_used,
        warnings: s.warnings.clone(),
    })
}

#[tauri::command]
pub async fn detect_document(state: State<'_, AppState>, id: String) -> Result<Vec<Finding>> {
    let ctx = state.ctx.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = ctx.settings();
        let rules = {
            let config = ctx
                .config
                .lock()
                .map_err(|_| AppError::Other("配置锁异常".into()))?;
            config.rules.clone()
        };
        let engine = DetectEngine::build(&rules, settings.threshold, settings.propagate_names);
        let (enhance_model, model_warning) = load_enhance_model(&ctx, &settings);
        let (text, kind) = {
            let docs = ctx
                .docs
                .lock()
                .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
            let s = docs.get(&id).ok_or(AppError::NoDocument)?;
            (s.doc.text.clone(), s.kind)
        };
        let result = detect_text(&engine, &text, kind, enhance_model.as_deref());
        let mut docs = ctx
            .docs
            .lock()
            .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
        let s = docs.get_mut(&id).ok_or(AppError::NoDocument)?;
        // 人工标记永远保留，自动识别结果整体替换
        let manual: Vec<Finding> = s
            .findings
            .drain(..)
            .filter(|f| f.source == FindingSource::Manual)
            .collect();
        s.findings = merge_manual_and_auto(manual, result.findings);
        s.detected = true;
        s.warnings
            .retain(|w| !w.starts_with("规则「") && !w.starts_with("增强脱敏"));
        if let Some(w) = model_warning {
            s.warnings.push(w);
        }
        s.warnings.extend(result.warnings);
        Ok(s.findings.clone())
    })
    .await
    .map_err(|e| AppError::Other(format!("识别任务异常：{e}")))?
}

#[tauri::command]
pub async fn redetect_all(state: State<'_, AppState>) -> Result<Vec<DocSummary>> {
    let ctx = state.ctx.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = ctx.settings();
        let rules = {
            let config = ctx
                .config
                .lock()
                .map_err(|_| AppError::Other("配置锁异常".into()))?;
            config.rules.clone()
        };
        let engine = DetectEngine::build(&rules, settings.threshold, settings.propagate_names);
        let (enhance_model, model_warning) = load_enhance_model(&ctx, &settings);

        // 先在锁外完成识别，避免大文档批量识别时阻塞预览/导出等命令
        let snapshots: Vec<(
            String,
            Arc<crate::models::ParsedDoc>,
            crate::models::DocKind,
        )> = {
            let docs = ctx
                .docs
                .lock()
                .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
            docs.values()
                .map(|s| (s.id.clone(), s.doc.clone(), s.kind))
                .collect()
        };
        let results: Vec<(String, crate::detect::DetectionResult)> = snapshots
            .into_iter()
            .map(|(id, doc, kind)| {
                let mut result = detect_text(&engine, &doc.text, kind, enhance_model.as_deref());
                if let Some(w) = &model_warning {
                    result.warnings.push(w.clone());
                }
                (id, result)
            })
            .collect();

        let mut docs = ctx
            .docs
            .lock()
            .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
        let mut summaries = Vec::new();
        for (id, result) in results {
            if let Some(s) = docs.get_mut(&id) {
                let manual: Vec<Finding> = s
                    .findings
                    .drain(..)
                    .filter(|f| f.source == FindingSource::Manual)
                    .collect();
                s.findings = merge_manual_and_auto(manual, result.findings);
                s.detected = true;
                s.warnings
                    .retain(|w| !w.starts_with("规则「") && !w.starts_with("增强脱敏"));
                s.warnings.extend(result.warnings);
                summaries.push(s.summary());
            }
        }
        Ok(summaries)
    })
    .await
    .map_err(|e| AppError::Other(format!("批量识别任务异常：{e}")))?
}

#[tauri::command]
pub async fn reparse_document(
    state: State<'_, AppState>,
    id: String,
    app: AppHandle,
) -> Result<Vec<Finding>> {
    let ctx = state.ctx.clone();
    let progress = ocr_progress_sink(app);
    tauri::async_runtime::spawn_blocking(move || {
        let settings = ctx.settings();
        let rules = {
            let config = ctx
                .config
                .lock()
                .map_err(|_| AppError::Other("配置锁异常".into()))?;
            config.rules.clone()
        };
        let mut opts = ctx.parse_options();
        opts.progress = Some(progress);
        let (enhance_model, model_warning) = load_enhance_model(&ctx, &settings);
        let path = {
            let docs = ctx
                .docs
                .lock()
                .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
            docs.get(&id).ok_or(AppError::NoDocument)?.file_path.clone()
        };
        let parsed = parser::parse_file(&path, &opts)?;
        let engine = DetectEngine::build(&rules, settings.threshold, settings.propagate_names);
        let result = detect_text(
            &engine,
            &parsed.doc.text,
            parsed.kind,
            enhance_model.as_deref(),
        );
        let mut docs = ctx
            .docs
            .lock()
            .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
        let s = docs.get_mut(&id).ok_or(AppError::NoDocument)?;
        s.char_count = parsed.doc.text.chars().count();
        s.doc = Arc::new(parsed.doc);
        s.kind = parsed.kind;
        s.needs_ocr = parsed.needs_ocr;
        s.ocr_used = parsed.ocr_used;
        s.warnings = parsed.warnings;
        if let Some(w) = model_warning {
            s.warnings.push(w);
        }
        s.warnings.extend(result.warnings);
        s.findings = result.findings;
        s.detected = true;
        Ok(s.findings.clone())
    })
    .await
    .map_err(|e| AppError::Other(format!("重新解析任务异常：{e}")))?
}

#[tauri::command]
pub fn remove_document(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut docs = state
        .ctx
        .docs
        .lock()
        .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
    docs.remove(&id);
    Ok(())
}

// ---------------------------------------------------------------------------
// 人工复核
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn update_finding(
    state: State<'_, AppState>,
    doc_id: String,
    finding: Finding,
) -> Result<Vec<Finding>> {
    let mut docs = state
        .ctx
        .docs
        .lock()
        .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
    let s = docs.get_mut(&doc_id).ok_or(AppError::NoDocument)?;
    let len = s.doc.text.chars().count();
    if finding.end > len || finding.start >= finding.end {
        return Err(AppError::Other("标记区间越界".into()));
    }
    if let Some(slot) = s.findings.iter_mut().find(|f| f.id == finding.id) {
        // 只允许修改“分类 / 脱敏方式 / 是否启用”，位置与原文始终以服务端为准，
        // 防止前端 bug 或手工调用把标记挪到错误位置造成漏脱敏。
        slot.enabled = finding.enabled;
        slot.category = finding.category;
        slot.category_label = finding.category_label;
        slot.strategy = finding.strategy;
    }
    s.findings.sort_by_key(|f| f.start);
    Ok(s.findings.clone())
}

#[tauri::command]
pub fn add_manual_finding(
    state: State<'_, AppState>,
    doc_id: String,
    start: usize,
    end: usize,
    category: String,
    category_label: String,
    strategy: StrategyConfig,
) -> Result<Vec<Finding>> {
    let mut docs = state
        .ctx
        .docs
        .lock()
        .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
    let s = docs.get_mut(&doc_id).ok_or(AppError::NoDocument)?;
    let chars: Vec<char> = s.doc.text.chars().collect();
    if start >= end || end > chars.len() {
        return Err(AppError::Other("选区无效，请重新框选".into()));
    }

    // 跨段选择（含换行）拆成每段一个标记：
    // 否则 Word 导出按段落改写时，同一处会被重复替换成两个占位标签。
    let segments = split_selection_ranges(&chars, start, end);
    if segments.is_empty() {
        return Err(AppError::Other("选区内没有文字".into()));
    }

    // 手动标记覆盖已有自动标记：先移除与整个选区重叠的旧标记
    s.findings.retain(|f| !(f.start < end && start < f.end));
    for (a, b) in segments {
        let text: String = chars[a..b].iter().collect();
        s.findings.push(Finding {
            id: uuid::Uuid::new_v4().to_string(),
            start: a,
            end: b,
            text,
            category: category.clone(),
            category_label: category_label.clone(),
            confidence: 1.0,
            source: FindingSource::Manual,
            rule_id: None,
            rule_name: "人工标记".into(),
            enabled: true,
            strategy: strategy.clone(),
        });
    }
    s.findings.sort_by_key(|f| f.start);
    Ok(s.findings.clone())
}

#[tauri::command]
pub fn remove_finding(
    state: State<'_, AppState>,
    doc_id: String,
    finding_id: String,
) -> Result<Vec<Finding>> {
    let mut docs = state
        .ctx
        .docs
        .lock()
        .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
    let s = docs.get_mut(&doc_id).ok_or(AppError::NoDocument)?;
    s.findings.retain(|f| f.id != finding_id);
    Ok(s.findings.clone())
}

#[tauri::command]
pub fn set_all_enabled(
    state: State<'_, AppState>,
    doc_id: String,
    enabled: bool,
) -> Result<Vec<Finding>> {
    let mut docs = state
        .ctx
        .docs
        .lock()
        .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
    let s = docs.get_mut(&doc_id).ok_or(AppError::NoDocument)?;
    for f in s.findings.iter_mut() {
        f.enabled = enabled;
    }
    Ok(s.findings.clone())
}

#[tauri::command]
pub fn redact_preview(state: State<'_, AppState>, doc_id: String) -> Result<RedactionPreview> {
    let docs = state
        .ctx
        .docs
        .lock()
        .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
    let s = docs.get(&doc_id).ok_or(AppError::NoDocument)?;
    let out = redact::redact_text(&s.doc.text, &s.findings);
    let total = out.total();
    Ok(RedactionPreview {
        text: out.text,
        segments: out.segments,
        total,
        by_category: out.by_category,
    })
}

// ---------------------------------------------------------------------------
// 导出
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub doc_ids: Vec<String>,
    pub out_dir: String,
    #[serde(default)]
    pub formats: Vec<String>,
    /// 仅导出每份文档的原格式（PDF→PDF、Word→Word、TXT→TXT，
    /// RTF/OFD/旧版 Word 转 Word），默认由前端打开
    #[serde(default)]
    pub native_only: bool,
    #[serde(default = "default_true")]
    pub write_audit: bool,
}

fn default_true() -> bool {
    true
}

/// 原格式导出时的目标格式：只有 PDF/TXT 能原样生成；
/// Word/RTF/HTML/OFD/旧版 Office 统一导出为可编辑的 .docx。
fn native_export_format(kind: DocKind) -> &'static str {
    match kind {
        DocKind::PdfText | DocKind::PdfScan => "pdf",
        DocKind::Txt => "txt",
        DocKind::Docx | DocKind::DocLegacy | DocKind::Rtf | DocKind::Html | DocKind::Ofd => "docx",
    }
}

#[tauri::command]
pub async fn export_documents(
    state: State<'_, AppState>,
    req: ExportRequest,
) -> Result<Vec<ExportOutcome>> {
    let ctx = state.ctx.clone();
    tauri::async_runtime::spawn_blocking(move || export_impl(&ctx, req))
        .await
        .map_err(|e| AppError::Other(format!("导出任务异常：{e}")))?
}

fn export_impl(ctx: &Ctx, req: ExportRequest) -> Result<Vec<ExportOutcome>> {
    if req.doc_ids.is_empty() {
        return Err(AppError::Export("请先选择要导出的文档".into()));
    }
    if !req.native_only && req.formats.is_empty() {
        return Err(AppError::Export("请至少选择一种导出格式".into()));
    }
    let out_dir = PathBuf::from(&req.out_dir);
    if req.out_dir.trim().is_empty() {
        return Err(AppError::Export("请选择导出目录".into()));
    }
    fs::create_dir_all(&out_dir)?;

    let settings = ctx.settings();
    let pdfium_path = ctx.pdfium_path.clone();
    let mut outcomes = Vec::new();

    for id in &req.doc_ids {
        let snapshot = {
            let docs = ctx
                .docs
                .lock()
                .map_err(|_| AppError::Other("文档缓存锁异常".into()))?;
            docs.get(id).map(|s| {
                (
                    s.doc.clone(),
                    s.findings.clone(),
                    s.kind,
                    s.file_name.clone(),
                    s.file_path.clone(),
                    s.file_size,
                    s.sha256.clone(),
                )
            })
        };
        let Some((doc, findings, kind, file_name, source_path, file_size, sha256)) = snapshot
        else {
            outcomes.push(ExportOutcome {
                doc_id: id.clone(),
                file_name: String::new(),
                files: vec![],
                audit_path: String::new(),
                warnings: vec!["文档已不在列表中".into()],
                total: 0,
                error: Some("文档已不在列表中".into()),
            });
            continue;
        };

        // 默认每份文档只导出它自己的原格式，避免选 PDF 却额外生成 TXT/Word
        let native = vec![native_export_format(kind).to_string()];
        let formats: &[String] = if req.native_only {
            &native
        } else {
            &req.formats
        };
        let job = ExportJob {
            doc: &doc,
            findings: &findings,
            source_path: &source_path,
            source_kind: kind,
            file_name: &file_name,
            file_size,
            sha256: &sha256,
            out_dir: &out_dir,
            formats,
            pdfium_path: pdfium_path.clone(),
            pdf_dpi: settings.pdf_export_dpi,
            write_audit: req.write_audit,
        };

        match export::export(&job) {
            Ok(result) => {
                let mut files = result.files;
                files.extend(result.audit_files);
                let audit_path = files
                    .iter()
                    .find(|f| f.label.contains("日志"))
                    .map(|f| f.path.clone())
                    .unwrap_or_default();
                outcomes.push(ExportOutcome {
                    doc_id: id.clone(),
                    file_name,
                    files,
                    audit_path,
                    warnings: result.warnings,
                    total: result.total,
                    error: None,
                });
            }
            Err(e) => outcomes.push(ExportOutcome {
                doc_id: id.clone(),
                file_name,
                files: vec![],
                audit_path: String::new(),
                warnings: vec![],
                total: 0,
                error: Some(e.to_string()),
            }),
        }
    }

    // 记住上次导出目录
    if outcomes.iter().any(|o| o.error.is_none()) {
        if let Ok(mut config) = ctx.config.lock() {
            config.settings.last_output_dir = req.out_dir.clone();
            let _ = config.save_settings();
        }
    }

    Ok(outcomes)
}

// ---------------------------------------------------------------------------
// 系统集成
// ---------------------------------------------------------------------------

/// 在系统文件管理器中打开文件/目录（不依赖 shell 插件，避免权限配置负担）
#[tauri::command]
pub fn open_path(path: String) -> Result<()> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(AppError::Other("路径不存在".into()));
    }
    open_in_file_manager(p)
}

#[cfg(target_os = "windows")]
fn open_in_file_manager(path: &Path) -> Result<()> {
    // explorer.exe 即使成功也常返回非 0，这里只判断能否启动
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map_err(|e| AppError::Other(format!("调用资源管理器失败：{e}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_in_file_manager(path: &Path) -> Result<()> {
    let status = std::process::Command::new("open").arg(path).status();
    status
        .map_err(|e| AppError::Other(format!("调用 Finder 失败：{e}")))?
        .success()
        .then_some(())
        .ok_or_else(|| AppError::Other("Finder 返回失败".into()))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_in_file_manager(path: &Path) -> Result<()> {
    let status = std::process::Command::new("xdg-open").arg(path).status();
    status
        .map_err(|e| AppError::Other(format!("调用文件管理器失败：{e}")))?
        .success()
        .then_some(())
        .ok_or_else(|| AppError::Other("文件管理器返回失败".into()))
}

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn sha256_file(path: &Path) -> String {
    let mut hasher = Sha256::new();
    if let Ok(mut f) = fs::File::open(path) {
        let mut buf = [0u8; 64 * 1024];
        while let Ok(n) = f.read(&mut buf) {
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn split_multiline_selection() {
        let chars: Vec<char> = "第一行\n\n第二行".chars().collect();
        let ranges = split_selection_ranges(&chars, 0, chars.len());
        assert_eq!(ranges.len(), 2, "空行应被丢弃");
        assert_eq!(ranges[0], (0, 3));
        let seg: String = chars[ranges[1].0..ranges[1].1].iter().collect();
        assert_eq!(seg, "第二行");
    }

    #[test]
    fn manual_mark_wins_over_auto() {
        let mk = |start: usize, end: usize, source: FindingSource| Finding {
            id: format!("{start}-{end}"),
            start,
            end,
            text: "x".into(),
            category: "person_name".into(),
            category_label: "姓名".into(),
            confidence: 1.0,
            source,
            rule_id: None,
            rule_name: "t".into(),
            enabled: true,
            strategy: StrategyConfig::default(),
        };
        let merged = merge_manual_and_auto(
            vec![mk(0, 2, FindingSource::Manual)],
            vec![
                mk(1, 3, FindingSource::Regex),
                mk(5, 6, FindingSource::Regex),
            ],
        );
        assert_eq!(merged.len(), 2, "与人工标记重叠的自动结果应被丢弃");
    }

    #[test]
    fn enhanced_import_adds_model_findings() {
        let dir = std::env::temp_dir().join(format!(
            "luminar_enhance_test_{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = dir.join("sample.txt");
        std::fs::write(
            &sample,
            "原告：王小明，住北京市朝阳区建国路88号SOHO现代城。被告：华为技术有限公司。",
        )
        .unwrap();

        let mut config = crate::config::ConfigStore::init(dir.clone()).expect("初始化配置失败");
        config.settings.enhance_model = true;
        config.settings.threshold = 0.75;
        let ctx = Ctx {
            config: Arc::new(std::sync::Mutex::new(config)),
            docs: Arc::new(std::sync::Mutex::new(Default::default())),
            ocr_cache: Arc::new(std::sync::Mutex::new(None)),
            model_ner_cache: Arc::new(std::sync::Mutex::new(None)),
            models_dir: std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/models"),
            pdfium_path: None,
        };
        let out = import_impl_with_progress(&ctx, vec![sample.to_string_lossy().to_string()], None)
            .expect("导入失败");
        assert!(out.errors.is_empty(), "导入错误：{:?}", out.errors);
        let id = out.docs[0].id.clone();
        let docs = ctx.docs.lock().unwrap();
        let session = docs.get(&id).expect("缺少会话");
        assert!(
            session.findings.iter().any(|f| f.category == "org_name"),
            "增强识别应补充单位名称：{:?}",
            session
                .findings
                .iter()
                .map(|f| (&f.text, &f.category))
                .collect::<Vec<_>>()
        );
        assert!(
            !session
                .warnings
                .iter()
                .any(|w| w.contains("增强识别不可用")),
            "模型加载不应失败：{:?}",
            session.warnings
        );
    }

    #[test]
    fn native_only_export_ignores_extra_formats() {
        let dir = std::env::temp_dir().join(format!(
            "luminar_native_export_{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = dir.join("sample.txt");
        std::fs::write(&sample, "原告：张三，电话13812345678。").unwrap();
        let config = crate::config::ConfigStore::init(dir.clone()).expect("初始化配置失败");
        let ctx = Ctx {
            config: Arc::new(std::sync::Mutex::new(config)),
            docs: Arc::new(std::sync::Mutex::new(Default::default())),
            ocr_cache: Arc::new(std::sync::Mutex::new(None)),
            model_ner_cache: Arc::new(std::sync::Mutex::new(None)),
            models_dir: dir.join("models"),
            pdfium_path: None,
        };
        let out = import_impl_with_progress(&ctx, vec![sample.to_string_lossy().to_string()], None)
            .expect("导入失败");
        let id = out.docs[0].id.clone();
        let out_dir = dir.join("export");
        let result = export_impl(
            &ctx,
            ExportRequest {
                doc_ids: vec![id],
                out_dir: out_dir.to_string_lossy().to_string(),
                formats: vec!["txt".into(), "docx".into(), "pdf".into()],
                native_only: true,
                write_audit: false,
            },
        )
        .expect("导出失败");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].files.len(), 1, "原格式模式应只生成一个文件");
        assert!(result[0].files[0].path.ends_with(".txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn native_export_formats_match_source() {
        assert_eq!(native_export_format(DocKind::PdfText), "pdf");
        assert_eq!(native_export_format(DocKind::PdfScan), "pdf");
        assert_eq!(native_export_format(DocKind::Docx), "docx");
        assert_eq!(native_export_format(DocKind::Txt), "txt");
        assert_eq!(native_export_format(DocKind::Rtf), "docx");
        assert_eq!(native_export_format(DocKind::Ofd), "docx");
        assert_eq!(native_export_format(DocKind::DocLegacy), "docx");
    }

    /// 回归测试：导入流程曾在持有配置锁时再次读取配置，导致永久死锁。
    #[test]
    fn import_does_not_deadlock() {
        let dir = std::env::temp_dir().join(format!(
            "luminar_import_test_{}",
            uuid::Uuid::new_v4().simple()
        ));
        let config = crate::config::ConfigStore::init(dir).expect("初始化配置失败");
        let ctx = Ctx {
            config: Arc::new(std::sync::Mutex::new(config)),
            docs: Arc::new(std::sync::Mutex::new(Default::default())),
            ocr_cache: Arc::new(std::sync::Mutex::new(None)),
            model_ner_cache: Arc::new(std::sync::Mutex::new(None)),
            models_dir: std::env::temp_dir().join("luminar_no_models"),
            pdfium_path: None,
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/民事起诉状.docx");
        assert!(path.is_file(), "缺少测试样例：{}", path.display());

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let out =
                import_impl_with_progress(&ctx, vec![path.to_string_lossy().to_string()], None);
            let _ = tx.send(out.map(|o| o.docs.len()));
        });
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(n)) => assert_eq!(n, 1),
            Ok(Err(e)) => panic!("导入失败：{e}"),
            Err(_) => panic!("导入超时：疑似死锁"),
        }
    }
}
