use std::collections::HashMap;

pub mod lexer;
pub mod parser;
pub mod vm;
pub mod utils;
pub mod error;

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
    Hyphen,     // '-'
    Equals,     // '='
    AngleLeft,  // '<'
    AngleRight, // '>'
    Ampersand,  // '&'
    Tilde,      // '~'
    Hash,       // '#"
    Annotation, // '//'

}

#[derive(Debug, PartialEq, Clone)]
pub struct Token {
    pub spaces: usize,          // 前导空格数量
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
    pub byte_pos: usize,}

#[derive(Debug, Clone)]
pub enum Instruction {
    Push(i64),                  // (n >= 5) +  -> n-5
    Dup,                        // 1 +
    Swap,                       // 2 +
    Rotate,                     // 3 +
    Pop,                        // 4 +
    Add,                        // 0 *
    Sub,                        // 1 *
    Mul,                        // 2 *
    Div,                        // 3 *
    Mod,                        // 4 *
    Reverse,                    // 5 *
    NumOut,                     // 0 .
    NumIn,                      // 1 .
    CharOut,                    // 0 ,
    CharIn,                     // 1 ,
    Mark { name: usize },       // (n) `
    Jump { name: usize },       // (n) '
    Call { name: usize, argc: usize }, // (n1) : (n2) ;
    // 危险操作
    UnconditionalJump { name: usize ,} // (n) ~
}

#[derive(Debug)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpan {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
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
    JumpWhenStackAreNotZero,
    InvalidAnnotation,
    ParseChar,
    StdIoError,
    CodePointTooLarge,
}

/// 完整的错误信息
#[derive(Debug, Clone)]
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
