//! 敏感识别引擎
//!
//! 结构：`规则（正则/关键词/内置 NER） → 候选 → 校验 → 阈值过滤 → 冲突消解 → 同名传播 → 结果`
//!
//! 性能：
//! - 正则一次性编译缓存，全文单遍扫描（regex crate 线性时间，无回溯风险）
//! - 字节坐标 → 字符坐标用二分查找，O(log n)，避免逐字符重扫
//! - NER 为纯文本扫描，复杂度 O(n × 线索数)，数万字文档毫秒级

pub mod builtin_rules;
pub mod model_ner;
pub mod ner;
pub mod validator;

use std::collections::HashSet;

use regex::{Regex, RegexBuilder};

use crate::models::{DocKind, Finding, FindingSource, Rule, RuleKind, StrategyConfig, Validator};

pub use model_ner::NerHit as ModelHit;

/// 识别结果
pub struct DetectionResult {
    pub findings: Vec<Finding>,
    pub warnings: Vec<String>,
}

/// 单个候选命中
#[derive(Debug, Clone)]
struct Candidate {
    start: usize,
    end: usize,
    text: String,
    category: String,
    category_label: String,
    confidence: f32,
    source: FindingSource,
    rule_id: Option<String>,
    rule_name: String,
    strategy: StrategyConfig,
    priority: i32,
}

