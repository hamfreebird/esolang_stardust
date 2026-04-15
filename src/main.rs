pub mod stardust;

#[derive(Debug, PartialEq, Clone)]
pub enum TokenType {
    Plus,       // '+'
    Star,       // '*'
    Backtick,   // '`'
    Quote,      // '\''
    Colon,      // ':'
    Semicolon,  // ';'
    Dot,        // '.'
    Comma,      // ','
}

#[derive(Debug, PartialEq, Clone)]
pub struct Token {
    pub spaces: usize,          // 前导空格数量
    pub token_type: TokenType,
    pub line: usize,            // 可选，用于错误报告
    pub column: usize,
}

fn main() {}