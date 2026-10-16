use crate::extension::char2sd_list::*;
use crate::stardust::StardustError;
use std::borrow::Cow;

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
        'A' => SD_A,
        'B' => SD_B,
        'C' => SD_C,
        'D' => SD_D,
        'E' => SD_E,
        'F' => SD_F,
        'G' => SD_G,
        'H' => SD_H,
        'I' => SD_I,
        'J' => SD_J,
        'K' => SD_K,
        'L' => SD_L,
        'M' => SD_M,
        'N' => SD_N,
        'O' => SD_O,
        'P' => SD_P,
        'Q' => SD_Q,
        'R' => SD_R,
        'S' => SD_S,
        'T' => SD_T,
        'U' => SD_U,
        'V' => SD_V,
        'W' => SD_W,
        'X' => SD_X,
        'Y' => SD_Y,
        'Z' => SD_Z,
        _ => unreachable!(),
    }
}

fn lower_replacement(c: char) -> &'static str {
    match c {
        'a' => SD_M_A,
        'b' => SD_M_B,
        'c' => SD_M_C,
        'd' => SD_M_D,
        'e' => SD_M_E,
        'f' => SD_M_F,
        'g' => SD_M_G,
        'h' => SD_M_H,
        'i' => SD_M_I,
        'j' => SD_M_J,
        'k' => SD_M_K,
        'l' => SD_M_L,
        'm' => SD_M_M,
        'n' => SD_M_N,
        'o' => SD_M_O,
        'p' => SD_M_P,
        'q' => SD_M_Q,
        'r' => SD_M_R,
        's' => SD_M_S,
        't' => SD_M_T,
        'u' => SD_M_U,
        'v' => SD_M_V,
        'w' => SD_M_W,
        'x' => SD_M_X,
        'y' => SD_M_Y,
        'z' => SD_M_Z,
        _ => unreachable!(),
    }
}

fn digit_replacement(c: char) -> &'static str {
    match c {
        '0' => SD_N_0,
        '1' => SD_N_1,
        '2' => SD_N_2,
        '3' => SD_N_3,
        '4' => SD_N_4,
        '5' => SD_N_5,
        '6' => SD_N_6,
        '7' => SD_N_7,
        '8' => SD_N_8,
        '9' => SD_N_9,
        _ => unreachable!(),
    }
}

