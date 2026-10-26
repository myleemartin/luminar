//! 本地配置持久化
//!
//! - `settings.json`：置信度阈值、OCR 参数、导出参数等
//! - `rules.json`：全部识别规则（内置 + 自定义）
//!
//! 文件全部位于系统应用配置目录（macOS: ~/Library/Application Support/cn.luminar.app），
//! 也可在界面「设置」中一键打开。写入采用「临时文件 + 重命名」，避免断电/崩溃损坏配置。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::detect::builtin_rules::{builtin_categories, builtin_rules};
use crate::error::{AppError, Result};
use crate::models::{AppPaths, CategoryInfo, Rule, Settings};

/// rules.json 的外层结构，带版本号便于以后升级迁移
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesFile {
    #[serde(default = "default_version")]
    pub version: u32,
    pub rules: Vec<Rule>,
}

fn default_version() -> u32 {
    1
}

pub struct ConfigStore {
    pub paths: AppPaths,
    pub settings: Settings,
    pub rules: Vec<Rule>,
}

impl ConfigStore {
    /// 初始化：确保目录存在，读取配置；首次运行写入内置规则
    pub fn init(config_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&config_dir)?;
        let models_dir = config_dir.join("models");
        fs::create_dir_all(&models_dir)?;
        let paths = AppPaths {
            config_dir: config_dir.to_string_lossy().to_string(),
            rules_file: config_dir.join("rules.json").to_string_lossy().to_string(),
            settings_file: config_dir
                .join("settings.json")
                .to_string_lossy()
                .to_string(),
            models_dir: models_dir.to_string_lossy().to_string(),
        };

        let settings = load_json::<Settings>(Path::new(&paths.settings_file))
            .unwrap_or_default()
            .normalized();

        // 空规则列表是合法状态（用户可能主动删光），不要强行恢复内置规则
        let rules = match load_json::<RulesFile>(Path::new(&paths.rules_file)) {
            Some(file) => file.rules,
            None => {
                let rules = builtin_rules();
                // 首次运行落盘，方便用户直接编辑 JSON
                let _ = save_json(
                    Path::new(&paths.rules_file),
                    &RulesFile {
                        version: 1,
                        rules: rules.clone(),
                    },
                );
                rules
            }
        };

        let store = ConfigStore {
            paths,
            settings,
            rules,
        };
        store.save_settings()?;
        Ok(store)
    }

    pub fn save_settings(&self) -> Result<()> {
        save_json(Path::new(&self.paths.settings_file), &self.settings)
    }

    pub fn save_rules(&self) -> Result<()> {
        save_json(
            Path::new(&self.paths.rules_file),
            &RulesFile {
                version: 1,
                rules: self.rules.clone(),
            },
        )
    }

    /// 新增或更新一条规则
    pub fn upsert_rule(&mut self, rule: Rule) -> Result<()> {
        rule.validate()?;
        match self.rules.iter_mut().find(|r| r.id == rule.id) {
            Some(existing) => *existing = rule,
            None => self.rules.push(rule),
        }
        self.save_rules()
    }

    pub fn delete_rule(&mut self, id: &str) -> Result<()> {
        self.rules.retain(|r| r.id != id);
        self.save_rules()
    }

    pub fn reset_rules(&mut self) -> Result<()> {
        self.rules = builtin_rules();
        self.save_rules()
    }

    /// 分类清单：内置分类 + 规则里出现的自定义分类
    pub fn categories(&self) -> Vec<CategoryInfo> {
        let mut out: Vec<CategoryInfo> = builtin_categories()
            .into_iter()
            .map(|(id, label)| CategoryInfo {
                id: id.into(),
                label: label.into(),
            })
            .collect();
        for r in &self.rules {
            if !out.iter().any(|c| c.id == r.category) {
                out.push(CategoryInfo {
                    id: r.category.clone(),
                    label: r.category_label.clone(),
                });
            }
        }
        out
    }
}

