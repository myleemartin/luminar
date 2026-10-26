//! 端到端集成测试：解析 → 识别 → 脱敏 → 导出
//!
//! 依赖 `tests/fixtures/` 下的样例文书（由 `scripts/make_fixtures.py` 生成，全部虚构）。
//! 运行：`cargo test --test pipeline_test -- --nocapture`

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use luminar_lib::detect::builtin_rules::builtin_rules;
use luminar_lib::detect::DetectEngine;
use luminar_lib::export::{self, ExportJob};
use luminar_lib::models::DocKind;
use luminar_lib::parser::{self, ParseOptions};
use luminar_lib::redact;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(name)
}

fn pdfium() -> Option<PathBuf> {
    let name = if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    };
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(name);
    p.is_file().then_some(p)
}

fn tmpdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "luminar_test_{tag}_{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn engine() -> DetectEngine {
    DetectEngine::build(&builtin_rules(), 0.75, true)
}

#[test]
fn docx_pipeline_end_to_end() {
    let path = fixture("民事起诉状.docx");
    assert!(path.is_file(), "缺少测试样例：{}", path.display());

    let parsed = parser::parse_file(
        &path,
        &ParseOptions {
            pdfium_path: pdfium(),
            ..Default::default()
        },
    )
    .expect("解析 docx 失败");
    assert_eq!(parsed.kind, DocKind::Docx);
    assert!(parsed.doc.text.contains("原告"));
    assert!(parsed.doc.text.contains("420102198801011234"));

    let detection = engine().detect_for_kind(&parsed.doc.text, parsed.kind);
    let cats: Vec<&str> = detection
        .findings
        .iter()
        .map(|f| f.category.as_str())
        .collect();
    for must in [
        "id_card",
        "phone",
        "address",
        "case_number",
        "court",
        "person_name",
    ] {
        assert!(cats.contains(&must), "未识别出 {must}，实际：{cats:?}");
    }

    let out = tmpdir("docx");
    let formats = vec!["txt".to_string(), "docx".to_string()];
    let job = ExportJob {
        doc: &parsed.doc,
        findings: &detection.findings,
        source_path: &path,
        source_kind: DocKind::Docx,
        file_name: "民事起诉状.docx",
        file_size: std::fs::metadata(&path).unwrap().len(),
        sha256: "test",
        out_dir: &out,
        formats: &formats,
        pdfium_path: pdfium(),
        pdf_dpi: 150,
        write_audit: true,
    };
    let result = export::export(&job).expect("导出失败");
    assert_eq!(result.files.len(), 2);
    assert_eq!(result.audit_files.len(), 2);
    assert!(result.total > 5);

    // TXT 中不应再出现完整身份证号
    let txt_path = result
        .files
        .iter()
        .find(|f| f.path.ends_with(".txt"))
        .expect("缺少 txt");
    let txt = std::fs::read_to_string(&txt_path.path).unwrap();
    assert!(!txt.contains("420102198801011234"), "身份证号没有脱敏");
    assert!(txt.contains("【"), "占位标签缺失");

    // Word 中同样不应残留原文
    let docx_path = result
        .files
        .iter()
        .find(|f| f.path.ends_with(".docx"))
        .expect("缺少 docx");
    let file = std::fs::File::open(&docx_path.path).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").unwrap(), &mut xml)
        .unwrap();
    assert!(!xml.contains("420102198801011234"), "docx 中残留身份证号");
    assert!(xml.contains("w:t"), "docx 结构异常");

    // 日志应包含每条记录
    let json_path = result
        .audit_files
        .iter()
        .find(|f| f.path.ends_with(".json"))
        .expect("缺少 json 日志");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json_path.path).unwrap()).unwrap();
    assert_eq!(json["total"], result.total);
    assert!(json["entries"].as_array().unwrap().len() > 5);
}

#[test]
fn pdf_text_and_redacted_pdf() {
    let Some(pdfium_path) = pdfium() else {
        eprintln!("跳过：未找到 libpdfium.dylib");
        return;
    };
    let path = fixture("民事起诉状.pdf");
    assert!(path.is_file());

    let parsed = parser::parse_file(
        &path,
        &ParseOptions {
            pdfium_path: Some(pdfium_path.clone()),
            ..Default::default()
        },
    )
    .expect("解析 PDF 失败");
    assert_eq!(parsed.kind, DocKind::PdfText, "应识别为文字版 PDF");
    assert!(parsed.doc.text.contains("张三"), "PDF 文本抽取失败");

    let detection = engine().detect_for_kind(&parsed.doc.text, parsed.kind);
    assert!(detection.findings.iter().any(|f| f.category == "id_card"));

    let out = tmpdir("pdf");
    let formats = vec!["pdf".to_string(), "txt".to_string()];
    let job = ExportJob {
        doc: &parsed.doc,
        findings: &detection.findings,
        source_path: &path,
        source_kind: parsed.kind,
        file_name: "民事起诉状.pdf",
        file_size: std::fs::metadata(&path).unwrap().len(),
        sha256: "test",
        out_dir: &out,
        formats: &formats,
        pdfium_path: Some(pdfium_path),
        pdf_dpi: 120,
        write_audit: false,
    };
    let result = export::export(&job).expect("导出 PDF 失败");
    let pdf_out = result
        .files
        .iter()
        .find(|f| f.path.ends_with(".pdf"))
        .expect("缺少脱敏 PDF");
    let bytes = std::fs::read(&pdf_out.path).unwrap();
    assert!(bytes.starts_with(b"%PDF"), "输出不是 PDF");
    let doc = lopdf::Document::load(&pdf_out.path).unwrap();
    assert_eq!(doc.get_pages().len(), 1);
}

