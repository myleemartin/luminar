//! 法律行业内置识别规则（预设）
//!
//! 内置规则在首次启动时写入 `rules.json`，之后可由律师在「规则管理」面板中
//! 修改脱敏方式、置信度、启停，甚至删除；点「恢复内置规则」可还原。
//!
//! 注意：Rust `regex` 库不支持后行断言（lookbehind），需要边界的规则通过
//! `require_boundary` 在匹配后二次校验，或使用捕获组 + 前置字符类实现。

use crate::models::{Rule, RuleKind, StrategyConfig, StrategyKind, Validator};

fn mask(keep_prefix: usize, keep_suffix: usize) -> StrategyConfig {
    StrategyConfig {
        kind: StrategyKind::Mask,
        mask_char: "*".into(),
        keep_prefix,
        keep_suffix,
        placeholder: "【{type}】".into(),
    }
}

fn placeholder() -> StrategyConfig {
    StrategyConfig {
        kind: StrategyKind::Placeholder,
        mask_char: "*".into(),
        keep_prefix: 0,
        keep_suffix: 0,
        placeholder: "【{type}】".into(),
    }
}

fn delete() -> StrategyConfig {
    StrategyConfig {
        kind: StrategyKind::Delete,
        mask_char: "*".into(),
        keep_prefix: 0,
        keep_suffix: 0,
        placeholder: "【{type}】".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn rule(
    id: &str,
    name: &str,
    category: &str,
    label: &str,
    kind: RuleKind,
    pattern: &str,
    confidence: f32,
    strategy: StrategyConfig,
    description: &str,
) -> Rule {
    Rule {
        id: id.into(),
        name: name.into(),
        category: category.into(),
        category_label: label.into(),
        kind,
        pattern: pattern.into(),
        keywords: vec![],
        capture_group: 0,
        confidence,
        strategy,
        validator: Validator::None,
        priority: 50,
        enabled: true,
        require_boundary: false,
        case_insensitive: false,
        builtin: true,
        description: description.into(),
    }
}

/// 返回全部内置规则
pub fn builtin_rules() -> Vec<Rule> {
    let mut rules = vec![];

    // 1. 身份证号：18 位（含 X）与 15 位旧号，带校验位验证
    let mut id_card = rule(
        "builtin_id_card",
        "身份证号",
        "id_card",
        "身份证号",
        RuleKind::Regex,
        r"\d{17}[0-9Xx]|\d{15}",
        0.96,
        mask(0, 0),
        "居民身份证号码（18 位 / 15 位旧号）。校验位不通过时自动降权，低于阈值不展示。",
    );
    id_card.validator = Validator::IdCard;
    id_card.require_boundary = true;
    id_card.priority = 100;
    rules.push(id_card);

    // 2. 手机号
    let mut phone = rule(
        "builtin_phone",
        "手机号",
        "phone",
        "手机号",
        RuleKind::Regex,
        r"(?:\+?86[-\s]?)?1[3-9]\d{9}",
        0.94,
        mask(3, 4),
        "中国大陆 11 位手机号，可识别 +86 / 86 前缀。默认保留前 3 后 4 位。",
    );
    phone.require_boundary = true;
    phone.priority = 95;
    rules.push(phone);

    // 3. 固定电话
    let mut landline = rule(
        "builtin_landline",
        "固定电话",
        "landline",
        "固定电话",
        RuleKind::Regex,
        r"(?:\(0\d{2,3}\)|0\d{2,3})[-\s]?\d{7,8}(?:[-\s]?\d{1,6})?",
        0.86,
        mask(0, 4),
        "含区号的固定电话 / 传真，支持 027-88888888-123 格式。",
    );
    landline.require_boundary = true;
    landline.priority = 80;
    rules.push(landline);

    // 4. 银行卡号：Luhn 校验失败的候选自动降权
    let mut bank = rule(
        "builtin_bank_card",
        "银行卡号",
        "bank_card",
        "银行卡号",
        RuleKind::Regex,
        r"\d{13,19}",
        0.95,
        mask(0, 4),
        "13~19 位银行卡号，使用 Luhn 算法校验；校验不过按 0.5 倍置信度保留。",
    );
    bank.validator = Validator::Luhn;
    bank.require_boundary = true;
    bank.priority = 90;
    rules.push(bank);

    // 5. 电子邮箱
    rules.push(rule(
        "builtin_email",
        "电子邮箱",
        "email",
        "电子邮箱",
        RuleKind::Regex,
        r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,10}",
        0.97,
        mask(1, 0),
        "常规邮箱地址。",
    ));

    // 6. 住址：省市区 + 路街号 + 楼栋室
    rules.push(rule(
        "builtin_address",
        "住址",
        "address",
        "住址",
        RuleKind::Regex,
        r"(?:(?:北京|天津|上海|重庆|河北|山西|辽宁|吉林|黑龙江|江苏|浙江|安徽|福建|江西|山东|河南|湖北|湖南|广东|海南|四川|贵州|云南|陕西|甘肃|青海|内蒙古|广西|西藏|宁夏|新疆)[省市自治区]{0,3})?[\u4e00-\u9fa5]{1,6}(?:市|自治州|地区)[\u4e00-\u9fa5]{1,6}(?:区|县|旗)[\u4e00-\u9fa50-9]{2,30}(?:路|街|道|巷|弄|大道|胡同)[\u4e00-\u9fa50-9]{0,15}(?:号)?(?:[A-Za-z0-9\u4e00-\u9fa5]{0,20}(?:号楼|栋|单元|室|层|楼))?",
        0.8,
        placeholder(),
        "识别「省+市+区+路/街+号+楼栋室」结构的通讯地址，可识别省略省份的写法。",
    ));

    // 7. 当事人姓名：内置 NER，基于姓氏表 + 上下文线索
    rules.push(Rule {
        id: "builtin_person_name".into(),
        name: "当事人姓名（智能识别）".into(),
        category: "person_name".into(),
        category_label: "当事人姓名".into(),
        kind: RuleKind::Ner,
        pattern: String::new(),
        keywords: vec![],
        capture_group: 0,
        confidence: 0.82,
        strategy: placeholder(),
        validator: Validator::None,
        priority: 60,
        enabled: true,
        require_boundary: false,
        case_insensitive: false,
        builtin: true,
        description: "依据「原告／被告／申请人／委托人／姓名：」等上下文线索 + 百家姓表识别人名，非机器学习模型，零网络依赖。".into(),
    });

    // 8. 代理人
    rules.push(rule(
        "builtin_agent",
        "代理人",
        "agent",
        "代理人",
        RuleKind::Regex,
        r"(?:委托诉讼代理人|委托代理人|诉讼代理人|代理人|辩护人)[：:\s]{0,3}([\u4e00-\u9fa5·]{2,4})",
        0.88,
        placeholder(),
        "识别「代理人：张三」等表述，取第一个捕获组。",
    ));
    if let Some(r) = rules.last_mut() {
        r.capture_group = 1;
        r.priority = 70;
    }

    // 9. 律所名称
    rules.push(rule(
        "builtin_law_firm",
        "律所名称",
        "law_firm",
        "律所名称",
        RuleKind::Regex,
        r"(?:[\s，,。；;：:、（）()《》]|^|由|在|向|与|和|系|为|委托|聘请|指派)([\u4e00-\u9fa5]{2,15}(?:[（(][\u4e00-\u9fa5]{1,8}[）)])?[\u4e00-\u9fa5]{0,8}(?:律师事务所|法律服务所))",
        0.88,
        placeholder(),
        "识别「XX律师事务所」「XX（武汉）律师事务所」，取第一个捕获组。",
    ));
    if let Some(r) = rules.last_mut() {
        r.capture_group = 1;
        r.priority = 75;
    }

    // 10. 案号
    rules.push(rule(
        "builtin_case_number",
        "案号",
        "case_number",
        "案号",
        RuleKind::Regex,
        r"[（(]\s?\d{4}\s?[）)]\s?[\u4e00-\u9fa5]{1,4}\d{0,4}[\u4e00-\u9fa5]{1,3}\d{1,6}\s?号",
        0.93,
        placeholder(),
        "如（2024）鄂0102民初1234号，支持全角/半角括号。",
    ));

    // 11. 法院名称
    rules.push(rule(
        "builtin_court",
        "法院名称",
        "court",
        "法院名称",
        RuleKind::Regex,
        r"(?:[\s，,。；;：:、（）()《》]|^|由|向|在|至|经)([\u4e00-\u9fa5]{2,12}(?:人民法院|法院))",
        0.85,
        placeholder(),
        "识别各级人民法院名称，取第一个捕获组。",
    ));
    if let Some(r) = rules.last_mut() {
        r.capture_group = 1;
        r.priority = 72;
    }

    // 12. 法院编号 / 法院代字
    rules.push(rule(
        "builtin_court_code",
        "法院编号",
        "court_code",
        "法院编号",
        RuleKind::Regex,
        r"(?:法院代码|法院编号|法院代字)[：:\s]{0,3}([A-Za-z0-9]{2,12})",
        0.86,
        placeholder(),
        "识别「法院代码：420102」等编号，取第一个捕获组。",
    ));
    if let Some(r) = rules.last_mut() {
        r.capture_group = 1;
        r.priority = 60;
    }

    // 13. 统一社会信用代码
    let mut uscc = rule(
        "builtin_credit_code",
        "统一社会信用代码",
        "credit_code",
        "统一社会信用代码",
        RuleKind::Regex,
        r"[0-9A-HJ-NPQRTUWXY]{2}\d{6}[0-9A-HJ-NPQRTUWXY]{10}",
        0.9,
        mask(0, 6),
        "企业/组织统一社会信用代码，校验位不通过时降权。",
    );
    uscc.validator = Validator::Uscc;
    uscc.require_boundary = true;
    uscc.priority = 65;
    rules.push(uscc);

    // 14. 关键词示例（默认关闭，供律师照葫芦画瓢）
    let mut kw = rule(
        "builtin_keyword_demo",
        "关键词示例：项目代号",
        "custom",
        "自定义",
        RuleKind::Keyword,
        "",
        0.95,
        delete(),
        "示例规则：把关键词列表中出现的词整体替换为占位标签。可在规则面板中改为自己的案件代号、客户简称等。",
    );
    kw.keywords = vec!["示例项目代号".into()];
    kw.enabled = false;
    kw.priority = 30;
    rules.push(kw);

    rules
}

/// 全部内置分类
pub fn builtin_categories() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id_card", "身份证号"),
        ("phone", "手机号"),
        ("landline", "固定电话"),
        ("bank_card", "银行卡号"),
        ("email", "电子邮箱"),
        ("address", "住址"),
        ("person_name", "当事人姓名"),
        ("agent", "代理人"),
        ("law_firm", "律所名称"),
        ("case_number", "案号"),
        ("court", "法院名称"),
        ("court_code", "法院编号"),
        ("credit_code", "统一社会信用代码"),
        ("org_name", "单位名称"),
        ("custom", "自定义"),
    ]
}