impl Candidate {
    fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

struct CompiledRule {
    rule: Rule,
    regex: Option<Regex>,
}

pub struct DetectEngine {
    compiled: Vec<CompiledRule>,
    ner_rule: Option<Rule>,
    threshold: f32,
    propagate: bool,
    warnings: Vec<String>,
}

impl DetectEngine {
    /// 编译规则。非法正则不会导致整体失败，而是跳过并记录警告。
    pub fn build(rules: &[Rule], threshold: f32, propagate: bool) -> Self {
        let mut compiled = Vec::new();
        let mut ner_rule = None;
        let mut warnings = Vec::new();

        for rule in rules.iter().filter(|r| r.enabled) {
            match rule.kind {
                RuleKind::Ner => {
                    // 只接受内置人名识别规则，避免手写 JSON 覆盖整个 NER 引擎
                    if rule.builtin {
                        ner_rule = Some(rule.clone());
                    }
                }
                RuleKind::Regex => match RegexBuilder::new(&rule.pattern)
                    .case_insensitive(rule.case_insensitive)
                    .size_limit(10 * 1024 * 1024)
                    .build()
                {
                    Ok(re) => compiled.push(CompiledRule {
                        rule: rule.clone(),
                        regex: Some(re),
                    }),
                    Err(e) => warnings.push(format!("规则「{}」正则无效，已跳过：{e}", rule.name)),
                },
                RuleKind::Keyword => {
                    let mut words: Vec<&str> = rule
                        .keywords
                        .iter()
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    // 长词优先，避免「张三」抢先匹配「张三丰」
                    words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
                    words.dedup();
                    if words.is_empty() {
                        continue;
                    }
                    let pattern = words
                        .iter()
                        .map(|w| regex::escape(w))
                        .collect::<Vec<_>>()
                        .join("|");
                    match RegexBuilder::new(&pattern)
                        .case_insensitive(rule.case_insensitive)
                        .build()
                    {
                        Ok(re) => compiled.push(CompiledRule {
                            rule: rule.clone(),
                            regex: Some(re),
                        }),
                        Err(e) => {
                            warnings.push(format!("关键词规则「{}」编译失败：{e}", rule.name))
                        }
                    }
                }
            }
        }

        DetectEngine {
            compiled,
            ner_rule,
            threshold: threshold.clamp(0.0, 1.0),
            propagate,
            warnings,
        }
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// 对全文做自动识别
    pub fn detect(&self, text: &str) -> DetectionResult {
        self.detect_with(text, true, Vec::new())
    }

    /// 按文档类型识别：PDF 的换行多为版式折行，识别时去掉换行；
    /// Word/TXT 的换行是真实段落边界，保留换行避免跨段误匹配。
    pub fn detect_for_kind(&self, text: &str, kind: DocKind) -> DetectionResult {
        self.detect_with(text, kind.is_pdf(), Vec::new())
    }

    /// 规则识别 + 本地小模型增强识别
    pub fn detect_with_model(
        &self,
        text: &str,
        kind: DocKind,
        model_hits: Vec<ModelHit>,
    ) -> DetectionResult {
        self.detect_with(text, kind.is_pdf(), model_hits)
    }

    fn detect_with(
        &self,
        text: &str,
        join_lines: bool,
        model_hits: Vec<ModelHit>,
    ) -> DetectionResult {
        // 识别前去掉换行：PDF 常把手机号、住址、卡号折行，
        // 直接带 \n 匹配会漏检。这里建立「去换行文本 → 原文」的坐标映射，
        // 命中后仍回写到原文坐标，保证高亮与脱敏不偏移。
        let orig: Vec<char> = text.chars().collect();
        let mut norm = String::with_capacity(text.len());
        let mut map: Vec<usize> = Vec::with_capacity(orig.len() + 1);
        if join_lines {
            for (i, c) in orig.iter().enumerate() {
                if *c == '\n' || *c == '\r' {
                    continue;
                }
                norm.push(*c);
                map.push(i);
            }
            map.push(orig.len());
        } else {
            norm = text.to_string();
            map = (0..=orig.len()).collect();
        }
        let hay = norm.as_str();
        let index = CharIndex::new(hay);

        let to_orig = |ns: usize, ne: usize| -> (usize, usize) {
            let s = map.get(ns).copied().unwrap_or(orig.len());
            let e = if ne > ns {
                map.get(ne - 1).copied().unwrap_or(orig.len()) + 1
            } else {
                s
            };
            (s, e)
        };

        let mut candidates: Vec<Candidate> = Vec::new();
        let mut warnings = self.warnings.clone();

        // ---------- 1. 正则 / 关键词 ----------
        for cr in &self.compiled {
            let re = match &cr.regex {
                Some(re) => re,
                None => continue,
            };
            for caps in re.captures_iter(hay) {
                let m = match caps.get(cr.rule.capture_group).or_else(|| caps.get(0)) {
                    Some(m) => m,
                    None => continue,
                };
                if m.start() >= m.end() {
                    continue;
                }
                let raw = &hay[m.start()..m.end()];
                if raw.trim().is_empty() {
                    continue;
                }
                // 规则常把前置动词一起捕获（如「特向武汉市…人民法院」），这里按分类裁掉
                let snippet = trim_leading(&cr.rule.category, raw);
                let trim_bytes = raw.len() - snippet.len();
                let start_byte = m.start() + trim_bytes;
                let end_byte = m.end();
                if start_byte >= end_byte {
                    continue;
                }
                if cr.rule.require_boundary && !has_boundary(hay, start_byte, end_byte) {
                    continue;
                }
                let mut confidence = validate(&cr.rule, snippet, cr.rule.confidence);
                // 银行卡号旁出现「卡号/账号/账户」时提高置信度（很多脱敏示例号码过不了 Luhn 校验）
                if cr.rule.category == "bank_card"
                    && has_context(
                        hay,
                        start_byte,
                        &["卡号", "账号", "账户", "银行卡", "借记卡", "信用卡"],
                    )
                {
                    confidence = (confidence + 0.08).min(1.0);
                }
                if confidence < self.threshold {
                    continue;
                }
                let (o_start, o_end) =
                    to_orig(index.index_of(start_byte), index.index_of(end_byte));
                candidates.push(Candidate {
                    start: o_start,
                    end: o_end,
                    text: snippet.to_string(),
                    category: cr.rule.category.clone(),
                    category_label: cr.rule.category_label.clone(),
                    confidence,
                    source: if cr.rule.kind == RuleKind::Keyword {
                        FindingSource::Keyword
                    } else {
                        FindingSource::Regex
                    },
                    rule_id: Some(cr.rule.id.clone()),
                    rule_name: cr.rule.name.clone(),
                    strategy: cr.rule.strategy.clone(),
                    priority: cr.rule.priority,
                });
            }
        }

        // ---------- 2. 内置人名 NER ----------
        if let Some(rule) = &self.ner_rule {
            for hit in ner::detect(hay) {
                // NER 自带置信度与规则置信度取较小值，规则可整体调低灵敏度
                let confidence = hit.confidence.min(rule.confidence + 0.1);
                if confidence < self.threshold {
                    continue;
                }
                let (o_start, o_end) = to_orig(hit.start, hit.end);
                candidates.push(Candidate {
                    start: o_start,
                    end: o_end,
                    text: hit.name,
                    category: hit.category.to_string(),
                    category_label: if hit.category == "agent" {
                        "代理人".to_string()
                    } else {
                        "当事人姓名".to_string()
                    },
                    confidence,
                    source: FindingSource::Ner,
                    rule_id: Some(rule.id.clone()),
                    rule_name: rule.name.clone(),
                    strategy: rule.strategy.clone(),
                    priority: rule.priority,
                });
            }
        }

        // ---------- 2.5 本地小模型增强 ----------
        for hit in model_hits {
            if hit.confidence < self.threshold {
                continue;
            }
            candidates.push(Candidate {
                start: hit.start,
                end: hit.end,
                text: hit.text,
                category: hit.category.clone(),
                category_label: hit.category_label.clone(),
                confidence: hit.confidence,
                source: FindingSource::Model,
                rule_id: None,
                rule_name: "增强识别模型".into(),
                strategy: model_strategy(&hit.category),
                priority: 40,
            });
        }

        // ---------- 3. 冲突消解 ----------
        let mut accepted = resolve(candidates);

        // ---------- 4. 同名传播 ----------
        if self.propagate {
            propagate_names(hay, &index, &map, orig.len(), &mut accepted);
        }

        accepted.sort_by_key(|c| c.start);

        // ---------- 5. 转成对外模型 ----------
        let findings = accepted
            .into_iter()
            .map(|c| Finding {
                id: uuid::Uuid::new_v4().to_string(),
                start: c.start,
                end: c.end,
                text: c.text,
                category: c.category,
                category_label: c.category_label,
                confidence: (c.confidence * 1000.0).round() / 1000.0,
                source: c.source,
                rule_id: c.rule_id,
                rule_name: c.rule_name,
                enabled: true,
                strategy: c.strategy,
            })
            .collect();

        warnings.sort();
        warnings.dedup();
        DetectionResult { findings, warnings }
    }
}

/// 冲突消解：优先位置靠前、长度更长、置信度更高、规则优先级更高
fn resolve(mut cands: Vec<Candidate>) -> Vec<Candidate> {
    cands.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(b.len().cmp(&a.len()))
            .then(
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(b.priority.cmp(&a.priority))
    });
    let mut out: Vec<Candidate> = Vec::with_capacity(cands.len());
    for c in cands {
        if out.iter().any(|a| a.start < c.end && c.start < a.end) {
            continue;
        }
        out.push(c);
    }
    out
}

/// 同名传播：把已确认的姓名/律所在全文中的其他出现位置一并标记
fn propagate_names(
    hay: &str,
    index: &CharIndex,
    map: &[usize],
    orig_len: usize,
    accepted: &mut Vec<Candidate>,
) {
    let chars: Vec<char> = hay.chars().collect();
    let mut seen: HashSet<String> = HashSet::new();
    let seeds: Vec<Candidate> = accepted
        .iter()
        .filter(|c| {
            matches!(c.category.as_str(), "person_name" | "agent" | "law_firm")
                && c.text.chars().count() >= 2
                && !c.text.contains(char::is_whitespace)
        })
        .cloned()
        .collect();

    for seed in seeds {
        if !seen.insert(seed.text.clone()) {
            continue;
        }
        let seed_len = seed.text.chars().count();
        for (byte_pos, _) in hay.match_indices(&seed.text) {
            let ns = index.index_of(byte_pos);
            let ne = ns + seed_len;
            let os = map.get(ns).copied().unwrap_or(orig_len);
            let oe = map.get(ne - 1).copied().unwrap_or(orig_len) + 1;
            if accepted.iter().any(|a| a.start < oe && os < a.end) {
                continue;
            }
            if !has_char_boundary(&chars, ns, ne) {
                continue;
            }
            accepted.push(Candidate {
                start: os,
                end: oe,
                text: seed.text.clone(),
                category: seed.category.clone(),
                category_label: seed.category_label.clone(),
                confidence: (seed.confidence * 0.95).max(0.7),
                source: FindingSource::Propagated,
                rule_id: seed.rule_id.clone(),
                rule_name: format!("{}（同名传播）", seed.rule_name),
                strategy: seed.strategy.clone(),
                priority: seed.priority - 1,
            });
        }
    }
}

/// 前后不能紧贴汉字/字母数字（用于姓名传播，避免「张三」命中「张三丰」）
fn has_char_boundary(chars: &[char], start: usize, end: usize) -> bool {
    let is_word = |c: char| c.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&c);
    let before = if start == 0 {
        None
    } else {
        chars.get(start - 1).copied()
    };
    let after = chars.get(end).copied();
    !before.map(is_word).unwrap_or(false) && !after.map(is_word).unwrap_or(false)
}

