//! 轻量中文人名识别（NER）
//!
//! 不使用任何大模型，完全离线、可解释：
//! 1. 用「原告 / 被告 / 申请人 / 姓名：/ 代理人」等上下文线索定位候选人名；
//! 2. 用百家姓（含复姓）校验姓氏，过滤「公司 / 银行 / 法院」等机构词；
//! 3. 用停用词、长度、字符类别做后处理，输出字符下标。
//!
//! 与正则规则互补：正则抓「代理人：张三」，这里补充「原告张三诉被告李四」等无分隔写法。

/// 单字姓（覆盖常见 300+ 姓氏）
const SURNAMES: &[&str] = &[
    "赵", "钱", "孙", "李", "周", "吴", "郑", "王", "冯", "陈", "褚", "卫", "蒋", "沈", "韩", "杨",
    "朱", "秦", "尤", "许", "何", "吕", "施", "张", "孔", "曹", "严", "华", "金", "魏", "陶", "姜",
    "戚", "谢", "邹", "喻", "柏", "水", "窦", "章", "云", "苏", "潘", "葛", "奚", "范", "彭", "郎",
    "鲁", "韦", "昌", "马", "苗", "凤", "花", "方", "俞", "任", "袁", "柳", "酆", "鲍", "史", "唐",
    "费", "廉", "岑", "薛", "雷", "贺", "倪", "汤", "滕", "殷", "罗", "毕", "郝", "邬", "安", "常",
    "乐", "于", "时", "傅", "皮", "卞", "齐", "康", "伍", "余", "元", "卜", "顾", "孟", "平", "黄",
    "和", "穆", "萧", "尹", "姚", "邵", "湛", "汪", "祁", "毛", "禹", "狄", "米", "贝", "明", "臧",
    "计", "伏", "成", "戴", "谈", "宋", "茅", "庞", "熊", "纪", "舒", "屈", "项", "祝", "董", "梁",
    "杜", "阮", "蓝", "闵", "席", "季", "麻", "强", "贾", "路", "娄", "危", "江", "童", "颜", "郭",
    "梅", "盛", "林", "刁", "钟", "徐", "邱", "骆", "高", "夏", "蔡", "田", "樊", "胡", "凌", "霍",
    "虞", "万", "支", "柯", "昝", "管", "卢", "莫", "经", "房", "裘", "缪", "干", "解", "应", "宗",
    "丁", "宣", "贲", "邓", "郁", "单", "杭", "洪", "包", "诸", "左", "石", "崔", "吉", "钮", "龚",
    "程", "嵇", "邢", "滑", "裴", "陆", "荣", "翁", "荀", "羊", "於", "惠", "甄", "曲", "家", "封",
    "芮", "羿", "储", "靳", "汲", "邴", "糜", "松", "井", "段", "富", "巫", "乌", "焦", "巴", "弓",
    "牧", "隗", "山", "谷", "车", "侯", "宓", "蓬", "全", "郗", "班", "仰", "秋", "仲", "伊", "宫",
    "宁", "仇", "栾", "暴", "甘", "钭", "厉", "戎", "祖", "武", "符", "刘", "景", "詹", "束", "龙",
    "叶", "幸", "司", "韶", "郜", "黎", "蓟", "薄", "印", "宿", "白", "怀", "蒲", "邰", "从", "鄂",
    "索", "咸", "籍", "赖", "卓", "蔺", "屠", "蒙", "池", "乔", "阴", "胥", "能", "苍", "双", "闻",
    "莘", "党", "翟", "谭", "贡", "劳", "逄", "姬", "申", "扶", "堵", "冉", "宰", "郦", "雍", "郤",
    "璩", "桑", "桂", "濮", "牛", "寿", "通", "边", "扈", "燕", "冀", "郏", "浦", "尚", "农", "温",
    "别", "庄", "晏", "柴", "瞿", "阎", "充", "慕", "连", "茹", "习", "宦", "艾", "鱼", "容", "向",
    "古", "易", "慎", "戈", "廖", "庾", "终", "暨", "居", "衡", "步", "都", "耿", "满", "弘", "匡",
    "国", "文", "寇", "广", "禄", "阙", "东", "欧", "殳", "沃", "利", "蔚", "越", "夔", "隆", "师",
    "巩", "厍", "聂", "晁", "勾", "敖", "融", "冷", "訾", "辛", "阚", "那", "简", "饶", "空", "曾",
    "毋", "沙", "乜", "养", "鞠", "须", "丰", "巢", "关", "蒯", "相", "查", "后", "荆", "红", "游",
    "竺", "权", "逯", "盖", "益", "桓", "公", "覃", "谌", "仇", "年", "爱", "阳", "佟", "商", "言",
];

