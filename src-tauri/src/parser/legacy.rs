//! 旧版 Word / WPS（.doc / .wps）兼容层
//!
//! `.doc` / `.wps` 是二进制或私有格式，Rust 生态没有稳定的纯 Rust 解析器。
//! 这里优先调用本机已安装的 **LibreOffice** 无头转换：
//!   soffice --headless --convert-to docx --outdir <tmp> <file>
//! 转换后复用现有的 DOCX 解析（保留段落、可导出 Word）。
//!
//! macOS 上若没有 LibreOffice，会尝试系统自带的 `textutil` 提取纯文本（.doc）。
//! 两者都没有时给出明确提示，绝不假装成功。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::error::{AppError, Result};
use crate::models::{DocKind, ParsedDoc};
use crate::parser::{self, ParseOutput};

/// 转换/等待上限：LibreOffice 首次启动可能较慢
const CONVERT_TIMEOUT: Duration = Duration::from_secs(180);
const CHARS_PER_PAGE: usize = 3500;

pub fn parse(path: &Path) -> Result<ParseOutput> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // 先尝试原生 Word 97-2003 二进制解析：大多数 .doc 不需要 LibreOffice，
    // 也不依赖 Windows/macOS 外部程序；失败再走转换器。
    if ext == "doc" || ext == "wps" {
        if let Ok(out) = crate::parser::doc_binary::parse(path) {
            if !out.doc.text.trim().is_empty() {
                return Ok(out);
            }
        }
    }

    // 有些 WPS 文件实际是 OOXML（zip）改名，直接按 docx 解析，避免不必要的外部转换
    if is_zip(path) {
        let mut out = crate::parser::docx::parse(path)?;
        out.kind = DocKind::DocLegacy;
        out.warnings
            .push("该文件实际是 OOXML（zip）格式，已按 Word 解析；导出 Word 时会重新生成。".into());
        return Ok(out);
    }

    if let Some(soffice) = find_soffice() {
        match convert_with_soffice(&soffice, path) {
            Ok(out) => return Ok(out),
            Err(e) => {
                // 转换失败时，macOS 还有 textutil 兜底；否则把 LibreOffice 的错误带回去
                if cfg!(target_os = "macos") && ext == "doc" {
                    if let Ok(out) = convert_with_textutil(path) {
                        return Ok(out);
                    }
                }
                return Err(e);
            }
        }
    }

    if cfg!(target_os = "macos") && ext == "doc" {
        return convert_with_textutil(path);
    }

    Err(AppError::Unsupported(format!(
        "旧版 .{ext} 需要本机安装 LibreOffice 才能解析。\
         请安装 LibreOffice 后重试，或在 Word/WPS 中「另存为」.docx 再导入。"
    )))
}

fn is_zip(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).is_ok() && magic == [b'P', b'K', 3, 4]
}

fn find_soffice() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE.get_or_init(detect_soffice).clone()
}

fn detect_soffice() -> Option<PathBuf> {
    for key in ["TUOMINXING_SOFFICE", "LEGAL_REDACT_SOFFICE"] {
        if let Ok(p) = std::env::var(key) {
            if !p.trim().is_empty() {
                let path = PathBuf::from(p);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }

    // PATH 中的命令：直接试运行 --version（比手写 which 更可靠）
    for name in ["soffice", "libreoffice"] {
        if Command::new(name)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Some(PathBuf::from(name));
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        candidates.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
    }
    if cfg!(target_os = "windows") {
        candidates.push(r"C:\Program Files\LibreOffice\program\soffice.exe".into());
        candidates.push(r"C:\Program Files (x86)\LibreOffice\program\soffice.exe".into());
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn convert_with_soffice(soffice: &Path, source: &Path) -> Result<ParseOutput> {
    let dir = std::env::temp_dir().join(format!(
        "luminar_convert_{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&dir)?;
    let result = (|| -> Result<ParseOutput> {
        let mut child = Command::new(soffice)
            .arg("--headless")
            .arg("--norestore")
            .arg("--convert-to")
            .arg("docx")
            .arg("--outdir")
            .arg(&dir)
            .arg(source)
            // 不读取输出管道，避免 LibreOffice 日志把 pipe 写满导致死锁；
            // 失败时只返回退出状态，足够提示用户。
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AppError::Other(format!("启动 LibreOffice 失败：{e}")))?;

        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(status)) => {
                    return Err(AppError::Other(format!(
                        "LibreOffice 转换失败（{status}）；请确认文件未损坏，或另存为 .docx 后重试"
                    )));
                }
                Ok(None) => {
                    if started.elapsed() > CONVERT_TIMEOUT {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(AppError::Other(
                            "LibreOffice 转换超过 3 分钟，已终止；请另存为 .docx 后导入".into(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(e) => return Err(AppError::Other(format!("等待 LibreOffice 失败：{e}"))),
            }
        }

        let docx = fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| {
                p.extension()
                    .map(|e| e.eq_ignore_ascii_case("docx"))
                    .unwrap_or(false)
            })
            .ok_or_else(|| AppError::Other("LibreOffice 转换完成但没有生成 docx".into()))?;
        let mut out = parser::docx::parse(&docx)?;
        out.kind = DocKind::DocLegacy;
        out.warnings.push(
            "已通过 LibreOffice 转换为 docx 后解析；原文件排版可能与 Word 中略有差异。".into(),
        );
        Ok(out)
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

fn convert_with_textutil(source: &Path) -> Result<ParseOutput> {
    let output = Command::new("textutil")
        .arg("-convert")
        .arg("txt")
        .arg("-stdout")
        .arg(source)
        .output()
        .map_err(|e| AppError::Other(format!("调用 textutil 失败：{e}")))?;
    if !output.status.success() {
        return Err(AppError::Other(format!(
            "textutil 转换失败：{}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(300)
                .collect::<String>()
        )));
    }
    let text = crate::parser::txt::decode_bytes(&output.stdout);
    let text = crate::parser::txt::normalize_newlines(&text);
    let paragraphs: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    let doc = ParsedDoc::from_blocks(paragraphs, CHARS_PER_PAGE);
    Ok(ParseOutput {
        doc,
        kind: DocKind::DocLegacy,
        needs_ocr: false,
        ocr_used: false,
        warnings: vec!["已通过 macOS textutil 提取 .doc 文本；复杂表格/分栏版式可能丢失。".into()],
    })
}
