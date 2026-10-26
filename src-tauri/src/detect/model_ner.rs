//! 本地轻量 NER 模型（可选「增强识别」）
//!
//! 模型：`hfl/minirbt-h288` 微调的中文 NER（PER / ORG / LOC），
//! 由 ONNX Runtime 做 INT8 动态量化后约 12 MB，使用 `tract` 在本地 CPU 推理。
//!
//! 设计取舍：
//! - 只做「补充」：正则规则仍然优先，模型用于捞回规则漏掉的人名、单位、住址；
//! - 全离线、无 GPU 依赖，x86/ARM 通用；模型加载一次后缓存；
//! - 文本按 510 token 分块（重叠 48），避免长文档截断；
//! - 只接受 PER / ORG / LOC，MISC 丢弃；地址要求包含路/街/号等特征，减少误报。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tract_onnx::prelude::*;
use tract_onnx::tract_core::model::typed::TypedRunnableModel;

use crate::error::{AppError, Result};

const MAX_SEQ: usize = 512;
const MAX_TOKENS: usize = MAX_SEQ - 2;
const CHUNK_OVERLAP: usize = 48;
const LABELS: [&str; 9] = [
    "O", "B-PER", "I-PER", "B-ORG", "I-ORG", "B-LOC", "I-LOC", "B-MISC", "I-MISC",
];

/// 模型识别出的一个实体
#[derive(Debug, Clone)]
pub struct NerHit {
    pub start: usize,
    pub end: usize,
    pub category: String,
    pub category_label: String,
    pub confidence: f32,
    pub text: String,
}

#[derive(Debug, Clone)]
struct Token {
    id: i64,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct RawEntity {
    typ: &'static str,
    first: usize,
    last: usize,
    conf_sum: f32,
    conf_n: usize,
}

pub struct ModelNer {
    model: Arc<TypedRunnableModel>,
    vocab: HashMap<String, i64>,
    unk: i64,
    cls: i64,
    sep: i64,
    pad: i64,
}

impl ModelNer {
    pub fn is_available(dir: &Path) -> bool {
        dir.join("ner.onnx").is_file() && dir.join("vocab.txt").is_file()
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let model_path = dir.join("ner.onnx");
        let vocab_path = dir.join("vocab.txt");
        if !model_path.is_file() {
            return Err(AppError::Other(format!(
                "未找到增强识别模型：{}",
                model_path.display()
            )));
        }
        if !vocab_path.is_file() {
            return Err(AppError::Other(format!(
                "未找到增强识别词表：{}",
                vocab_path.display()
            )));
        }

        let mut model = tract_onnx::onnx()
            .model_for_path(&model_path)
            .map_err(|e| AppError::Other(format!("加载增强识别模型失败：{e:?}")))?;
        if model.inputs.len() != 3 {
            return Err(AppError::Other(format!(
                "增强识别模型输入数量异常（{}，期望 3）",
                model.inputs.len()
            )));
        }
        for i in 0..3 {
            model
                .set_input_fact(i, i64::fact([1, MAX_SEQ]).into())
                .map_err(|e| AppError::Other(format!("设置增强识别输入维度失败：{e:?}")))?;
        }
        let model = model
            .into_optimized()
            .map_err(|e| AppError::Other(format!("优化增强识别模型失败：{e:?}")))?
            .into_runnable()
            .map_err(|e| AppError::Other(format!("编译增强识别模型失败：{e:?}")))?;

        let text = std::fs::read_to_string(&vocab_path)
            .map_err(|e| AppError::Other(format!("读取增强识别词表失败：{e}")))?;
        let text = text.trim_start_matches('\u{feff}');
        let mut vocab = HashMap::with_capacity(22000);
        for (i, line) in text.lines().enumerate() {
            let token = line.trim_end_matches(['\r', '\n']);
            vocab.insert(token.to_string(), i as i64);
        }
        let unk = *vocab.get("[UNK]").unwrap_or(&100);
        let cls = *vocab.get("[CLS]").unwrap_or(&101);
        let sep = *vocab.get("[SEP]").unwrap_or(&102);
        let pad = *vocab.get("[PAD]").unwrap_or(&0);
        if vocab.len() < 1000 {
            return Err(AppError::Other(format!(
                "增强识别词表内容异常（{} 行）",
                vocab.len()
            )));
        }

        Ok(ModelNer {
            model,
            vocab,
            unk,
            cls,
            sep,
            pad,
        })
    }

