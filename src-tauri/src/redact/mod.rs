//! 脱敏处理层
//!
//! 纯函数式：输入全文 + 命中清单，输出脱敏文本与变更片段（用于前端对照预览、导出日志）。
//! 支持三种策略：掩码遮蔽 / 占位标签 / 直接删除，逐条可覆盖。

use std::collections::HashMap;

use crate::models::{Finding, RedactionSegment, StrategyConfig, StrategyKind};

/// 脱敏输出
#[derive(Debug, Clone, Default)]
pub struct RedactionOutput {
    pub text: String,
    pub segments: Vec<RedactionSegment>,
    /// (分类 id, 分类中文名, 次数)
    pub by_category: Vec<(String, String, usize)>,
}

impl RedactionOutput {
    /// 变更总数
    pub fn total(&self) -> usize {
        self.segments.len()
    }
}

/// 对全文脱敏
pub fn redact_text(text: &str, findings: &[Finding]) -> RedactionOutput {
    let mut numbering = HashMap::new();
    redact_range(text, findings, 0, usize::MAX, &mut numbering)
}

/// 对 `[range_start, range_end)` 范围脱敏（用于 docx 逐段改写）
///
/// - `numbering` 跨段共享，保证 `{{index}}` 编号连续
pub fn redact_range(
    text: &str,
    findings: &[Finding],
    range_start: usize,
    range_end: usize,
    numbering: &mut HashMap<String, usize>,
) -> RedactionOutput {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let range_end = range_end.min(len);

    // 只保留落在本次范围内、且启用的命中
    let mut items: Vec<(usize, usize, &Finding)> = findings
        .iter()
        .filter(|f| f.enabled && f.end > f.start && f.end > range_start && f.start < range_end)
        .map(|f| (f.start.max(range_start), f.end.min(range_end), f))
        .filter(|(s, e, _)| e > s)
        .collect();
    items.sort_by_key(|(s, _, _)| *s);

    let mut out = String::with_capacity(text.len());
    let mut segments = Vec::new();
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    let mut cursor = range_start;

    for (start, end, finding) in items {
        if start < cursor {
            continue; // 与已处理区间重叠，跳过（人工标记冲突时的兜底）
        }
        out.extend(chars[cursor..start].iter());
        let original: String = chars[start..end].iter().collect();
        let idx = numbering.entry(finding.category.clone()).or_insert(0);
        *idx += 1;
        let replacement =
            apply_strategy(&original, &finding.strategy, &finding.category_label, *idx);
        out.push_str(&replacement);
        segments.push(RedactionSegment {
            start,
            end,
            replacement,
            category: finding.category.clone(),
            category_label: finding.category_label.clone(),
            mode: finding.strategy.kind.label().to_string(),
        });
        *counts
            .entry((finding.category.clone(), finding.category_label.clone()))
            .or_insert(0) += 1;
        cursor = end;
    }
    out.extend(chars[cursor.min(range_end)..range_end].iter());

    let mut by_category: Vec<(String, String, usize)> = counts
        .into_iter()
        .map(|((id, label), n)| (id, label, n))
        .collect();
    by_category.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));

    RedactionOutput {
        text: out,
        segments,
        by_category,
    }
}

/// 单条脱敏：按策略生成替换文本
pub fn apply_strategy(
    original: &str,
    strategy: &StrategyConfig,
    label: &str,
    index: usize,
) -> String {
    // 命中区间可能跨越 PDF/Word 的换行符（识别时已去除换行），
    // 生成替换文本时也忽略换行，避免掩码后残留半截行。
    let cleaned: String = original
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect();
    let original = cleaned.as_str();
    match strategy.kind {
        StrategyKind::Delete => String::new(),
        StrategyKind::Placeholder => {
            let mut s = strategy.placeholder.clone();
            if s.is_empty() {
                s = "【{type}】".into();
            }
            s = s.replace("{type}", label);
            s = s.replace("{index}", &index.to_string());
            s
        }
        StrategyKind::Mask => {
            let chars: Vec<char> = original.chars().collect();
            let n = chars.len();
            let prefix = strategy.keep_prefix.min(n);
            let suffix = strategy.keep_suffix.min(n.saturating_sub(prefix));
            let mask_count = n.saturating_sub(prefix + suffix);
            let mask_char = strategy.mask_char.chars().next().unwrap_or('*');
            let mut s = String::with_capacity(n);
            s.extend(chars[..prefix].iter());
            for _ in 0..mask_count {
                s.push(mask_char);
            }
            s.extend(chars[n - suffix..].iter());
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::FindingSource;

    fn finding(start: usize, end: usize, text: &str, kind: StrategyKind) -> Finding {
        Finding {
            id: "t".into(),
            start,
            end,
            text: text.into(),
            category: "id_card".into(),
            category_label: "身份证号".into(),
            confidence: 0.9,
            source: FindingSource::Regex,
            rule_id: None,
            rule_name: "t".into(),
            enabled: true,
            strategy: StrategyConfig {
                kind,
                mask_char: "*".into(),
                keep_prefix: 3,
                keep_suffix: 4,
                placeholder: "【{type}】".into(),
            },
        }
    }

    #[test]
    fn test_mask() {
        let text = "电话13812345678。";
        let f = finding(2, 13, "13812345678", StrategyKind::Mask);
        let out = redact_text(text, &[f]);
        assert_eq!(out.text, "电话138****5678。");
        assert_eq!(out.segments.len(), 1);
    }

    #[test]
    fn test_placeholder_and_delete() {
        let text = "张三向李四借款";
        let f1 = finding(0, 2, "张三", StrategyKind::Placeholder);
        let mut f2 = finding(3, 5, "李四", StrategyKind::Delete);
        f2.category = "person_name".into();
        let out = redact_text(text, &[f1, f2]);
        assert_eq!(out.text, "【身份证号】向借款");
    }

    #[test]
    fn test_overlap_guard() {
        let text = "1234567890";
        let a = finding(0, 6, "123456", StrategyKind::Mask);
        let b = finding(3, 10, "4567890", StrategyKind::Mask);
        let out = redact_text(text, &[a, b]);
        // 第二个与第一个重叠，应被跳过
        assert_eq!(out.segments.len(), 1);
    }
}