/// 复姓
const COMPOUND_SURNAMES: &[&str] = &[
    "欧阳", "太史", "端木", "上官", "司马", "东方", "独孤", "南宫", "万俟", "闻人", "夏侯", "诸葛",
    "尉迟", "公羊", "赫连", "澹台", "皇甫", "宗政", "濮阳", "公冶", "太叔", "申屠", "公孙", "慕容",
    "仲孙", "钟离", "长孙", "宇文", "司徒", "鲜于", "司空", "闾丘", "子车", "亓官", "司寇", "巫马",
    "公西", "颛孙", "壤驷", "公良", "漆雕", "乐正", "宰父", "谷梁", "拓跋", "夹谷", "轩辕", "令狐",
    "段干", "百里", "呼延", "东郭", "南门", "羊舌", "微生", "公户", "公玉", "公仪", "梁丘", "公仲",
    "公上", "公门", "公山", "公坚", "左丘", "公伯", "西门", "公祖", "第五", "公乘", "贯丘", "公皙",
    "南荣", "东里", "东宫", "仲长", "子书", "子桑", "即墨", "达奚", "褚师", "吴铭",
];

/// 名字里不可能出现的字（机构、地址、时间等）
const NAME_STOP_CHARS: &[char] = &[
    '公', '司', '银', '行', '院', '局', '厅', '部', '委', '会', '政', '府', '市', '区', '县', '镇',
    '村', '街', '路', '号', '室', '楼', '栋', '层', '元', '角', '分', '年', '月', '日', '时', '法',
    '检', '警', '所', '站', '厂', '店', '校', '学', '医', '省', '国', '族', '党', '军', '团', '社',
    '队', '组', '系', '统', '科', '处', '办', '中', '心', '有', '限', '责', '任', '股', '份', '集',
    '体', '个', '体', '工', '商', '户', '农', '民', '合', '作', '信', '用', '储', '蓄', '卡', '账',
    '户', '编', '码', '代', '码', '证', '件', '件', '材', '料', '证', '据', '卷', '宗', '案', '件',
    '书', '请', '求', '事', '项', '内', '容', '签', '章', '盖', '章', '签', '字', '日', '期', '地',
    '址', '电', '话', '邮', '箱', '网', '址', '联',
];

/// 上下文线索：(线索词, 分类, 置信度)
const CUES: &[(&str, &str, f32)] = &[
    ("原告", "person_name", 0.86),
    ("被告", "person_name", 0.86),
    ("第三人", "person_name", 0.82),
    ("上诉人", "person_name", 0.84),
    ("被上诉人", "person_name", 0.84),
    ("申请人", "person_name", 0.82),
    ("被申请人", "person_name", 0.82),
    ("申请执行人", "person_name", 0.86),
    ("被执行人", "person_name", 0.86),
    ("被告人", "person_name", 0.86),
    ("被害人", "person_name", 0.78),
    ("犯罪嫌疑人", "person_name", 0.78),
    ("委托诉讼代理人", "agent", 0.9),
    ("委托代理人", "agent", 0.9),
    ("诉讼代理人", "agent", 0.88),
    ("代理人", "agent", 0.88),
    ("辩护人", "agent", 0.88),
    ("法定代表人", "person_name", 0.84),
    ("负责人", "person_name", 0.8),
    ("借款人", "person_name", 0.84),
    ("出借人", "person_name", 0.84),
    ("担保人", "person_name", 0.82),
    ("保证人", "person_name", 0.82),
    ("证人", "person_name", 0.76),
    ("经办人", "person_name", 0.78),
    ("联系人", "person_name", 0.76),
    ("委托人", "person_name", 0.8),
    ("姓名", "person_name", 0.8),
    ("当事人", "person_name", 0.78),
    ("甲方", "person_name", 0.78),
    ("乙方", "person_name", 0.78),
    ("丙方", "person_name", 0.76),
];

