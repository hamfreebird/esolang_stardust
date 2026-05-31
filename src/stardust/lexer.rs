use crate::stardust::{ErrorKind, SourceSpan, StardustError, Token, TokenType};
use std::iter::Peekable;
use std::str::Chars;

pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    line: usize,
    column: usize,
    byte_pos: usize, // 当前字节偏移
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Lexer {
            chars: input.chars().peekable(),
            line: 1,
            column: 1,
            byte_pos: 0,
        }
    }

    /// 消耗下一个字符并更新行列信息
    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.next()?;
        self.byte_pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    /// 查看下一个字符但不消耗
    fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    /// 消费 `//` 注释内容（调用前已消耗第一个 `/`）。
    ///
    /// 若紧随的字符是 `/` 则跳过直到行尾；否则返回 `InvalidAnnotation`。
    fn skip_comment(&mut self, span: &SourceSpan) -> Result<(), StardustError> {
        if self.peek() == Some(&'/') {
            self.advance(); // 消耗第二个 /
            while let Some(&ch) = self.peek() {
                if ch == '\n' { break; }
                self.advance();
            }
            Ok(())
        } else {
            Err(StardustError::new(ErrorKind::InvalidAnnotation, Some(span.clone())))
        }
    }

    /// 解析一个 token，忽略非指令空白
    pub fn next_token(&mut self) -> Option<Result<Token, StardustError>> {
        let mut spaces = 0;

        loop {
            // 每次循环迭代记录当前位置——确保跳过空白行后行号正确更新
            let start_line = self.line;
            let start_col = self.column;
            let span = SourceSpan {
                line: start_line,
                column: start_col,
            };

            match self.peek() {
                Some(&' ') => {
                    // 进入空格计数模式
                    self.advance();
                    spaces += 1;
                    // 持续读取连续空格
                    while let Some(&' ') = self.peek() {
                        self.advance();
                        spaces += 1;
                    }

                    // 空格后必须紧跟符号才能构成 token
                    match self.peek() {
                        Some(&ch) if is_symbol(ch) => {
                            let token_type = char_to_token_type(ch).unwrap();
                            let byte_pos_of_symbol = self.byte_pos;
                            self.advance(); // 消耗符号
                            return Some(Ok(Token {
                                spaces,
                                token_type,
                                line: start_line,
                                column: start_col,
                                byte_pos: byte_pos_of_symbol,
                            }));
                        }
                        Some(&ch) if is_anno(ch) => {
                            self.advance(); // 消耗第一个 /
                            if let Err(e) = self.skip_comment(&span) {
                                return Some(Err(e));
                            }
                        }
                        Some(&ch) if ch.is_whitespace() => {
                            // 空格后遇到换行/制表符等 → 丢弃前导空格，在下一轮处理
                            spaces = 0;
                            continue;
                        }
                        Some(_) => {
                            // 空格后跟了非符号字符（如字母、数字）
                            let err =
                                StardustError::new(ErrorKind::NonSymbolicCharacter, Some(span));
                            self.advance();
                            return Some(Err(err));
                        }
                        None => {
                            // 文件末尾只有空格，不产生 token，结束
                            return None;
                        }
                    }
                }
                Some(&ch) if is_symbol(ch) => {
                    // 无前导空格的符号 token
                    let token_type = char_to_token_type(ch).unwrap();
                    let byte_pos_of_symbol = self.byte_pos;
                    self.advance();
                    return Some(Ok(Token {
                        spaces: 0,
                        token_type,
                        line: start_line,
                        column: start_col,
                        byte_pos: byte_pos_of_symbol,
                    }));
                }
                Some(&ch) if is_anno(ch) => {
                    self.advance(); // 消耗第一个 /
                    let span = SourceSpan { line: start_line, column: start_col };
                    if let Err(e) = self.skip_comment(&span) {
                        return Some(Err(e));
                    }
                }
                Some(&ch) if ch.is_whitespace() => {
                    // 其他空白字符（\t \n \r）直接忽略
                    self.advance();
                    continue;
                }
                Some(&ch) if !is_symbol(ch) => {
                    let err = StardustError::new(ErrorKind::InvalidCharacter { ch }, Some(span));
                    self.advance();
                    return Some(Err(err));
                }
                None => return None, // EOF
                _ => {}
            }
        }
    }
}