    /// 对全文做实体识别
    pub fn detect(&self, text: &str) -> Result<Vec<NerHit>> {
        let chars: Vec<char> = text.chars().collect();
        let tokens = self.tokenize(text);
        if tokens.is_empty() {
            return Ok(vec![]);
        }

        let mut raw: Vec<(RawEntity, usize)> = Vec::new(); // (实体，全局 token 起点)
        let mut start = 0usize;
        loop {
            let end = (start + MAX_TOKENS).min(tokens.len());
            let window = &tokens[start..end];
            let entities = self.run_window(window)?;
            for e in entities {
                raw.push((e, start));
            }
            if end == tokens.len() {
                break;
            }
            start = end.saturating_sub(CHUNK_OVERLAP).max(start + 1);
        }

        let mut hits: Vec<NerHit> = Vec::new();
        for (e, offset) in raw {
            let first = offset + e.first;
            let last = offset + e.last;
            if first >= tokens.len() || last >= tokens.len() || first > last {
                continue;
            }
            let start_char = tokens[first].start;
            let end_char = tokens[last].end;
            if start_char >= end_char || end_char > chars.len() {
                continue;
            }
            let span: String = chars[start_char..end_char].iter().collect();
            // 跨段落/跨行的实体在 Word 定点改写时容易重复替换，交回规则处理
            if span.contains('\n') || span.contains('\r') {
                continue;
            }
            let Some((category, label)) = map_category(e.typ, &span) else {
                continue;
            };
            let conf = if e.conf_n > 0 {
                e.conf_sum / e.conf_n as f32
            } else {
                0.0
            };
            hits.push(NerHit {
                start: start_char,
                end: end_char,
                category: category.to_string(),
                category_label: label.to_string(),
                confidence: (conf.clamp(0.0, 1.0) * 1000.0).round() / 1000.0,
                text: span,
            });
        }

        // 分块重叠会产生重复实体：按区间去重，同区间保留置信度更高的
        hits.sort_by(|a, b| {
            a.start.cmp(&b.start).then(b.end.cmp(&a.end)).then(
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        let mut merged: Vec<NerHit> = Vec::new();
        for h in hits {
            if let Some(last) = merged.last_mut() {
                if last.category == h.category && h.start <= last.end {
                    if h.end > last.end {
                        last.end = h.end;
                        last.text = chars[last.start..last.end].iter().collect();
                        last.confidence = last.confidence.max(h.confidence);
                    }
                    continue;
                }
                if last.start == h.start && last.end == h.end {
                    last.confidence = last.confidence.max(h.confidence);
                    continue;
                }
            }
            merged.push(h);
        }
        Ok(merged)
    }

    fn run_window(&self, tokens: &[Token]) -> Result<Vec<RawEntity>> {
        let mut ids = Vec::with_capacity(MAX_SEQ);
        ids.push(self.cls);
        ids.extend(tokens.iter().map(|t| t.id));
        ids.push(self.sep);
        let real_len = ids.len();
        ids.resize(MAX_SEQ, self.pad);

        let mut mask = vec![0i64; MAX_SEQ];
        for v in mask.iter_mut().take(real_len) {
            *v = 1;
        }
        let tt = vec![0i64; MAX_SEQ];

        let input_ids = Tensor::from_shape(&[1, MAX_SEQ], &ids)
            .map_err(|e| AppError::Other(format!("构造增强识别输入失败：{e}")))?;
        let attention = Tensor::from_shape(&[1, MAX_SEQ], &mask)
            .map_err(|e| AppError::Other(format!("构造增强识别掩码失败：{e}")))?;
        let token_type = Tensor::from_shape(&[1, MAX_SEQ], &tt)
            .map_err(|e| AppError::Other(format!("构造增强识别类型失败：{e}")))?;

        let outputs = self
            .model
            .run(tvec!(input_ids.into(), attention.into(), token_type.into()))
            .map_err(|e| AppError::Other(format!("增强识别推理失败：{e}")))?;
        let arr = outputs[0]
            .to_plain_array_view::<f32>()
            .map_err(|e| AppError::Other(format!("读取增强识别输出失败：{e}")))?;
        let shape = arr.shape().to_vec();
        if shape.len() != 3 || shape[1] != MAX_SEQ || shape[2] != LABELS.len() {
            return Err(AppError::Other(format!("增强识别输出维度异常：{shape:?}")));
        }
        let logits: Vec<f32> = arr.iter().copied().collect();

        let mut entities: Vec<RawEntity> = Vec::new();
        let mut cur: Option<RawEntity> = None;
        for pos in 0..tokens.len() {
            let row = &logits[(pos + 1) * LABELS.len()..(pos + 2) * LABELS.len()];
            let mut best = 0usize;
            let mut best_v = f32::NEG_INFINITY;
            for (i, v) in row.iter().enumerate() {
                if *v > best_v {
                    best_v = *v;
                    best = i;
                }
            }
            let mut sum = 0f32;
            for v in row {
                sum += (v - best_v).exp();
            }
            let prob = if sum > 0.0 { 1.0 / sum } else { 0.0 };
            let label = LABELS[best];
            let (prefix, typ) = match label.split_once('-') {
                Some((p, t)) => (p, t),
                None => ("O", ""),
            };

            match prefix {
                "B" | "S" => {
                    if let Some(e) = cur.take() {
                        if e.conf_n > 0 {
                            entities.push(e);
                        }
                    }
                    if prefix == "S" {
                        entities.push(RawEntity {
                            typ: leak_type(typ),
                            first: pos,
                            last: pos,
                            conf_sum: prob,
                            conf_n: 1,
                        });
                    } else {
                        cur = Some(RawEntity {
                            typ: leak_type(typ),
                            first: pos,
                            last: pos,
                            conf_sum: prob,
                            conf_n: 1,
                        });
                    }
                }
                "I" => {
                    if let Some(mut e) = cur.take() {
                        if e.typ == typ {
                            e.last = pos;
                            e.conf_sum += prob;
                            e.conf_n += 1;
                            cur = Some(e);
                        } else {
                            entities.push(e.clone());
                            cur = Some(RawEntity {
                                typ: leak_type(typ),
                                first: pos,
                                last: pos,
                                conf_sum: prob,
                                conf_n: 1,
                            });
                        }
                    } else {
                        cur = Some(RawEntity {
                            typ: leak_type(typ),
                            first: pos,
                            last: pos,
                            conf_sum: prob,
                            conf_n: 1,
                        });
                    }
                }
                _ => {
                    if let Some(e) = cur.take() {
                        entities.push(e);
                    }
                }
            }
        }
        if let Some(e) = cur {
            entities.push(e);
        }
        Ok(entities)
    }

    fn tokenize(&self, text: &str) -> Vec<Token> {
        let chars: Vec<char> = text.chars().collect();
        let mut out: Vec<Token> = Vec::new();
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() || c.is_control() {
                i += 1;
                continue;
            }
            if is_cjk(c) || !c.is_alphanumeric() {
                let id = *self.vocab.get(&c.to_string()).unwrap_or(&self.unk);
                out.push(Token {
                    id,
                    start: i,
                    end: i + 1,
                });
                i += 1;
                continue;
            }
            // 连续字母/数字：整体小写后做 WordPiece
            let start = i;
            while i < chars.len()
                && !chars[i].is_whitespace()
                && !chars[i].is_control()
                && chars[i].is_alphanumeric()
                && !is_cjk(chars[i])
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
            for id in self.wordpiece(&word) {
                out.push(Token { id, start, end: i });
            }
        }
        out
    }

    fn wordpiece(&self, word: &str) -> Vec<i64> {
        let chars: Vec<char> = word.chars().collect();
        if chars.is_empty() {
            return vec![];
        }
        let mut ids = Vec::new();
        let mut start = 0usize;
        while start < chars.len() {
            let mut end = chars.len();
            let mut found: Option<(i64, usize)> = None;
            while start < end {
                let sub: String = chars[start..end].iter().collect();
                let key = if start > 0 { format!("##{sub}") } else { sub };
                if let Some(id) = self.vocab.get(&key) {
                    found = Some((*id, end));
                    break;
                }
                end -= 1;
            }
            match found {
                Some((id, next)) => {
                    ids.push(id);
                    start = next;
                }
                None => {
                    ids.push(self.unk);
                    break;
                }
            }
        }
        if ids.is_empty() {
            vec![self.unk]
        } else {
            ids
        }
    }
}

/// 标签类型需要 `'static` 生命周期，模型只有 4 类，直接映射
fn leak_type(typ: &str) -> &'static str {
    match typ {
        "PER" => "PER",
        "ORG" => "ORG",
        "LOC" => "LOC",
        _ => "MISC",
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

fn map_category(typ: &str, span: &str) -> Option<(&'static str, &'static str)> {
    match typ {
        "PER" => {
            if span.chars().count() < 2 || is_stop_name(span) {
                None
            } else {
                Some(("person_name", "当事人姓名"))
            }
        }
        "ORG" => {
            if span.contains("律师事务所") || span.contains("法律服务所") {
                Some(("law_firm", "律所名称"))
            } else if span.contains("法院") {
                Some(("court", "法院名称"))
            } else if span.chars().count() >= 4 {
                Some(("org_name", "单位名称"))
            } else {
                None
            }
        }
        "LOC" => {
            if span.chars().count() >= 6 && is_address_like(span) {
                Some(("address", "住址"))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_stop_name(s: &str) -> bool {
    matches!(
        s,
        "本院"
            | "原告"
            | "被告"
            | "第三人"
            | "上诉人"
            | "被上诉人"
            | "申请人"
            | "被申请人"
            | "代理人"
            | "当事人"
            | "委托人"
            | "法定代表人"
            | "负责人"
            | "公民"
            | "汉族"
            | "律师"
            | "法院"
            | "公司"
            | "银行"
    )
}

fn is_address_like(s: &str) -> bool {
    const MARKERS: &[&str] = &[
        "省",
        "市",
        "区",
        "县",
        "镇",
        "乡",
        "村",
        "路",
        "街",
        "道",
        "巷",
        "弄",
        "号",
        "室",
        "楼",
        "栋",
        "单元",
        "大厦",
        "小区",
        "广场",
        "苑",
        "园",
        "里",
        "自治区",
    ];
    if s.contains("中华人民共和国") {
        return false;
    }
    MARKERS.iter().any(|m| s.contains(m))
}

/// 模型目录（`models/ner`）路径
pub fn model_dir(models_root: &Path) -> PathBuf {
    models_root.join("ner")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ModelNer {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/models/ner");
        ModelNer::load(&dir).expect("加载增强识别模型失败")
    }

    #[test]
    fn recognize_person_org_address() {
        let text = "原告：张三，住湖北省武汉市江岸区建设大道123号新世界大厦A座1801室。被告：李四，系湖北观筑律师事务所律师。";
        let hits = model().detect(text).expect("推理失败");
        let names: Vec<&str> = hits.iter().map(|h| h.text.as_str()).collect();
        assert!(names.contains(&"张三"), "命中：{names:?}");
        assert!(names.contains(&"李四"), "命中：{names:?}");
        assert!(
            hits.iter().any(|h| h.category == "law_firm"),
            "应识别律所：{names:?}"
        );
        assert!(
            hits.iter()
                .any(|h| h.category == "address" && h.text.contains("建设大道")),
            "应识别完整住址：{names:?}"
        );
    }

    #[test]
    fn recognize_court_and_company() {
        let m = model();
        let hits = m
            .detect("原告王小明诉被告李娜民间借贷纠纷一案，由武汉市江岸区人民法院审理。")
            .expect("推理失败");
        assert!(hits
            .iter()
            .any(|h| h.text == "王小明" && h.category == "person_name"));
        assert!(hits.iter().any(|h| h.category == "court"));
        let orgs = m
            .detect("中国建设银行股份有限公司武汉江岸支行与华为技术有限公司签订合同。")
            .expect("推理失败");
        assert!(
            orgs.iter()
                .any(|h| h.category == "org_name" && h.text.contains("建设银行")),
            "应识别单位：{:?}",
            orgs.iter().map(|h| &h.text).collect::<Vec<_>>()
        );
        // 法律条文中的“中华人民共和国”不应被当成住址
        let law = m
            .detect("根据《中华人民共和国民法典》第五百七十七条，被告应承担违约责任。")
            .expect("推理失败");
        assert!(
            !law.iter().any(|h| h.text.contains("中华人民共和国")),
            "不应把国名当住址：{:?}",
            law.iter().map(|h| &h.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn long_text_chunks_do_not_duplicate() {
        let m = model();
        let text = "张三与李四签订合同。".repeat(80);
        let hits = m.detect(&text).expect("推理失败");
        let mut ranges: Vec<(usize, usize)> = hits.iter().map(|h| (h.start, h.end)).collect();
        ranges.sort();
        ranges.dedup();
        assert_eq!(ranges.len(), hits.len(), "分块结果不应有重复区间");
        assert!(!hits.is_empty());
    }

    #[test]
    fn tokenizer_offsets_are_consistent() {
        let m = model();
        let text = "原告张三，电话13812345678。";
        let tokens = m.tokenize(text);
        assert!(!tokens.is_empty());
        let chars: Vec<char> = text.chars().collect();
        for t in &tokens {
            assert!(t.start < t.end && t.end <= chars.len());
        }
        // token 覆盖的字符应能拼回原文中的片段
        let reconstructed: String = tokens
            .iter()
            .map(|t| chars[t.start..t.end].iter().collect::<String>())
            .collect();
        assert!(reconstructed.contains("张三"));
    }
}
