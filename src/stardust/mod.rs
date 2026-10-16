use crate::stardust::debugger::Debugger;
use serde::Serialize;
use std::collections::HashMap;
use std::rc::Rc;

pub mod config;
pub mod debugger;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod utils;
pub mod vm;

// 注意：调用深度上限由 `VmConfig::max_call_depth` / `CodeGenConfig::max_call_depth`
// 控制（默认 256），可经 `#[max_call_depth=N]` 注解覆盖。
// 早期这里还有一个 MAX_CALL_DEPTH 常量，但从未被读取，留着只会误导。

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
    pub spaces: usize, // 前导空格数量
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
    pub byte_pos: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Instruction {
    Push(i64, InstrMeta),                               // (n >= 5) +  -> n-5
    Dup(InstrMeta),                                     // 1 +
    Swap(InstrMeta),                                    // 2 +
    Rotate(InstrMeta),                                  // 3 +
    Pop(InstrMeta),                                     // 4 +
    Add(InstrMeta),                                     // 0 *
    Sub(InstrMeta),                                     // 1 *
    Mul(InstrMeta),                                     // 2 *
    Div(InstrMeta),                                     // 3 *
    Mod(InstrMeta),                                     // 4 *
    Reverse(InstrMeta),                                 // 5 *
    NumOut(InstrMeta),                                  // 0 .
    NumIn(InstrMeta),                                   // 1 .
    CharOut(InstrMeta),                                 // 0 ,
    CharIn(InstrMeta),                                  // 1 ,
    Mark { name: usize, meta: InstrMeta },              // (n) `
    Jump { name: usize, meta: InstrMeta },              // (n) '
    Call { name: usize, argc: usize, meta: InstrMeta }, // (n1) : (n2) ;
    // 危险操作
    UnconditionalJump { name: usize, meta: InstrMeta }, // (n) ~
    // ── 比较运算（符号 =）────────────────────────────────
    Eq(InstrMeta), // 0 =  相等
    Ne(InstrMeta), // 1 =  不等
    Lt(InstrMeta), // 2 =  小于
    Gt(InstrMeta), // 3 =  大于
    Le(InstrMeta), // 4 =  小于等于
    Ge(InstrMeta), // 5 =  大于等于
    // ── 逻辑运算（符号 &）────────────────────────────────
    And(InstrMeta), // 0 &  逻辑与
    Or(InstrMeta),  // 1 &  逻辑或
    Not(InstrMeta), // 2 &  逻辑非（仅弹出1个值）
    Xor(InstrMeta), // 3 &  逻辑异或
    // ── 堆操作（符号 -）──────────────────────────────────
    Store(InstrMeta), // 0 -  弹出 addr val, heap[addr]=val
    Load(InstrMeta),  // 1 -  弹出 addr, 压入 heap[addr]
    // ── 栈扩展（符号 < >）───────────────────────────────
    ShiftL(InstrMeta), // 0 <  循环左移 [a1..aN]→[a2..aN,a1]
    Depth(InstrMeta),  // 1 <  压入栈深度
    Pick(InstrMeta),   // 2 <  弹出n, 复制aN-n压入
    ShiftR(InstrMeta), // 0 >  循环右移 [a1..aN]→[aN,a1..aN-1]
    DropN(InstrMeta),  // 1 >  弹出n, 丢弃栈顶n个元素
    // ── 调试（符号 #）────────────────────────────────────
    DumpStack(InstrMeta),  // 0 #  输出栈内容到stderr
    DumpState(InstrMeta),  // 1 #  输出解释器状态到stderr
    Breakpoint(InstrMeta), // 2 #  调试断点（仅--debug模式生效）
}

impl Instruction {
    /// 返回指令的人类可读描述
    pub fn format(&self) -> String {
        match self {
            Instruction::Push(n, _) => format!("Push({})", n),
            Instruction::Dup(_) => "Dup".into(),
            Instruction::Swap(_) => "Swap".into(),
            Instruction::Rotate(_) => "Rotate".into(),
            Instruction::Pop(_) => "Pop".into(),
            Instruction::Add(_) => "Add".into(),
            Instruction::Sub(_) => "Sub".into(),
            Instruction::Mul(_) => "Mul".into(),
            Instruction::Div(_) => "Div".into(),
            Instruction::Mod(_) => "Mod".into(),
            Instruction::Reverse(_) => "Reverse".into(),
            Instruction::NumOut(_) => "NumOut".into(),
            Instruction::NumIn(_) => "NumIn".into(),
            Instruction::CharOut(_) => "CharOut".into(),
            Instruction::CharIn(_) => "CharIn".into(),
            Instruction::Mark { name, .. } => format!("Mark({})", name),
            Instruction::Jump { name, .. } => format!("Jump({})", name),
            Instruction::UnconditionalJump { name, .. } => format!("UncondJump({})", name),
            Instruction::Call { name, argc, .. } => format!("Call(func={}, argc={})", name, argc),
            Instruction::Eq(_) => "Eq".into(),
            Instruction::Ne(_) => "Ne".into(),
            Instruction::Lt(_) => "Lt".into(),
            Instruction::Gt(_) => "Gt".into(),
            Instruction::Le(_) => "Le".into(),
            Instruction::Ge(_) => "Ge".into(),
            Instruction::And(_) => "And".into(),
            Instruction::Or(_) => "Or".into(),
            Instruction::Not(_) => "Not".into(),
            Instruction::Xor(_) => "Xor".into(),
            Instruction::Store(_) => "Store".into(),
            Instruction::Load(_) => "Load".into(),
            Instruction::ShiftL(_) => "ShiftL".into(),
            Instruction::Depth(_) => "Depth".into(),
            Instruction::Pick(_) => "Pick".into(),
            Instruction::ShiftR(_) => "ShiftR".into(),
            Instruction::DropN(_) => "DropN".into(),
            Instruction::DumpStack(_) => "DumpStack".into(),
            Instruction::DumpState(_) => "DumpState".into(),
            Instruction::Breakpoint(_) => "Breakpoint".into(),
        }
    }
}