fn is_symbol(ch: char) -> bool {
    matches!(
        ch,
        '+' | '*' | '`' | '\'' | ':' | ';' | '.' | ',' | '-' | '=' | '<' | '>' | '&' | '~' | '#'
    )
}

fn is_anno(ch: char) -> bool {
    matches!(ch, '/')
}

fn char_to_token_type(ch: char) -> Option<TokenType> {
    match ch {
        '+' => Some(TokenType::Plus),
        '*' => Some(TokenType::Star),
        '`' => Some(TokenType::Backtick),
        '\'' => Some(TokenType::Quote),
        ':' => Some(TokenType::Colon),
        ';' => Some(TokenType::Semicolon),
        '.' => Some(TokenType::Dot),
        ',' => Some(TokenType::Comma),
        '-' => Some(TokenType::Hyphen),
        '=' => Some(TokenType::Equals),
        '<' => Some(TokenType::AngleLeft),
        '>' => Some(TokenType::AngleRight),
        '&' => Some(TokenType::Ampersand),
        '~' => Some(TokenType::Tilde),
        '#' => Some(TokenType::Hash),
        '/' => Some(TokenType::Annotation),
        _ => None,
    }
}

/// 将源代码字符串解析为 Token 序列

pub fn tokenize(source: &str) -> Result<Vec<Token>, StardustError> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    while let Some(result) = lexer.next_token() {
        tokens.push(result?);
    }
    Ok(tokens)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── 辅助函数 ──────────────────────────────────────────────

    /// 创建一个 Token，简化参数（仅用于测试断言）
    fn tok(spaces: usize, tt: TokenType, line: usize, col: usize, bp: usize) -> Token {
        Token {
            spaces,
            token_type: tt,
            line,
            column: col,
            byte_pos: bp,
        }
    }

    /// 从 &str 收集所有 token，忽略错误（用于正常路径测试）
    fn collect_all(source: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        while let Some(result) = lexer.next_token() {
            tokens.push(result.unwrap());
        }
        tokens
    }

    /// 收集到第一个错误为止的 token 和错误
    fn collect_until_err(source: &str) -> (Vec<Token>, Option<StardustError>) {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        loop {
            match lexer.next_token() {
                Some(Ok(t)) => tokens.push(t),
                Some(Err(e)) => return (tokens, Some(e)),
                None => return (tokens, None),
            }
        }
    }

    // ════════════════════════════════════════════════════════════
    // 1. 单个符号识别（无前导空格）
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_single_symbol_no_spaces_plus() {
        let tokens = collect_all("+");
        assert_eq!(tokens, vec![tok(0, TokenType::Plus, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_star() {
        let tokens = collect_all("*");
        assert_eq!(tokens, vec![tok(0, TokenType::Star, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_backtick() {
        let tokens = collect_all("`");
        assert_eq!(tokens, vec![tok(0, TokenType::Backtick, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_quote() {
        let tokens = collect_all("'");
        assert_eq!(tokens, vec![tok(0, TokenType::Quote, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_colon() {
        let tokens = collect_all(":");
        assert_eq!(tokens, vec![tok(0, TokenType::Colon, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_semicolon() {
        let tokens = collect_all(";");
        assert_eq!(tokens, vec![tok(0, TokenType::Semicolon, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_dot() {
        let tokens = collect_all(".");
        assert_eq!(tokens, vec![tok(0, TokenType::Dot, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_comma() {
        let tokens = collect_all(",");
        assert_eq!(tokens, vec![tok(0, TokenType::Comma, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_tilde() {
        let tokens = collect_all("~");
        assert_eq!(tokens, vec![tok(0, TokenType::Tilde, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_hash() {
        let tokens = collect_all("#");
        assert_eq!(tokens, vec![tok(0, TokenType::Hash, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_hyphen() {
        let tokens = collect_all("-");
        assert_eq!(tokens, vec![tok(0, TokenType::Hyphen, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_equals() {
        let tokens = collect_all("=");
        assert_eq!(tokens, vec![tok(0, TokenType::Equals, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_angle_left() {
        let tokens = collect_all("<");
        assert_eq!(tokens, vec![tok(0, TokenType::AngleLeft, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_angle_right() {
        let tokens = collect_all(">");
        assert_eq!(tokens, vec![tok(0, TokenType::AngleRight, 1, 1, 0)]);
    }

    #[test]
    fn test_single_symbol_no_spaces_ampersand() {
        let tokens = collect_all("&");
        assert_eq!(tokens, vec![tok(0, TokenType::Ampersand, 1, 1, 0)]);
    }

    // ════════════════════════════════════════════════════════════
    // 2. 前导空格 + 符号
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_one_space_plus() {
        let tokens = collect_all(" +");
        assert_eq!(tokens, vec![tok(1, TokenType::Plus, 1, 1, 1)]);
    }

    #[test]
    fn test_three_spaces_star() {
        let tokens = collect_all("   *");
        assert_eq!(tokens, vec![tok(3, TokenType::Star, 1, 1, 3)]);
    }

    #[test]
    fn test_five_spaces_plus_push_0() {
        let tokens = collect_all("     +");
        assert_eq!(tokens, vec![tok(5, TokenType::Plus, 1, 1, 5)]);
    }

    #[test]
    fn test_ten_spaces_plus_push_5() {
        let tokens = collect_all("          +");
        assert_eq!(tokens, vec![tok(10, TokenType::Plus, 1, 1, 10)]);
    }

    #[test]
    fn test_many_spaces_backtick() {
        let tokens = collect_all("        `");
        assert_eq!(tokens, vec![tok(8, TokenType::Backtick, 1, 1, 8)]);
    }

    #[test]
    fn test_many_spaces_quote() {
        let tokens = collect_all("   '");
        assert_eq!(tokens, vec![tok(3, TokenType::Quote, 1, 1, 3)]);
    }

    #[test]
    fn test_spaces_colon() {
        let tokens = collect_all("  :");
        assert_eq!(tokens, vec![tok(2, TokenType::Colon, 1, 1, 2)]);
    }

    #[test]
    fn test_spaces_semicolon() {
        let tokens = collect_all("    ;");
        assert_eq!(tokens, vec![tok(4, TokenType::Semicolon, 1, 1, 4)]);
    }

    #[test]
    fn test_spaces_dot() {
        let tokens = collect_all(" .");
        assert_eq!(tokens, vec![tok(1, TokenType::Dot, 1, 1, 1)]);
    }

    #[test]
    fn test_spaces_comma() {
        let tokens = collect_all(" ,");
        assert_eq!(tokens, vec![tok(1, TokenType::Comma, 1, 1, 1)]);
    }

    #[test]
    fn test_spaces_tilde() {
        let tokens = collect_all("  ~");
        assert_eq!(tokens, vec![tok(2, TokenType::Tilde, 1, 1, 2)]);
    }

    // ════════════════════════════════════════════════════════════
    // 3. 行号和列号跟踪
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_line_column_single_line_multiple_tokens() {
        // "  +* ," — 三个 token，都在第 1 行
        let tokens = collect_all("  +* ,");
        assert_eq!(tokens.len(), 3);
        // (2空格 +): spaces=2, col=1
        assert_eq!(tokens[0], tok(2, TokenType::Plus, 1, 1, 2));
        // (*): 无空格, col=4（前一个 "  +" 占 3 个字符）
        assert_eq!(tokens[1], tok(0, TokenType::Star, 1, 4, 3));
        // (1空格 ,): spaces=1, col=5
        assert_eq!(tokens[2], tok(1, TokenType::Comma, 1, 5, 5));
    }

    #[test]
    fn test_line_column_multi_line() {
        let source = "+\n +\n  +\n";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 3);
        // 第1行: +
        assert_eq!(tokens[0], tok(0, TokenType::Plus, 1, 1, 0));
        // 第2行:  +
        assert_eq!(tokens[1], tok(1, TokenType::Plus, 2, 1, 3));
        // 第3行:   +
        assert_eq!(tokens[2], tok(2, TokenType::Plus, 3, 1, 7));
    }

    #[test]
    fn test_line_column_after_newline_with_whitespace() {
        // 第1行有 token，第2行跳过空行（只有 \n），第3行有 token
        let source = "*\n\n +";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], tok(0, TokenType::Star, 1, 1, 0));
        assert_eq!(tokens[1], tok(1, TokenType::Plus, 3, 1, 4));
    }

    #[test]
    fn test_column_resets_after_newline() {
        let source = "  +\n  *";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].column, 1);
        assert_eq!(tokens[1].column, 1); // 空格开始于行首
    }

    // ════════════════════════════════════════════════════════════
    // 4. 空白字符处理（换行、制表符）
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_tab_ignored_between_tokens() {
        let tokens = collect_all("+\t+");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[1].token_type, TokenType::Plus);
    }

    #[test]
    fn test_carriage_return_ignored() {
        let tokens = collect_all("+\r+");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[1].token_type, TokenType::Plus);
    }

    #[test]
    fn test_mixed_whitespace_between_tokens() {
        let tokens = collect_all("+\t\r\n +");
        assert_eq!(tokens.len(), 2);
        // 第一个 token: 第1行 +
        assert_eq!(tokens[0], tok(0, TokenType::Plus, 1, 1, 0));
        // 第二个 token: 第2行  +
        assert_eq!(tokens[1], tok(1, TokenType::Plus, 2, 1, 5));
    }

    #[test]
    fn test_newline_does_not_break_spaces() {
        // 换行不会跨行累计空格
        let source = "+\n +";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].spaces, 0);
        assert_eq!(tokens[1].spaces, 1);
    }

    #[test]
    fn test_multiple_newlines_ignored() {
        let source = "+\n\n\n\n*";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[1].token_type, TokenType::Star);
        assert_eq!(tokens[1].line, 5); // * 实际在第5行（跳过3个空行）
    }

    // ════════════════════════════════════════════════════════════
    // 5. 注释处理
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_comment_whole_line_no_token() {
        // 一整行都是注释，不产生 token
        let tokens = collect_all("// this is a comment\n");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_comment_whole_line_no_newline_at_end() {
        // 注释末尾没有换行符（EOF）
        let tokens = collect_all("// comment at eof");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_comment_after_instruction() {
        // 指令后面跟注释
        let tokens = collect_all("+ // push value\n");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[0].spaces, 0);
    }

    #[test]
    fn test_comment_after_instruction_with_spaces() {
        // 指令 + 空格 + 注释
        let tokens = collect_all("   +  // rotate comment\n");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], tok(3, TokenType::Plus, 1, 1, 3));
    }

    #[test]
    fn test_comment_then_next_line_token() {
        let source = "// comment line\n+\n";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], tok(0, TokenType::Plus, 2, 1, 16));
    }

    #[test]
    fn test_comment_containing_symbols() {
        // 注释内的符号字符不产生 token
        let tokens = collect_all("// + * ` ' : ; . , ~\n");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_multiple_comments_and_tokens() {
        let source = "+\n// middle comment\n*\n// another comment\n,\n";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[1].token_type, TokenType::Star);
        assert_eq!(tokens[2].token_type, TokenType::Comma);
    }

    #[test]
    fn test_comment_with_spaces_prefix() {
        // 空格后的 // 被识别为注释（不产生 token）
        let source = "   // indented comment\n+";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[0].line, 2);
    }

    // ════════════════════════════════════════════════════════════
    // 6. 注释错误：单个 / 而非 //
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_single_slash_error_no_spaces() {
        let (tokens, err) = collect_until_err("/");
        assert!(tokens.is_empty());
        assert!(err.is_some());
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidAnnotation);
    }

    #[test]
    fn test_single_slash_error_with_spaces() {
        let (tokens, err) = collect_until_err("  /");
        assert!(tokens.is_empty());
        assert!(err.is_some());
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidAnnotation);
        // 错误的 span 应该指向空格开始的位置
        assert_eq!(e.span, Some(SourceSpan { line: 1, column: 1 }));
    }

    #[test]
    fn test_slash_followed_by_non_slash() {
        // / 后面跟的不是 / → InvalidAnnotation
        let (tokens, err) = collect_until_err("/a");
        assert!(tokens.is_empty());
        assert!(err.is_some());
        assert_eq!(err.unwrap().kind, ErrorKind::InvalidAnnotation);
    }

    #[test]
    fn test_slash_followed_by_space() {
        // / 后面是空格 → InvalidAnnotation
        let (tokens, err) = collect_until_err("/ comment");
        assert!(tokens.is_empty());
        assert!(err.is_some());
        assert_eq!(err.unwrap().kind, ErrorKind::InvalidAnnotation);
    }

    // ════════════════════════════════════════════════════════════
    // 7. 无效字符错误
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_invalid_character_letter() {
        let (tokens, err) = collect_until_err("a");
        assert!(tokens.is_empty());
        assert!(err.is_some());
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidCharacter { ch: 'a' });
    }

    #[test]
    fn test_invalid_character_digit() {
        let (tokens, err) = collect_until_err("5");
        assert!(tokens.is_empty());
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidCharacter { ch: '5' });
    }

    #[test]
    fn test_invalid_character_punctuation() {
        let (tokens, err) = collect_until_err("!");
        assert!(tokens.is_empty());
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidCharacter { ch: '!' });
    }

    #[test]
    fn test_invalid_character_at_sign() {
        let (tokens, err) = collect_until_err("@");
        assert!(tokens.is_empty());
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidCharacter { ch: '@' });
    }

    #[test]
    fn test_non_symbolic_character_spaces_then_letter() {
        // 空格后跟非符号字符 → NonSymbolicCharacter
        let (tokens, err) = collect_until_err("   x");
        assert!(tokens.is_empty());
        assert!(err.is_some());
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::NonSymbolicCharacter);
        assert_eq!(e.span, Some(SourceSpan { line: 1, column: 1 }));
    }

    #[test]
    fn test_non_symbolic_character_spaces_then_digit() {
        let (tokens, err) = collect_until_err(" 9");
        assert!(tokens.is_empty());
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::NonSymbolicCharacter);
    }

    #[test]
    fn test_valid_token_before_invalid_character() {
        // 前面的合法 token 应正常解析
        let (tokens, err) = collect_until_err("+x");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidCharacter { ch: 'x' });
    }

    #[test]
    fn test_line_column_in_error() {
        let (tokens, err) = collect_until_err("*\n@");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Star);
        let e = err.unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidCharacter { ch: '@' });
        // 错误应定位在第2行第1列
        assert_eq!(e.span, Some(SourceSpan { line: 2, column: 1 }));
    }

    // ════════════════════════════════════════════════════════════
    // 8. EOF 处理
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_empty_input_no_tokens() {
        let tokens = collect_all("");
        assert!(tokens.is_empty());
    }

    // #[test]
    // fn test_only_whitespace_no_tokens() {
    //     let tokens = collect_all("  \t \n \r \n  ");
    //     assert!(tokens.is_empty());
    // }

    #[test]
    fn test_only_spaces_no_symbol_eof() {
        // 只有空格，没有符号就 EOF → 不产生 token
        let tokens = collect_all("   ");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_spaces_at_end_of_file() {
        // 最后一个 token 后的空格不产生新 token
        let tokens = collect_all("+   ");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
    }

    #[test]
    fn test_newline_at_end_of_file() {
        let tokens = collect_all("+\n");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
    }

    // ════════════════════════════════════════════════════════════
    // 9. byte_pos 跟踪
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_byte_pos_simple() {
        let tokens = collect_all("+");
        // '+' 在字节位置 0
        assert_eq!(tokens[0].byte_pos, 0);
    }

    #[test]
    fn test_byte_pos_with_spaces() {
        let tokens = collect_all("   +");
        // 3 个空格后 '+' 在字节位置 3
        assert_eq!(tokens[0].byte_pos, 3);
    }

    #[test]
    fn test_byte_pos_multiple_tokens() {
        // 字节: 0='+', 1=' ', 2='*', 3='\n', 4=' ', 5=','
        let tokens = collect_all("+ *\n ,");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].byte_pos, 0); // '+'
        assert_eq!(tokens[1].byte_pos, 2); // '*'
        assert_eq!(tokens[2].byte_pos, 5); // ','
    }

    #[test]
    fn test_byte_pos_with_newlines() {
        // 字节: 0='+', 1='\n', 2='\n', 3='*'
        let tokens = collect_all("+\n\n*");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].byte_pos, 0);
        assert_eq!(tokens[1].byte_pos, 3);
    }

    #[test]
    fn test_byte_pos_with_multibyte_not_applicable() {
        // Stardust 源代码仅使用 ASCII 字符（空格 + 符号），所以 multibyte 不存在
        // 但测试确保 byte_pos 与列的概念一致
        let source = "          +"; // 10 spaces + '+'
        let tokens = collect_all(source);
        assert_eq!(tokens[0].byte_pos, 10);
        assert_eq!(tokens[0].spaces, 10);
    }

    // ════════════════════════════════════════════════════════════
    // 10. tokenize() 包装函数
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_tokenize_happy_path() {
        let result = tokenize("+ * ,");
        assert!(result.is_ok());
        let tokens = result.unwrap();
        assert_eq!(tokens.len(), 3);
    }

    #[test]
    fn test_tokenize_empty_string() {
        let result = tokenize("");
        assert!(result.is_ok());
        let tokens = result.unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_error_stops() {
        // tokenize 在第一个错误处停止
        let result = tokenize("+ x");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::NonSymbolicCharacter);
    }

    #[test]
    fn test_tokenize_comment_only() {
        let result = tokenize("// nothing here\n");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // ════════════════════════════════════════════════════════════
    // 11. 综合场景
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_realistic_hello_world_chunk() {
        // 取自 hello_world.sd 的一部分
        let source = "            +               +  *       +* +,";
        let tokens = collect_all(source);
        // 不 panic，能正常解析
        assert!(tokens.len() > 0);
        // 验证第一个 token
        assert_eq!(tokens[0], tok(12, TokenType::Plus, 1, 1, 12));
    }

    #[test]
    fn test_function_declaration_pattern() {
        // (3) :  表示函数声明开始，spaces=3 的函数名
        let tokens = collect_all("   :");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], tok(3, TokenType::Colon, 1, 1, 3));
    }

    #[test]
    fn test_function_call_pattern() {
        // (3) : (2) ;  → Colon(spaces=3) + Semicolon(spaces=2)
        let tokens = collect_all("   :  ;");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], tok(3, TokenType::Colon, 1, 1, 3));
        assert_eq!(tokens[1], tok(2, TokenType::Semicolon, 1, 5, 6));
    }

    #[test]
    fn test_mark_and_jump() {
        // (1) ` 声明标志1
        // (1) ' 条件跳转到标志1
        let source = " `\n '";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], tok(1, TokenType::Backtick, 1, 1, 1));
        assert_eq!(tokens[1], tok(1, TokenType::Quote, 2, 1, 4));
    }

    #[test]
    fn test_unconditional_jump() {
        let tokens = collect_all("   ~");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], tok(3, TokenType::Tilde, 1, 1, 3));
    }

    #[test]
    fn test_push_char_out_sequence() {
        // Push(72) + CharOut  →  (77空格)+  +  ,
        let source =
            "                                                                             +,";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].spaces, 77);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[1].spaces, 0);
        assert_eq!(tokens[1].token_type, TokenType::Comma);
    }

    #[test]
    fn test_consecutive_symbols_no_spaces() {
        // 连续无空格符号
        let tokens = collect_all("+*");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], tok(0, TokenType::Plus, 1, 1, 0));
        assert_eq!(tokens[1], tok(0, TokenType::Star, 1, 2, 1));
    }

    #[test]
    fn test_mixed_instructions_with_comments() {
        let source = "\
            +               +  *       +* +,  // Hello\n\
         +            +  *      +** +,            +* +, +,  // World\n";
        let tokens = collect_all(source);
        // 确保解析不会 panic，所有指令都被正确提取
        assert!(tokens.len() > 0);
        // 注释后的内容（包括下一行开头的空格）能正确识别
        let last_line_tokens: Vec<_> = tokens.iter().filter(|t| t.line == 2).collect();
        assert!(!last_line_tokens.is_empty());
    }

    #[test]
    fn test_zero_spaces_is_valid_mark_name() {
        // (0) `  — 标志名称为 0
        let tokens = collect_all("`");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], tok(0, TokenType::Backtick, 1, 1, 0));
    }

    #[test]
    fn test_zero_spaces_colon_is_valid_func_name() {
        // (0) :  — 函数名称为 0
        let tokens = collect_all(":");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], tok(0, TokenType::Colon, 1, 1, 0));
    }

    #[test]
    fn test_spaces_followed_by_newline_ignored() {
        // 前导空格后跟换行符 → 不产生 token，空格被忽略
        let tokens = collect_all("   \n+");
        assert_eq!(tokens.len(), 1);
        // lexer 的 line/col 记录函数入口位置，byte_pos 记录符号位置
        assert_eq!(tokens[0].spaces, 0);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[0].byte_pos, 4);
    }

    #[test]
    fn test_spaces_followed_by_tab_ignored() {
        // 前导空格后跟制表符 → 不产生 token
        let tokens = collect_all("  \t+");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].spaces, 0);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
    }

    #[test]
    fn test_multiple_lines_with_only_spaces() {
        // 多行只有空格，不产生 token
        let source = "   \n  \n +\n";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].spaces, 1);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
    }

    #[test]
    fn test_very_large_space_count() {
        // 测试大空格数 (模拟 Push 大值)
        let spaces = 200;
        let source = format!("{}+", " ".repeat(spaces));
        let tokens = collect_all(&source);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].spaces, spaces);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
    }

    // ════════════════════════════════════════════════════════════
    // 12. 辅助函数测试
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_is_symbol_all_valid() {
        let symbols = "+*`':;.,-=<>&~#";
        for ch in symbols.chars() {
            assert!(is_symbol(ch), "Expected '{}' to be a symbol", ch);
        }
    }

    #[test]
    fn test_is_symbol_invalid() {
        assert!(!is_symbol('a'));
        assert!(!is_symbol('1'));
        assert!(!is_symbol(' '));
        assert!(!is_symbol('/'));
        assert!(!is_symbol('\\'));
        assert!(!is_symbol('!'));
        assert!(!is_symbol('\n'));
    }

    #[test]
    fn test_is_anno_valid() {
        assert!(is_anno('/'));
    }

    #[test]
    fn test_is_anno_invalid() {
        assert!(!is_anno('\\'));
        assert!(!is_anno('*'));
        assert!(!is_anno(' '));
    }

    #[test]
    fn test_char_to_token_type_all_mappings() {
        let mappings = vec![
            ('+', TokenType::Plus),
            ('*', TokenType::Star),
            ('`', TokenType::Backtick),
            ('\'', TokenType::Quote),
            (':', TokenType::Colon),
            (';', TokenType::Semicolon),
            ('.', TokenType::Dot),
            (',', TokenType::Comma),
            ('-', TokenType::Hyphen),
            ('=', TokenType::Equals),
            ('<', TokenType::AngleLeft),
            ('>', TokenType::AngleRight),
            ('&', TokenType::Ampersand),
            ('~', TokenType::Tilde),
            ('#', TokenType::Hash),
            ('/', TokenType::Annotation),
        ];
        for (ch, expected) in mappings {
            assert_eq!(
                char_to_token_type(ch),
                Some(expected),
                "char_to_token_type('{}') failed",
                ch
            );
        }
    }

    #[test]
    fn test_char_to_token_type_invalid() {
        assert_eq!(char_to_token_type('a'), None);
        assert_eq!(char_to_token_type(' '), None);
        assert_eq!(char_to_token_type('1'), None);
        assert_eq!(char_to_token_type('!'), None);
    }

    // ════════════════════════════════════════════════════════════
    // 13. 边界情况
    // ════════════════════════════════════════════════════════════

    #[test]
    fn test_token_at_eof_without_newline() {
        // 文件末尾没有换行符
        let source = "+";
        let mut lexer = Lexer::new(source);
        let t1 = lexer.next_token();
        assert!(t1.is_some() && t1.unwrap().is_ok());
        let t2 = lexer.next_token();
        assert!(t2.is_none()); // EOF
    }

    #[test]
    fn test_token_after_comment_no_newline() {
        // 注释后没有换行直接EOF → 没有后续 token
        let tokens = collect_all("// comment");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_comment_between_tokens_same_line() {
        // 同一行的注释不会影响前面的 token，后面的内容属于注释
        let tokens = collect_all("+ // middle *\n,");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[1].token_type, TokenType::Comma);
    }

    #[test]
    fn test_lexer_new_initial_state() {
        let mut lexer = Lexer::new("+");
        let result = lexer.next_token();
        let token = result.unwrap().unwrap();
        assert_eq!(token.line, 1);
        assert_eq!(token.column, 1);
    }

    #[test]
    fn test_spaces_reset_after_each_token() {
        // 每个 token 的前导空格独立计算，不会累加
        let source = "  +  +";
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].spaces, 2);
        assert_eq!(tokens[1].spaces, 2);
    }

    #[test]
    fn test_all_sixteen_symbol_types_no_spaces() {
        // 验证所有 16 种符号类型在无空格情况下都能正确解析
        let source = "+*`':;.,-=<>&~#";
        let expected_types = vec![
            TokenType::Plus,
            TokenType::Star,
            TokenType::Backtick,
            TokenType::Quote,
            TokenType::Colon,
            TokenType::Semicolon,
            TokenType::Dot,
            TokenType::Comma,
            TokenType::Hyphen,
            TokenType::Equals,
            TokenType::AngleLeft,
            TokenType::AngleRight,
            TokenType::Ampersand,
            TokenType::Tilde,
            TokenType::Hash,
        ];
        let tokens = collect_all(source);
        assert_eq!(tokens.len(), expected_types.len());
        for (i, expected) in expected_types.iter().enumerate() {
            assert_eq!(tokens[i].token_type, *expected, "Token {} mismatch", i);
            assert_eq!(tokens[i].spaces, 0, "Token {} spaces mismatch", i);
        }
    }
}
