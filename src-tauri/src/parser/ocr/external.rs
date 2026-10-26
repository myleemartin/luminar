//! 外部本地 OCR 兜底引擎
//!
//! 适合本机已经装好 RapidOCR / PaddleOCR 命令行的情况：把页面渲染成 PNG，
//! 调用本地命令识别，读取标准输出。**不联网、不上传**，只是复用本机已有的离线模型。
//!
//! 命令模板示例（在「设置」中填写）：
//! `~/.local/bin/ocr_rapid.sh {image} -o -`
//!
//! 输出约定（二选一）：
//! 1. 每行一段文字（无坐标）——可用于 TXT / DOCX 导出；
//! 2. 每行 `文字<TAB>x<TAB>y<TAB>宽<TAB>高`——带坐标，可用于脱敏 PDF 导出。

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{AppError, Result};
use crate::parser::ocr::engine::{OcrEngine, OcrLine, OcrPageResult};

pub struct ExternalOcr {
    pub command: String,
}

impl ExternalOcr {
    pub fn new(command: &str) -> Self {
        ExternalOcr {
            command: command.trim().to_string(),
        }
    }

    /// 解析 stdout：支持「纯文本行」与「文本 + 4 个数字」两种格式
    fn parse_stdout(stdout: &str, width: u32, height: u32) -> Vec<OcrLine> {
        let mut lines = Vec::new();
        for raw in stdout.lines() {
            let line = raw.trim_end_matches('\r');
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let nums: Option<Vec<f32>> = parts[1..5]
                    .iter()
                    .map(|s| s.trim().parse::<f32>().ok())
                    .collect();
                if let Some(v) = nums {
                    lines.push(OcrLine {
                        text: parts[0].to_string(),
                        x: v[0],
                        y: v[1],
                        w: v[2],
                        h: v[3],
                    });
                    continue;
                }
            }
            // 无坐标：整行给一个占位框（后续 PDF 导出会退化为整页提示）
            lines.push(OcrLine {
                text: line.trim().to_string(),
                x: 0.0,
                y: 0.0,
                w: width as f32,
                h: height as f32,
            });
        }
        lines
    }
}

impl OcrEngine for ExternalOcr {
    fn id(&self) -> &'static str {
        "external"
    }

    fn label(&self) -> String {
        "外部本地 OCR 命令".into()
    }

    fn is_ready(&self) -> bool {
        !self.command.is_empty()
    }

    fn message(&self) -> String {
        if self.command.is_empty() {
            "尚未配置外部 OCR 命令，请在「设置」中填写。".into()
        } else {
            format!("将调用本机命令：{}", self.command)
        }
    }

    fn recognize(&self, image: &image::RgbImage) -> Result<OcrPageResult> {
        if !self.is_ready() {
            return Err(AppError::Ocr(self.message()));
        }
        let (width, height) = image.dimensions();

        // 临时文件放在系统临时目录，识别完立即删除
        let tmp = std::env::temp_dir().join(format!(
            "luminar_ocr_{}.png",
            uuid::Uuid::new_v4().simple()
        ));
        image
            .save(&tmp)
            .map_err(|e| AppError::Ocr(format!("写出临时图片失败：{e}")))?;

        let cmdline = self.command.replace("{image}", &tmp.to_string_lossy());
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", &cmdline]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", &cmdline]);
            c
        };
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Ocr(format!("调用外部 OCR 失败：{e}")))?;

        // 外部命令可能卡死（等待输入、GUI 弹窗等），超过 3 分钟强制终止
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if started.elapsed() > Duration::from_secs(180) {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = std::fs::remove_file(&tmp);
                        return Err(AppError::Ocr(
                            "外部 OCR 超过 3 分钟未返回，已终止。请检查命令模板或改用内置模型。"
                                .into(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = std::fs::remove_file(&tmp);
                    return Err(AppError::Ocr(format!("等待外部 OCR 失败：{e}")));
                }
            }
        };

        let mut stdout = String::new();
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stdout.take() {
            let _ = pipe.read_to_string(&mut stdout);
        }
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        let _ = std::fs::remove_file(&tmp);

        if !status.success() {
            return Err(AppError::Ocr(format!(
                "外部 OCR 返回失败状态：{status}\n{}",
                stderr.chars().take(500).collect::<String>()
            )));
        }
        let lines = Self::parse_stdout(stdout.as_str(), width, height);
        if lines.is_empty() {
            return Err(AppError::Ocr(
                "外部 OCR 没有输出文字，请检查命令模板是否正确".into(),
            ));
        }
        Ok(OcrPageResult {
            lines,
            width,
            height,
        })
    }
}