#[test]
fn scanned_pdf_uses_ocr_when_models_available() {
    let Some(models) = std::env::var("TUOMINXING_MODELS").ok().map(PathBuf::from) else {
        eprintln!("跳过：未设置 TUOMINXING_MODELS");
        return;
    };
    let Some(pdfium_path) = pdfium() else {
        eprintln!("跳过：未找到 libpdfium.dylib");
        return;
    };
    let settings = luminar_lib::models::Settings::default();
    let engine =
        luminar_lib::parser::ocr::build_engine(&settings, &models).expect("OCR 引擎加载失败");

    let path = fixture("民事起诉状-扫描件.pdf");
    let events: Arc<Mutex<Vec<luminar_lib::parser::OcrProgress>>> =
        Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let parsed = parser::parse_file(
        &path,
        &ParseOptions {
            ocr: Some(engine),
            ocr_provider: None,
            ocr_dpi: 200,
            pdfium_path: Some(pdfium_path),
            progress: Some(Arc::new(move |p| {
                sink.lock().unwrap().push(p);
            })),
        },
    )
    .expect("扫描件解析失败");

    assert!(parsed.ocr_used, "未走 OCR 流程");
    assert!(
        parsed.doc.text.contains("原告"),
        "OCR 文本异常：{}",
        parsed.doc.text
    );
    let detection = engine_detect(&parsed.doc.text);
    assert!(
        detection.iter().any(|c| c == "id_card"),
        "OCR 后未识别身份证：{detection:?}"
    );

    let events = events.lock().unwrap();
    assert!(!events.is_empty(), "应产生 OCR 进度事件");
    assert_eq!(events.first().unwrap().phase, "loading");
    let last = events.last().unwrap();
    assert_eq!(last.phase, "done", "最后一帧应为完成");
    assert_eq!(last.current, last.total);
    assert!(last.total >= 1);
    assert!(events.iter().any(|e| e.phase == "page"), "应有逐页进度");
}

fn engine_detect(text: &str) -> Vec<String> {
    engine()
        .detect(text)
        .findings
        .iter()
        .map(|f| f.category.clone())
        .collect()
}

#[test]
fn redaction_strategies() {
    use luminar_lib::models::{Finding, FindingSource, StrategyConfig, StrategyKind};
    let text = "张三的电话是13812345678";
    let findings = vec![
        Finding {
            id: "1".into(),
            start: 0,
            end: 2,
            text: "张三".into(),
            category: "person_name".into(),
            category_label: "当事人姓名".into(),
            confidence: 0.9,
            source: FindingSource::Manual,
            rule_id: None,
            rule_name: "测试".into(),
            enabled: true,
            strategy: StrategyConfig {
                kind: StrategyKind::Placeholder,
                mask_char: "*".into(),
                keep_prefix: 0,
                keep_suffix: 0,
                placeholder: "【{type}】".into(),
            },
        },
        Finding {
            id: "2".into(),
            start: 6,
            end: 17,
            text: "13812345678".into(),
            category: "phone".into(),
            category_label: "手机号".into(),
            confidence: 0.9,
            source: FindingSource::Manual,
            rule_id: None,
            rule_name: "测试".into(),
            enabled: true,
            strategy: StrategyConfig {
                kind: StrategyKind::Mask,
                mask_char: "*".into(),
                keep_prefix: 3,
                keep_suffix: 4,
                placeholder: String::new(),
            },
        },
    ];
    let out = redact::redact_text(text, &findings);
    assert_eq!(out.text, "【当事人姓名】的电话是138****5678");
    assert_eq!(out.total(), 2);
}

#[test]
fn additional_legal_formats_are_parsed() {
    let dir = std::env::temp_dir().join(format!(
        "luminar_formats_{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    // RTF：Unicode + 段落
    let rtf = dir.join("a.rtf");
    std::fs::write(
        &rtf,
        r"{\rtf1\ansi\ansicpg936 {\fonttbl{\f0 SimSun;}}\f0 \u21407?\u21578?：\u24352?\u19977?\par 合同纠纷}",
    )
    .unwrap();
    let out = parser::parse_file(&rtf, &ParseOptions::default()).expect("RTF 解析失败");
    assert_eq!(out.kind, DocKind::Rtf);
    assert!(out.doc.text.contains("张三"), "{}", out.doc.text);

    // HTML：去脚本/样式 + 实体
    let html = dir.join("a.html");
    std::fs::write(
        &html,
        "<html><head><style>p{color:red}</style></head><body><h1>民事起诉状</h1><p>原告：张三&amp;李四</p><script>alert(1)</script></body></html>",
    )
    .unwrap();
    let out = parser::parse_file(&html, &ParseOptions::default()).expect("HTML 解析失败");
    assert_eq!(out.kind, DocKind::Html);
    assert!(out.doc.text.contains("民事起诉状"));
    assert!(out.doc.text.contains("张三&李四"));
    assert!(!out.doc.text.contains("alert"));

    // 实际是 OOXML 的 .wps：应按 zip 识别，不依赖 LibreOffice
    let wps = dir.join("a.wps");
    std::fs::copy(fixture("民事起诉状.docx"), &wps).unwrap();
    let out = parser::parse_file(&wps, &ParseOptions::default()).expect("OOXML WPS 解析失败");
    assert_eq!(out.kind, DocKind::DocLegacy);
    assert!(out.doc.text.contains("原告"));

    let _ = std::fs::remove_dir_all(&dir);
}
