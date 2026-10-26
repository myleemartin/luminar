//! 命令行流水线验证工具（开发/排障用，不参与打包）
//!
//! 用法：
//! ```bash
//! cargo run --example pipeline -- ../tests/fixtures/民事起诉状.docx /tmp/out
//! TUOMINXING_MODELS=../models cargo run --example pipeline -- ../tests/fixtures/民事起诉状-扫描件.pdf /tmp/out
//! ```
//!
//! 输出：解析结果、命中清单、脱敏后文本片段、导出文件列表与合规日志路径。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use luminar_lib::config::ConfigStore;
use luminar_lib::detect::DetectEngine;
use luminar_lib::export::{self, ExportJob};
use luminar_lib::parser::{self, ParseOptions};
use luminar_lib::redact;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("用法：pipeline <文书路径> <输出目录>");
        std::process::exit(2);
    }
    let file = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    if let Err(e) = run(&file, &out_dir) {
        eprintln!("失败：{e}");
        std::process::exit(1);
    }
}

fn run(file: &Path, out_dir: &Path) -> Result<(), String> {
    // 临时配置目录，避免污染用户真实配置
    let config_dir = std::env::temp_dir().join("luminar_cli");
    let config = ConfigStore::init(config_dir).map_err(|e| e.to_string())?;

    let pdfium_name = if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    };
    let pdfium = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(pdfium_name);
    let models = std::env::var("TUOMINXING_MODELS").ok().map(PathBuf::from);

    let ocr = match &models {
        Some(dir) => match luminar_lib::parser::ocr::build_engine(&config.settings, dir) {
            Ok(engine) => {
                println!("OCR 引擎：{}", engine.label());
                Some(engine)
            }
            Err(e) => {
                println!("OCR 不可用：{e}");
                None
            }
        },
        None => None,
    };

    let opts = ParseOptions {
        ocr,
        ocr_provider: None,
        ocr_dpi: 200,
        pdfium_path: pdfium.is_file().then_some(pdfium),
        progress: None,
    };

    let t0 = std::time::Instant::now();
    let parsed = parser::parse_file(file, &opts).map_err(|e| e.to_string())?;
    println!(
        "解析：{:?}，{} 页，{} 字，耗时 {:?}",
        parsed.kind,
        parsed.doc.page_count,
        parsed.doc.text.chars().count(),
        t0.elapsed()
    );
    for w in &parsed.warnings {
        println!("  提示：{w}");
    }

    let engine = DetectEngine::build(&config.rules, 0.8, true);
    let t1 = std::time::Instant::now();
    let detection = engine.detect_for_kind(&parsed.doc.text, parsed.kind);
    println!(
        "识别：{} 条，耗时 {:?}",
        detection.findings.len(),
        t1.elapsed()
    );
    for f in detection.findings.iter().take(30) {
        println!(
            "  [{:>5}..{:<5}] {:<10} {:.2} {:<8} {}",
            f.start,
            f.end,
            f.category,
            f.confidence,
            f.source.id(),
            f.text.replace('\n', " ")
        );
    }

    let red = redact::redact_text(&parsed.doc.text, &detection.findings);
    let preview: String = red.text.chars().take(300).collect();
    println!("脱敏预览：\n{preview}\n…（共 {} 处）", red.total());

    let size = std::fs::metadata(file).map_err(|e| e.to_string())?.len();
    let mut hasher = Sha256::new();
    hasher.update(std::fs::read(file).map_err(|e| e.to_string())?);
    let sha = format!("{:x}", hasher.finalize());

    let formats: Vec<String> = vec!["txt".into(), "docx".into(), "pdf".into()];
    let file_name = file
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let job = ExportJob {
        doc: &parsed.doc,
        findings: &detection.findings,
        source_path: file,
        source_kind: parsed.kind,
        file_name: &file_name,
        file_size: size,
        sha256: &sha,
        out_dir,
        formats: &formats,
        pdfium_path: opts.pdfium_path.clone(),
        pdf_dpi: 150,
        write_audit: true,
    };
    let t2 = std::time::Instant::now();
    let result = export::export(&job).map_err(|e| e.to_string())?;
    println!(
        "导出：{} 个文件，耗时 {:?}",
        result.files.len() + result.audit_files.len(),
        t2.elapsed()
    );
    for f in result.files.iter().chain(result.audit_files.iter()) {
        println!("  {} ({} 字节)", f.path, f.bytes);
    }
    for w in &result.warnings {
        println!("  提示：{w}");
    }
    Ok(())
}