/// 线索与人名之间的分隔符
fn is_separator(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\u{3000}'
            | ':'
            | '：'
            | '，'
            | ','
            | '、'
            | '（'
            | '('
            | '）'
            | ')'
            | '"'
            | '\''
            | '“'
            | '”'
            | '《'
            | '》'
    )
}

/// 名字后面出现这些字说明名字已结束
fn is_name_end(c: char) -> bool {
    matches!(
        c,
        '，' | ','
            | '。'
            | '.'
            | '；'
            | ';'
            | '：'
            | ':'
            | ' '
            | '\n'
            | '\t'
            | '、'
            | '（'
            | '('
            | '）'
            | ')'
            | '《'
            | '》'
            | '“'
            | '”'
            | '诉'
            | '系'
            | '与'
            | '和'
            | '及'
            | '等'
            | '的'
            | '之'
            | '向'
            | '因'
            | '为'
            | '现'
            | '住'
            | '男'
            | '女'
            | '于'
            | '在'
            | '所'
    )
}

/// 命中
#[derive(Debug, Clone)]
pub struct NameHit {
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub category: &'static str,
    pub confidence: f32,
}

/// 对全文做人名识别
pub fn detect(text: &str) -> Vec<NameHit> {
    let chars: Vec<char> = text.chars().collect();
    let mut hits: Vec<NameHit> = Vec::new();

    // 线索按长度降序，优先匹配更长的线索（如「委托诉讼代理人」先于「代理人」）
    let mut cues: Vec<&(&str, &str, f32)> = CUES.iter().collect();
    cues.sort_by_key(|(c, _, _)| std::cmp::Reverse(c.chars().count()));

    for (cue, category, confidence) in cues {
        let cue: &str = cue;
        let category: &'static str = category;
        let confidence: f32 = *confidence;
        let cue_chars: Vec<char> = cue.chars().collect();
        if cue_chars.len() > chars.len() {
            continue;
        }
        let mut i = 0usize;
        while i + cue_chars.len() <= chars.len() {
            if chars[i..i + cue_chars.len()] != cue_chars[..] {
                i += 1;
                continue;
            }
            // 线索前不能紧贴姓氏（避免「原告代理人」这类复合词误触发两次）
            let mut j = i + cue_chars.len();
            while j < chars.len() && is_separator(chars[j]) {
                j += 1;
            }
            if let Some((name, end)) = read_name(&chars, j) {
                // 线索与姓名之间允许「（以下简称）」等修饰的情况直接跳过
                hits.push(NameHit {
                    start: j,
                    end,
                    name,
                    category,
                    confidence,
                });
            }
            i += cue_chars.len();
        }
    }

    // 去重：同一区间保留置信度最高者
    hits.sort_by(|a, b| {
        a.start.cmp(&b.start).then(a.end.cmp(&b.end)).then(
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    hits.dedup_by(|a, b| a.start == b.start && a.end == b.end);
    hits
}

/// 从位置 i 读取一个人名，返回（姓名，结束下标）
///
/// 中文姓名没有分隔符，这里用「姓氏表 + 连接词边界」做保守判断：
/// - 遇到「诉 / 与 / 和 / 系 / 等」等连接词立即截断，避免把「张三诉被告李四」读成一串；
/// - 默认取 2 字（复姓 3 字），只有整段姓名独立成词（后面跟标点或连接词）时才取 3~4 字。
fn read_name(chars: &[char], i: usize) -> Option<(String, usize)> {
    let mut start = i;
    let mut guard = 0;
    while start < chars.len() && is_separator(chars[start]) && guard < 6 {
        start += 1;
        guard += 1;
    }
    if start >= chars.len() {
        return None;
    }

    // 收集连续的汉字，遇到连接词/标点停止
    let mut run: Vec<char> = Vec::new();
    let mut k = start;
    while k < chars.len() && run.len() < 8 {
        let c = chars[k];
        if is_cjk(c) && !is_run_stop(c) {
            run.push(c);
            k += 1;
        } else {
            break;
        }
    }
    if run.len() < 2 {
        return None;
    }

    // 姓氏校验：复姓优先
    let mut base = 0usize;
    for c in COMPOUND_SURNAMES {
        let cs: Vec<char> = c.chars().collect();
        if run.len() > cs.len() && run[..cs.len()] == cs[..] {
            base = cs.len() + 1;
            break;
        }
    }
    if base == 0 {
        let first = run[0].to_string();
        if !SURNAMES.contains(&first.as_str()) {
            return None;
        }
        base = 2;
    }
    if run.len() < base {
        return None;
    }

    // 决定取几个字
    let mut len = base;
    if run.len() > base {
        let after_run = chars.get(start + run.len()).copied();
        let standalone = after_run
            .map(|c| !is_cjk(c) || is_name_end(c))
            .unwrap_or(true);
        if standalone && run.len() <= 4 {
            // 整段就是姓名，例如文末签名「张三峰」
            len = run.len();
        } else if run.get(base).map(|c| is_name_end(*c)).unwrap_or(false) {
            // 第 3 字后是「诉」等连接词，例如「张三峰诉李四」
            len = base + 1;
        }
    }

    // 名字后面紧跟的汉字：若是常见词首（借/合/纠/公…）说明姓名已结束；
    // 若是其他汉字（如「原告通过银行」的「银」），说明只是更长词语的前缀，放弃。
    if let Some(next) = chars.get(start + len).copied() {
        if is_cjk(next) && !is_name_end(next) && !is_run_stop(next) && !is_follow_word(next) {
            return None;
        }
    }

    // 后处理：末字不能是机构/地址/时间类字
    if NAME_STOP_CHARS.contains(&run[len - 1]) {
        len = base;
        if NAME_STOP_CHARS.contains(&run[len - 1]) {
            return None;
        }
    }
    let name: String = run[..len].iter().collect();
    if name.chars().count() < 2 {
        return None;
    }
    let end = start + len;
    Some((name, end))
}

/// 姓名后常见的「下一句/下一个词」的词首字。
/// 例如「被告李四民间借贷纠纷」中「民」说明姓名到此为止；
/// 而「原告通过银行」中「银」不属于此类，说明「通过」是动词而不是姓名。
fn is_follow_word(c: char) -> bool {
    matches!(
        c,
        '民' | '借'
            | '合'
            | '买'
            | '卖'
            | '租'
            | '房'
            | '车'
            | '款'
            | '钱'
            | '工'
            | '服'
            | '贸'
            | '投'
            | '担'
            | '保'
            | '离'
            | '婚'
            | '抚'
            | '继'
            | '收'
            | '侵'
            | '交'
            | '劳'
            | '建'
            | '加'
            | '欠'
            | '还'
            | '付'
            | '赔'
            | '损'
            | '签'
            | '约'
            | '履'
            | '违'
            | '解'
            | '终'
            | '裁'
            | '判'
            | '执'
            | '诉'
            | '称'
            | '辩'
            | '述'
            | '请'
            | '申'
            | '系'
            | '住'
            | '户'
            | '账'
            | '开'
            | '持'
            | '领'
    )
}

/// 采集姓名时遇到这些字立即截断（避免把后续动词/连词吞进姓名）
fn is_run_stop(c: char) -> bool {
    matches!(
        c,
        '诉' | '与'
            | '和'
            | '及'
            | '系'
            | '等'
            | '向'
            | '因'
            | '为'
            | '住'
            | '男'
            | '女'
            | '之'
            | '的'
            | '于'
            | '在'
            | '所'
            | '现'
    )
}

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{4e00}'..='\u{9fff}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        detect(text).into_iter().map(|h| h.name).collect()
    }

    #[test]
    fn test_plain() {
        let hits = names("原告张三诉被告李四民间借贷纠纷一案");
        assert!(hits.contains(&"张三".to_string()), "命中：{hits:?}");
        assert!(hits.contains(&"李四".to_string()), "命中：{hits:?}");
    }

    #[test]
    fn test_agent() {
        let hits = detect("委托诉讼代理人：王五，湖北观筑律师事务所律师。");
        assert!(hits
            .iter()
            .any(|h| h.name == "王五" && h.category == "agent"));
    }

    #[test]
    fn test_company_not_name() {
        let hits = names("原告湖北观筑律师事务所");
        assert!(!hits.contains(&"湖北".to_string()));
    }
}