/// 前后不能紧贴字母数字（避免从长串中截取身份证/手机号）
fn has_boundary(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    let bad = |c: char| c.is_ascii_alphanumeric() || c == '_';
    !before.map(bad).unwrap_or(false) && !after.map(bad).unwrap_or(false)
}

/// 按分类裁掉捕获片段前面混入的动词/介词（保持字符级偏移可回推）
fn trim_leading<'a>(category: &str, s: &'a str) -> &'a str {
    let prefixes: &[&str] = match category {
        "court" => &[
            "此致", "特向", "现向", "遂向", "依法", "请求", "判令", "不服", "委托", "向", "由",
            "在", "至", "经", "现", "遂",
        ],
        "law_firm" => &["委托", "聘请", "指派", "由", "在", "向", "与"],
        "address" => &[
            "户籍地址：",
            "户籍地址:",
            "住址：",
            "住址:",
            "地址：",
            "地址:",
            "现住",
            "位于",
            "住",
        ],
        _ => return s,
    };
    let mut out = s;
    for _ in 0..2 {
        let mut hit = false;
        for p in prefixes {
            if out.starts_with(p) {
                let rest = &out[p.len()..];
                let min_len = if category == "address" { 5 } else { 4 };
                if rest.chars().count() >= min_len {
                    out = rest;
                    hit = true;
                    break;
                }
            }
        }
        if !hit {
            break;
        }
    }
    out
}

