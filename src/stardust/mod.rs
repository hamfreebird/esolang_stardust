use std::collections::HashMap;
use serde::Serialize;

pub mod lexer;
pub mod parser;
pub mod vm;
pub mod utils;
pub mod error;

#[derive(Debug, PartialEq, Clone, Serialize)]
pub enum TokenType {
    Plus,       // '+'
    Star,       // '*'
    Backtick,   // '`'
    Quote,      // '\''
    Colon,      // ':'
    Semicolon,  // ';'
    Dot,        // '.'
    Comma,      // ','
    Hyphen,     // '-'
    Equals,     // '='
    AngleLeft,  // '<'
    AngleRight, // '>'
    Ampersand,  // '&'
    Tilde,      // '~'
    Hash,       // '#"
    Annotation, // '//'

}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct Token {
    pub spaces: usize,          // 前导空格数量
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
    pub byte_pos: usize,}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Instruction {
    Push(i64, InstrMeta),                  // (n >= 5) +  -> n-5
    Dup(InstrMeta),                        // 1 +
    Swap(InstrMeta),                       // 2 +
    Rotate(InstrMeta),                     // 3 +
    Pop(InstrMeta),                        // 4 +
    Add(InstrMeta),                        // 0 *
    Sub(InstrMeta),                        // 1 *
    Mul(InstrMeta),                        // 2 *
    Div(InstrMeta),                        // 3 *
    Mod(InstrMeta),                        // 4 *
    Reverse(InstrMeta),                    // 5 *
    NumOut(InstrMeta),                     // 0 .
    NumIn(InstrMeta),                      // 1 .
    CharOut(InstrMeta),                    // 0 ,
    CharIn(InstrMeta),                     // 1 ,
    Mark { name: usize, meta: InstrMeta },       // (n) `
    Jump { name: usize, meta: InstrMeta },       // (n) '
    Call { name: usize, argc: usize, meta: InstrMeta }, // (n1) : (n2) ;
    // 危险操作
    UnconditionalJump { name: usize, meta: InstrMeta } // (n) ~
}

#[derive(Debug, Serialize)]
pub struct ParseResult {
    pub main_instructions: Vec<Instruction>,
    pub main_marks: HashMap<usize, usize>,
    pub functions: HashMap<usize, Vec<Instruction>>,
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    // 解析过程暂存
    instructions: Vec<Instruction>,
    marks: HashMap<usize, usize>,
    functions: HashMap<usize, Vec<Instruction>>,
}

pub struct VM {
    // 主程序指令（只读）
    main_instructions: Vec<Instruction>,
    // 主程序标志映射
    main_marks: HashMap<usize, usize>,
    // 函数库
    functions: HashMap<usize, Vec<Instruction>>,

    // 主栈
    main_stack: Vec<i64>,
    // 程序计数器 (对于主程序)
    pc: usize,
    // 是否结束运行
    halted: bool,
}

/// 源代码位置信息
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceSpan {
    pub line: usize,
    pub column: usize,
}

/// 指令元数据 — 附加到每条指令上的源码位置和调试信息
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstrMeta {
    pub span: SourceSpan,
}

impl InstrMeta {
    pub fn new(line: usize, column: usize) -> Self {
        InstrMeta {
            span: SourceSpan { line, column },
        }
    }
}

impl Default for InstrMeta {
    fn default() -> Self {
        InstrMeta {
            span: SourceSpan { line: 1, column: 1 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ErrorKind {
    InvalidCharacter { ch: char },
    NonSymbolicCharacter,
    TrailingSpaces,
    UnexpectedToken { expected: String, found: String },
    DuplicateMark { name: usize },
    DuplicateFunction { name: usize },
    UndefinedMark { name: usize },
    UndefinedFunction { name: usize },
    CallInsideFunction,
    UnclosedFunction { name: usize },
    StackUnderflow,
    DivisionByZero,
    ModuloByZero,
    InvalidAscii { value: i64 },
    InvalidIntegerInput,
    IoError { reason: String },
    UnexpectedEof,
    IncompleteFunctionCall,
    ExpectedColonInCall,
    ExpectedSemicolonAfterCall,
    InvalidSpacesForPlus,
    InvalidSpacesForStar { spaces: usize },
    InvalidSpacesForDot { spaces: usize },
    InvalidSpacesForComma { spaces: usize },
    InvalidInstructionContext,
    NotEnoughArguments { func: usize, expected: usize, actual: usize },
    // JumpWhenStackAreNotZero,
    InvalidAnnotation,
    ParseChar,
    StdIoError,
    CodePointTooLarge,
}

/// 完整的错误信息
#[derive(Debug, Clone, Serialize)]
pub struct StardustError {
    pub kind: ErrorKind,
    pub span: Option<SourceSpan>,
    pub message: String,
}

#[derive(Debug)]
pub enum StageResult {
    Source(String),
    UnwindSource(String),
    Tokens(Vec<Token>),
    Parsed(ParseResult),
    Error(String),
    None,
}