impl Settings {
    /// 修正越界值，避免手改 JSON 后程序行为异常
    pub fn normalized(mut self) -> Self {
        self.threshold = self.threshold.clamp(0.0, 1.0);
        self.ocr_dpi = self.ocr_dpi.clamp(100, 400);
        self.pdf_export_dpi = self.pdf_export_dpi.clamp(96, 400);
        if self.default_mask_char.is_empty() {
            self.default_mask_char = "*".into();
        }
        if self.placeholder_template.is_empty() {
            self.placeholder_template = "{{type}}".into();
        }
        if self.ocr_engine.is_empty() {
            self.ocr_engine = "paddle_onnx".into();
        }
        self
    }
}

impl Rule {
    /// 保存前的合法性检查，错误信息直接给前端展示
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Rule("规则名称不能为空".into()));
        }
        if self.category.trim().is_empty() {
            return Err(AppError::Rule("请选择识别分类".into()));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(AppError::Rule("置信度必须在 0~1 之间".into()));
        }
        match self.kind {
            crate::models::RuleKind::Regex => {
                if self.pattern.trim().is_empty() {
                    return Err(AppError::Rule("正则表达式不能为空".into()));
                }
                regex::RegexBuilder::new(&self.pattern)
                    .case_insensitive(self.case_insensitive)
                    .build()
                    .map_err(|e| AppError::Rule(format!("正则表达式不合法：{e}")))?;
            }
            crate::models::RuleKind::Keyword => {
                if self.keywords.iter().all(|k| k.trim().is_empty()) {
                    return Err(AppError::Rule("关键词列表不能为空".into()));
                }
            }
            crate::models::RuleKind::Ner => {}
        }
        Ok(())
    }
}

fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str::<T>(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            // 配置文件损坏时先备份再回退，避免用户手改出错后规则被静默覆盖
            let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let backup = path.with_file_name(format!(
                "{}.broken-{stamp}",
                path.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "config.json".into())
            ));
            let _ = fs::rename(path, &backup);
            eprintln!(
                "[配置] 解析 {} 失败：{e}；原文件已备份为 {}，本次使用默认值",
                path.display(),
                backup.display()
            );
            None
        }
    }
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| AppError::Config(format!("序列化配置失败：{e}")))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &text)?;
    // Windows 的 fs::rename 不允许覆盖已存在文件，先移除旧文件再重命名
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    if fs::rename(&tmp, path).is_err() {
        // 极端情况下（杀毒软件占用等）退回直接写入，保证配置不丢
        fs::write(path, &text)?;
        let _ = fs::remove_file(&tmp);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Rule;

    #[test]
    fn parse_hand_written_rule() {
        // 教程里给律师抄的极简规则：只有必填项，其余走默认值
        let json = r#"{
            "name": "项目代号",
            "kind": "keyword",
            "keywords": ["星河计划"],
            "strategy": { "kind": "delete" }
        }"#;
        let rule: Rule = serde_json::from_str(json).expect("手写规则应能解析");
        assert_eq!(rule.category, "custom");
        assert_eq!(rule.category_label, "自定义");
        assert!(!rule.id.is_empty());
        assert!((rule.confidence - 0.9).abs() < f32::EPSILON);
        assert_eq!(rule.strategy.kind, crate::models::StrategyKind::Delete);
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "luminar_cfg_{tag}_{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn corrupt_rules_are_backed_up() {
        let dir = tmp_dir("corrupt");
        fs::write(dir.join("rules.json"), "{ not json").unwrap();
        let store = ConfigStore::init(dir.clone()).expect("初始化应回退到内置规则");
        assert!(!store.rules.is_empty(), "应回退到内置规则");
        let backup = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".broken-"));
        assert!(backup, "损坏的 rules.json 应被备份而不是静默覆盖");
    }

    #[test]
    fn explicitly_empty_rules_are_kept() {
        let dir = tmp_dir("empty");
        fs::write(dir.join("rules.json"), r#"{"version":1,"rules":[]}"#).unwrap();
        let store = ConfigStore::init(dir).expect("空规则文件应能加载");
        assert!(store.rules.is_empty(), "用户主动清空的规则不应被重置");
    }
}