pub fn simple_preprocess(input: &str) -> Result<Cow<'_, str>, StardustError> {
    // 没有可替换字符时零分配返回
    if !input.chars().any(|c| c.is_ascii_alphanumeric()) {
        return Ok(Cow::Borrowed(input));
    }

    // 每个字母数字最多展开成 133 字节（127 + 5 空格 + '+'）；
    // 这里只给个粗略提示，真正的扩容交给 String 自己。
    let alnum = input.chars().filter(|c| c.is_ascii_alphanumeric()).count();
    let mut result = String::with_capacity(input.len() + alnum * 8);
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        // 注释 `//` 到行尾的内容必须原样保留。
        // 注释里的字母数字是给人看的说明文字，展开成 Push 序列虽然不影响
        // 执行（仍在注释行内），但会让 --dump 的预处理阶段输出变得不可读：
        // 一个 13 字节的 `// print ABC` 会膨胀到 800 多字节。
        if ch == '/' && chars.peek() == Some(&'/') {
            result.push_str("//");
            chars.next(); // 消耗第二个 '/'
            for c in chars.by_ref() {
                result.push(c);
                if c == '\n' {
                    break;
                }
            }
            continue;
        }

        if ch.is_ascii_alphanumeric() {
            for _ in 0..(ch as usize + 5) {
                result.push(' ');
            }
            result.push('+');
        } else {
            result.push(ch);
        }
    }

    Ok(Cow::Owned(result))
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── simple_preprocess ──────────────────────────────────────────────────

    #[test]
    fn simple_empty_returns_empty() {
        let result = simple_preprocess("").unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn simple_symbols_unchanged() {
        let result = simple_preprocess(" +*,").unwrap();
        assert_eq!(result, " +*,");
    }

    #[test]
    fn simple_uppercase_h() {
        // 'H' = ASCII 72 → (72+5)=77 spaces + '+'
        let result = simple_preprocess("H").unwrap();
        assert_eq!(result, format!("{}+", " ".repeat(77)));
    }

    #[test]
    fn simple_lowercase_a() {
        // 'a' = ASCII 97 → (97+5)=102 spaces + '+'
        let result = simple_preprocess("a").unwrap();
        assert_eq!(result, format!("{}+", " ".repeat(102)));
    }

    #[test]
    fn simple_digit_zero() {
        // '0' = ASCII 48 → (48+5)=53 spaces + '+'
        let result = simple_preprocess("0").unwrap();
        assert_eq!(result, format!("{}+", " ".repeat(53)));
    }

    #[test]
    fn simple_mixed_chars_and_symbols() {
        // "A+" → Push(65) + literal '+'
        let result = simple_preprocess("A+").unwrap();
        assert_eq!(result, format!("{}+{}", " ".repeat(70), "+"));
    }

    #[test]
    fn simple_output_is_valid_stardust() {
        // 验证 simple_preprocess 的输出可以被 tokenize 解析
        use crate::stardust::lexer::tokenize;
        let result = simple_preprocess("Hi").unwrap();
        let tokens = tokenize(&result);
        assert!(tokens.is_ok(), "preprocess output should be tokenizable");
        let tokens = tokens.unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].spaces, 77); // H
        assert_eq!(tokens[1].spaces, 110); // i
    }

    #[test]
    fn simple_multiple_chars() {
        let result = simple_preprocess("AB").unwrap();
        assert_eq!(result, format!("{}+{}+", " ".repeat(70), " ".repeat(71)));
    }

    #[test]
    fn simple_preserves_comments() {
        let result = simple_preprocess("// Hi").unwrap();
        assert_eq!(result, "// Hi", "注释内容必须原样保留，不得展开成 Push 序列");
    }

    #[test]
    fn simple_comment_content_is_not_expanded() {
        // 回归测试：注释里的字母数字过去会被展开成 Push 序列，
        // 让 --dump 的预处理阶段输出从 13 字节膨胀到 800 多字节。
        let src = "// print ABC\n     +";
        let out = simple_preprocess(src).unwrap();
        assert!(
            out.starts_with("// print ABC\n"),
            "注释应原样保留，实际得到: {:?}",
            &out[..out.len().min(40)]
        );
        // 注释之后才是真正需要展开的代码部分
        assert!(out.len() < 80, "总长度不应因注释而膨胀，实际 {} 字节", out.len());
    }

    #[test]
    fn simple_expands_code_outside_comments() {
        // 确认跳过注释的同时，注释之外的字母数字仍被正常展开
        let out = simple_preprocess("// abc\nA").unwrap();
        assert!(out.starts_with("// abc\n"), "注释原样：{}", &out[..20]);
        // 'A' = 65 → 70 个空格 + '+'，共 71 字节
        assert_eq!(out.len(), "// abc\n".len() + 71);
    }

    #[test]
    fn simple_single_slash_is_not_a_comment() {
        // 单独的 `/` 不是注释起始，仍按原样交给词法器报 InvalidAnnotation
        let out = simple_preprocess("/ A").unwrap();
        assert!(out.starts_with('/'), "单个斜杠应原样保留");
    }

    #[test]
    fn simple_comment_at_eof_without_newline() {
        // 注释在文件末尾且无换行符
        let out = simple_preprocess("     +\n// tail").unwrap();
        assert!(out.ends_with("// tail"), "结尾注释应完整保留");
    }

    #[test]
    fn simple_consecutive_comments() {
        let out = simple_preprocess("// a\n// b\nA").unwrap();
        assert!(out.starts_with("// a\n// b\n"), "连续注释都应保留");
    }

    // ── preprocess ─────────────────────────────────────────────────────────

    #[test]
    fn preprocess_empty_borrowed() {
        let result = preprocess("").unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn preprocess_symbols_borrowed() {
        let result = preprocess(" +*,").unwrap();
        assert_eq!(result.as_ref(), " +*,");
    }

    #[test]
    fn preprocess_uppercase_a() {
        let result = preprocess("A").unwrap();
        assert_eq!(result.as_ref(), SD_A);
    }

    #[test]
    fn preprocess_lowercase_a() {
        let result = preprocess("a").unwrap();
        assert_eq!(result.as_ref(), SD_M_A);
    }

    #[test]
    fn preprocess_digit_0() {
        let result = preprocess("0").unwrap();
        assert_eq!(result.as_ref(), SD_N_0);
    }

    #[test]
    fn preprocess_mixed_content() {
        let result = preprocess("A+").unwrap();
        assert!(result.contains(SD_A));
        assert!(result.contains('+'));
    }
}
