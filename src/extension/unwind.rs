use std::borrow::Cow;
use crate::extension::char2sd_list::*;
use crate::stardust::{ErrorKind, StardustError, Token};

impl StardustError {
    fn code_point_too_large(c: char) -> Self {
        StardustError {
            kind: ErrorKind::CodePointTooLarge,
            span: None,
            message: format!("character '{}' has code point {} which exceeds safety limit", c, c as u32),
        }
    }
}

/// 对输入文本进行预处理：将 ASCII 字母和数字替换为预定义的常量字符串。
///
/// 替换规则：
/// - `A`..`Z` → `A`..`Z` 常量
/// - `a`..`z` → `M_A`..`M_Z` 常量
/// - `0`..`9` → `N_0`..`N_9` 常量
/// - 其他字符保持不变
///
/// 如果输入中不包含任何需要替换的字符，则返回 `Cow::Borrowed(input)`，
/// 避免不必要的内存分配，仅处理单个字符。
pub fn preprocess(input: &str) -> Result<Cow<'_, str>, StardustError> {
    // 若没有 ASCII 字母数字则无需替换
    if !input.chars().any(|c| c.is_ascii_alphanumeric()) {
        return Ok(Cow::Borrowed(input));
    }

    let mut result = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            'A'..='Z' => result.push_str(upper_replacement(ch)),
            'a'..='z' => result.push_str(lower_replacement(ch)),
            '0'..='9' => result.push_str(digit_replacement(ch)),
            _ => result.push(ch),
        }
    }
    Ok(Cow::Owned(result))
}

fn upper_replacement(c: char) -> &'static str {
    match c {
        'A' => SD_A, 'B' => SD_B, 'C' => SD_C, 'D' => SD_D, 'E' => SD_E,
        'F' => SD_F, 'G' => SD_G, 'H' => SD_H, 'I' => SD_I, 'J' => SD_J,
        'K' => SD_K, 'L' => SD_L, 'M' => SD_M, 'N' => SD_N, 'O' => SD_O,
        'P' => SD_P, 'Q' => SD_Q, 'R' => SD_R, 'S' => SD_S, 'T' => SD_T,
        'U' => SD_U, 'V' => SD_V, 'W' => SD_W, 'X' => SD_X, 'Y' => SD_Y,
        'Z' => SD_Z,
        _ => unreachable!(),
    }
}

fn lower_replacement(c: char) -> &'static str {
    match c {
        'a' => SD_M_A, 'b' => SD_M_B, 'c' => SD_M_C, 'd' => SD_M_D, 'e' => SD_M_E,
        'f' => SD_M_F, 'g' => SD_M_G, 'h' => SD_M_H, 'i' => SD_M_I, 'j' => SD_M_J,
        'k' => SD_M_K, 'l' => SD_M_L, 'm' => SD_M_M, 'n' => SD_M_N, 'o' => SD_M_O,
        'p' => SD_M_P, 'q' => SD_M_Q, 'r' => SD_M_R, 's' => SD_M_S, 't' => SD_M_T,
        'u' => SD_M_U, 'v' => SD_M_V, 'w' => SD_M_W, 'x' => SD_M_X, 'y' => SD_M_Y,
        'z' => SD_M_Z,
        _ => unreachable!(),
    }
}

fn digit_replacement(c: char) -> &'static str {
    match c {
        '0' => SD_N_0, '1' => SD_N_1, '2' => SD_N_2, '3' => SD_N_3, '4' => SD_N_4,
        '5' => SD_N_5, '6' => SD_N_6, '7' => SD_N_7, '8' => SD_N_8, '9' => SD_N_9,
        _ => unreachable!(),
    }
}

pub fn simple_preprocess(input: &str) -> Result<Cow<'_, str>, StardustError> {
    const MAX_CODE_POINT: u32 = 0x7F; // 仅 ASCII 字符

    // 计算所需总容量，验证字符合法性
    let mut required_capacity = 0;
    for ch in input.chars() {
        let code = ch as u32;
        if ch.is_ascii_alphanumeric() {
            if code > MAX_CODE_POINT {
                return Err(StardustError::code_point_too_large(ch));
            }
            let spaces = (code as usize) + 5;
            required_capacity += spaces + 1;
        } else {
            required_capacity += ch.len_utf8();
        }
    }

    // 构建字符串
    let mut result = String::with_capacity(required_capacity);
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            let spaces = (ch as usize) + 5;
            for _ in 0..spaces {
                result.push(' ');
            }
            result.push('+');
        } else {
            result.push(ch);
        }
    }

    Ok(Cow::Owned(result))
}