/// 命中位置前方 36 个字节内是否出现给定关键词
fn has_context(text: &str, start: usize, words: &[&str]) -> bool {
    let mut from = start.saturating_sub(36);
    while from > 0 && !text.is_char_boundary(from) {
        from -= 1;
    }
    let window = &text[from..start];
    words.iter().any(|w| window.contains(w))
}

/// 模型命中使用的默认脱敏策略：姓名/单位/地址用占位标签
fn model_strategy(category: &str) -> StrategyConfig {
    match category {
        "person_name" | "agent" | "law_firm" | "court" | "org_name" | "address" => StrategyConfig {
            kind: crate::models::StrategyKind::Placeholder,
            mask_char: "*".into(),
            keep_prefix: 0,
            keep_suffix: 0,
            placeholder: "【{type}】".into(),
        },
        _ => StrategyConfig::default(),
    }
}

fn validate(rule: &Rule, snippet: &str, base: f32) -> f32 {
    let ok = match rule.validator {
        Validator::None => return base,
        Validator::IdCard => validator::id_card_valid(snippet),
        Validator::Luhn => validator::luhn_valid(snippet),
        Validator::Uscc => validator::uscc_valid(snippet),
    };
    if ok {
        (base + 0.02).min(1.0)
    } else {
        // 校验失败只降权，不直接丢弃：文书里出现真实但校验不规范的号码仍需要脱敏
        (base * 0.85).max(0.3)
    }
}

/// 字节下标 → 字符下标（二分查找）
pub struct CharIndex {
    starts: Vec<usize>,
}

impl CharIndex {
    pub fn new(s: &str) -> Self {
        let mut starts: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
        starts.push(s.len());
        CharIndex { starts }
    }

    pub fn index_of(&self, byte: usize) -> usize {
        match self.starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::builtin_rules::builtin_rules;

    fn engine() -> DetectEngine {
        DetectEngine::build(&builtin_rules(), 0.75, true)
    }

    #[test]
    fn detect_common_fields() {
        let text = "原告张三，男，1988年1月1日出生，身份证号420102198801011234，\
                    住湖北省武汉市江岸区建设大道123号新世界大厦A座1801室，\
                    电话13812345678，邮箱zhangsan@example.com。";
        let result = engine().detect(text);
        let cats: Vec<&str> = result
            .findings
            .iter()
            .map(|f| f.category.as_str())
            .collect();
        assert!(cats.contains(&"id_card"), "{cats:?}");
        assert!(cats.contains(&"phone"), "{cats:?}");
        assert!(cats.contains(&"email"), "{cats:?}");
        assert!(cats.contains(&"address"), "{cats:?}");
        assert!(cats.contains(&"person_name"), "{cats:?}");
    }

    #[test]
    fn detect_case_number_and_court() {
        let text = "（2024）鄂0102民初1234号，由武汉市江岸区人民法院审理。";
        let result = engine().detect(text);
        let cats: Vec<&str> = result
            .findings
            .iter()
            .map(|f| f.category.as_str())
            .collect();
        assert!(cats.contains(&"case_number"), "{cats:?}");
        assert!(cats.contains(&"court"), "{cats:?}");
    }

    #[test]
    fn model_hits_are_merged_as_findings() {
        let rules = crate::detect::builtin_rules::builtin_rules();
        let engine = DetectEngine::build(&rules, 0.75, true);
        let text = "某甲与某乙签订合同。";
        let hits = vec![ModelHit {
            start: 0,
            end: 2,
            category: "person_name".into(),
            category_label: "当事人姓名".into(),
            confidence: 0.99,
            text: "某甲".into(),
        }];
        let result = engine.detect_with_model(text, DocKind::Txt, hits);
        let f = result
            .findings
            .iter()
            .find(|f| f.text == "某甲")
            .expect("模型命中应进入结果");
        assert_eq!(f.source, FindingSource::Model);
        assert_eq!(f.category_label, "当事人姓名");
    }

    #[test]
    fn no_overlap() {
        let text = "身份证号420102198801011234，银行卡6222021234567890123。";
        let result = engine().detect(text);
        let mut ranges: Vec<(usize, usize)> =
            result.findings.iter().map(|f| (f.start, f.end)).collect();
        ranges.sort();
        for w in ranges.windows(2) {
            assert!(w[0].1 <= w[1].0, "命中区间重叠：{:?}", w);
        }
    }
}
