use std::iter::Peekable;
use std::str::Chars;
use crate::stardust::{ErrorKind, SourceSpan, StardustError, Token, TokenType};

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

    /// 解析一个 token，忽略非指令空白
    pub fn next_token(&mut self) -> Option<Result<Token, StardustError>> {
        let mut spaces = 0;
        let start_line = self.line;
        let start_col = self.column;
        let span = SourceSpan { line: start_line, column: start_col };

        loop {
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
                                byte_pos: byte_pos_of_symbol
                            }));
                        }
                        Some(_) => {
                            // 空格后跟了非符号字符（如字母、数字）
                            let err = StardustError::new(
                                ErrorKind::NonSymbolicCharacter,
                                Some(span),
                            );
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
                        byte_pos: byte_pos_of_symbol
                    }));
                }

                Some(&ch) if ch.is_whitespace() => {
                    // 其他空白字符（\t \n \r）直接忽略
                    self.advance();
                    continue;
                }

                Some(&ch) if !is_symbol(ch) => {
                    let err = StardustError::new(
                        ErrorKind::InvalidCharacter { ch },
                        Some(span),
                    );
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
    matches!(ch, '+' | '*' | '`' | '\'' | ':' | ';' | '.' | ',')
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
