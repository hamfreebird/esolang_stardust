use std::collections::HashMap;
use serde::Serialize;

pub mod lexer;
pub mod parser;
pub mod vm;
pub mod utils;
pub mod error;

/// 最大调用深度 — 防止无限递归
pub const MAX_CALL_DEPTH: usize = 256;

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
    UnconditionalJump { name: usize, meta: InstrMeta }, // (n) ~
    // ── 比较运算（符号 =）────────────────────────────────
    Eq(InstrMeta),                          // 0 =  相等
    Ne(InstrMeta),                          // 1 =  不等
    Lt(InstrMeta),                          // 2 =  小于
    Gt(InstrMeta),                          // 3 =  大于
    Le(InstrMeta),                          // 4 =  小于等于
    Ge(InstrMeta),                          // 5 =  大于等于
    // ── 逻辑运算（符号 &）────────────────────────────────
    And(InstrMeta),                         // 0 &  逻辑与
    Or(InstrMeta),                          // 1 &  逻辑或
    Not(InstrMeta),                         // 2 &  逻辑非（仅弹出1个值）
    Xor(InstrMeta),                         // 3 &  逻辑异或
    // ── 堆操作（符号 -）──────────────────────────────────
    Store(InstrMeta),                       // 0 -  弹出 addr val, heap[addr]=val
    Load(InstrMeta),                        // 1 -  弹出 addr, 压入 heap[addr]
    // ── 栈扩展（符号 < >）───────────────────────────────
    ShiftL(InstrMeta),                      // 0 <  循环左移 [a1..aN]→[a2..aN,a1]
    Depth(InstrMeta),                       // 1 <  压入栈深度
    Pick(InstrMeta),                        // 2 <  弹出n, 复制aN-n压入
    ShiftR(InstrMeta),                      // 0 >  循环右移 [a1..aN]→[aN,a1..aN-1]
    DropN(InstrMeta),                       // 1 >  弹出n, 丢弃栈顶n个元素
    // ── 调试（符号 #）────────────────────────────────────
    DumpStack(InstrMeta),                   // 0 #  输出栈内容到stderr
    DumpState(InstrMeta),                   // 1 #  输出解释器状态到stderr
    Breakpoint(InstrMeta),                  // 2 #  调试断点（仅--debug模式生效）
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

/// 调用栈帧 — 每次函数调用创建一个新帧
///
/// 主程序作为第 0 帧，函数调用时压入新帧，返回时弹出并合并栈内容。
#[derive(Debug, Clone)]
pub struct CallFrame {
    pub instructions: Vec<Instruction>,
    pub stack: Vec<i64>,
    pub pc: usize,
    pub marks: HashMap<usize, usize>,
    /// 返回后主调帧应继续执行的 PC（Call 指令的下一条）
    pub ret_pc: usize,
}

impl CallFrame {
    pub fn new(instructions: Vec<Instruction>, marks: HashMap<usize, usize>) -> Self {
        CallFrame {
            instructions,
            stack: Vec::new(),
            pc: 0,
            marks,
            ret_pc: 0,
        }
    }
}

pub struct VM {
    /// 函数库（所有帧共享）
    functions: HashMap<usize, Vec<Instruction>>,
    /// 调用栈：frames[0] 为主程序，后续为函数调用帧
    frames: Vec<CallFrame>,
    /// 堆存储（所有帧共享）：地址 → 值
    heap: HashMap<i64, i64>,
    /// 是否结束运行
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
    InvalidSpacesForEquals { spaces: usize },
    InvalidSpacesForAmpersand { spaces: usize },
    InvalidSpacesForHyphen { spaces: usize },
    InvalidSpacesForAngleLeft { spaces: usize },
    InvalidSpacesForAngleRight { spaces: usize },
    InvalidSpacesForHash { spaces: usize },
    InvalidInstructionContext,
    NotEnoughArguments { func: usize, expected: usize, actual: usize },
    IntegerOverflow,
    CallDepthExceeded,
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