/// 函数定义：函数体 + 预计算的 Marks 表
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub body: Rc<Vec<Instruction>>,
    pub marks: HashMap<usize, usize>,
}

#[derive(Debug)]
pub struct ParseResult {
    pub main_instructions: Vec<Instruction>,
    pub main_marks: HashMap<usize, usize>,
    pub functions: HashMap<usize, FunctionDef>,
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    instructions: Vec<Instruction>,
    marks: HashMap<usize, usize>,
    functions: HashMap<usize, Vec<Instruction>>,
}

/// 调用栈帧 — 每次函数调用创建一个新帧
///
/// 主程序作为第 0 帧，函数调用时压入新帧，返回时弹出并合并栈内容。
/// 使用 `Rc` 共享函数体避免每次调用 clone。
#[derive(Debug, Clone)]
pub struct CallFrame {
    pub instructions: Rc<Vec<Instruction>>,
    pub stack: Vec<i64>,
    pub pc: usize,
    pub marks: HashMap<usize, usize>,
    /// 返回后主调帧应继续执行的 PC（Call 指令的下一条）
    pub ret_pc: usize,
}

impl CallFrame {
    pub fn new(instructions: Rc<Vec<Instruction>>, marks: HashMap<usize, usize>) -> Self {
        CallFrame { instructions, stack: Vec::new(), pc: 0, marks, ret_pc: 0 }
    }
}

pub struct VM {
    /// 函数库（Rc 共享函数体，避免每次调用 clone）
    pub functions: HashMap<usize, FunctionDef>,
    /// 调用栈：frames[0] 为主程序，后续为函数调用帧
    frames: Vec<CallFrame>,
    /// 堆存储（所有帧共享）：地址 → 值
    heap: HashMap<i64, i64>,
    /// 是否结束运行
    halted: bool,
    /// 调试器（None 表示非调试模式）
    pub debug: Option<Debugger>,
    /// 运行时配置
    pub config: crate::stardust::config::VmConfig,
}

impl VM {
    /// 获取当前执行帧
    pub fn current_frame(&self) -> &CallFrame {
        self.frames.last().expect("VM has no frames")
    }

    /// 当前帧索引（0 = main）
    pub fn current_frame_index(&self) -> usize {
        self.frames.len().saturating_sub(1)
    }

    /// 堆条目数
    pub fn heap_len(&self) -> usize {
        self.heap.len()
    }

    /// 堆引用
    pub fn heap_entries(&self) -> &HashMap<i64, i64> {
        &self.heap
    }

    /// 函数表条目数
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    /// 所有调用帧
    pub fn all_frames(&self) -> &[CallFrame] {
        &self.frames
    }

    /// 当前帧的栈（栈顶在后）
    pub fn current_stack(&self) -> &[i64] {
        self.frames.last().map(|f| &f.stack[..]).unwrap_or(&[])
    }

    /// 设置主帧栈（REPL 状态注入）
    pub fn set_main_stack(&mut self, stack: Vec<i64>) {
        if self.frames.is_empty() {
            self.frames.push(CallFrame::new(Rc::new(Vec::new()), HashMap::new()));
        }
        self.frames[0].stack = stack;
    }

    /// 取出主帧栈（REPL 状态保存）
    pub fn take_main_stack(&mut self) -> Vec<i64> {
        self.frames.first_mut().map(|f| std::mem::take(&mut f.stack)).unwrap_or_default()
    }

    /// 设置堆（REPL 状态注入）
    pub fn set_heap(&mut self, heap: HashMap<i64, i64>) {
        self.heap = heap;
    }

    /// 取出堆（REPL 状态保存）
    pub fn take_heap(&mut self) -> HashMap<i64, i64> {
        std::mem::take(&mut self.heap)
    }
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
        InstrMeta { span: SourceSpan { line, column } }
    }
}

impl Default for InstrMeta {
    fn default() -> Self {
        InstrMeta { span: SourceSpan { line: 1, column: 1 } }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ErrorKind {
    InvalidCharacter { ch: char },
    NonSymbolicCharacter,
    TrailingSpaces, // 保留：词法器目前不产生此错误（行尾空格被静默忽略）
    UnexpectedToken { expected: String, found: String },
    DuplicateMark { name: usize },
    DuplicateFunction { name: usize },
    UndefinedMark { name: usize },
    UndefinedFunction { name: usize },
    UnclosedFunction { name: usize },
    StackUnderflow,
    DivisionByZero,
    ModuloByZero,
    InvalidAscii { value: i64 },
    InvalidIntegerInput,
    IoError { reason: String },
    UnexpectedEof,
    IncompleteFunctionCall,
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
    StackOverflow,
    HeapOverflow,
    InvalidCodePoint { value: i64 },
    StepLimitExceeded { steps: u64 },
    TimeLimitExceeded,
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
