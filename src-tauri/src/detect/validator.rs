//! 候选校验器：用校验位等硬规则降低误报
//!
//! 注意策略：校验通过只做轻微加分，校验失败只做轻微降权——
//! 文书脱敏宁可多标，律师在右栏一键取消即可；但也不希望错误号码高分干扰复核。

/// 18 位身份证校验位（ISO 7064:1983, MOD 11-2）
pub fn id_card_valid(s: &str) -> bool {
    let s = s.trim();
    if s.len() == 15 {
        // 旧版 15 位：全数字，出生日期 6 位
        return s.chars().all(|c| c.is_ascii_digit());
    }
    if s.len() != 18 {
        return false;
    }
    let bytes = s.as_bytes();
    let mut sum = 0u32;
    let weights = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    for (i, w) in weights.iter().enumerate() {
        let c = bytes[i];
        if !c.is_ascii_digit() {
            return false;
        }
        sum += (c - b'0') as u32 * w;
    }
    let check = b"10X98765432"[(sum % 11) as usize] as char;
    let last = (bytes[17] as char).to_ascii_uppercase();
    last == check
}

/// Luhn（银行卡号）校验
pub fn luhn_valid(s: &str) -> bool {
    let digits: Vec<u32> = s
        .chars()
        .filter(|c| c.is_ascii_digit())
        .map(|c| c.to_digit(10).unwrap_or(0))
        .collect();
    if digits.len() < 12 {
        return false;
    }
    let mut sum = 0u32;
    let parity = digits.len() % 2;
    for (i, d) in digits.iter().enumerate() {
        let mut v = *d;
        if i % 2 == parity {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    sum % 10 == 0
}

/// 统一社会信用代码校验（GB 32100-2015）
pub fn uscc_valid(s: &str) -> bool {
    const CHARS: &[u8] = b"0123456789ABCDEFGHJKLMNPQRTUWXY";
    const WEIGHTS: [u32; 17] = [
        1, 3, 9, 27, 19, 26, 16, 17, 20, 29, 25, 13, 8, 24, 10, 30, 28,
    ];
    let s = s.trim().to_uppercase();
    let bytes = s.as_bytes();
    if bytes.len() != 18 {
        return false;
    }
    let mut sum = 0u32;
    for i in 0..17 {
        let pos = match CHARS.iter().position(|c| *c == bytes[i]) {
            Some(p) => p as u32,
            None => return false,
        };
        sum += pos * WEIGHTS[i];
    }
    let check = (31 - sum % 31) % 31;
    CHARS[check as usize] == bytes[17]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_card_checksum() {
        // 测试用示例号码（校验位正确）
        assert!(id_card_valid("11010519491231002X"));
        assert!(!id_card_valid("110105194912310021"));
    }

    #[test]
    fn luhn() {
        assert!(luhn_valid("4111111111111111"));
        assert!(!luhn_valid("4111111111111112"));
    }
}
