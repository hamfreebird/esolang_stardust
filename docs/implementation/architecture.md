# 实现架构

> **实现语言**：Rust 2024 Edition (rust-version 1.95) · **依赖**：`serde` + `serde_json` (仅 CLI/JSON 模式)

## 1. 系统概览

```
                       ┌──────────┐
                       │  .sd 源码 │
                       └────┬─────┘
                            │
                       ┌────▼─────┐
                       │ 配置提取  │  parse_config() — 移除 #[key=value] 注解
                       └────┬─────┘
                            │
                       ┌────▼─────┐
                       │ 预处理    │  simple_preprocess() — 字符替换
                       └────┬─────┘
                            │
                       ┌────▼─────┐
                       │ 词法分析  │  tokenize() → Vec<Token>
                       └────┬─────┘
                            │
                       ┌────▼─────┐
                       │ 语法分析  │  parse_program() → ParseResult
                       └────┬─────┘
                            │
              ┌─────────────┼─────────────┐
              │             │             │
         ┌────▼───┐   ┌────▼────┐  ┌─────▼────┐
         │ VM 解释 │   │ LLVM    │  │ 语法检查  │
         │ 执行    │   │ 编译    │  │ (--check) │
         └────────┘   └────────┘  └──────────┘
```

## 2. 模块结构

```
src/
├── main.rs                  # CLI 入口，8 种运行模式分发 (395 行)
├── lib.rs                   # 库入口
├── stardust/                # 核心语言实现
│   ├── mod.rs               # 核心类型定义 (359 行)
│   ├── config.rs            # VmConfig + #[key=value] 注解解析 (221 行)
│   ├── lexer.rs             # 词法分析器 (1109 行)
│   ├── parser.rs            # 语法分析器 (890 行)
│   ├── vm.rs                # 栈式虚拟机 + VmIO trait (614 行)
│   ├── error.rs             # 错误类型系统 (114 行)
│   ├── utils.rs             # 工具函数 (405 行)
│   └── debugger.rs          # 交互式调试器 (322 行)
├── codegen/                 # LLVM 代码生成
│   ├── mod.rs               # 公开 API + 工具链检测 (228 行)
│   ├── compiler.rs          # Stardust → LLVM IR 编译器 (1950 行)
│   ├── optimizer.rs         # IR 优化器 (745 行)
│   └── intrinsics.rs        # 运行时内置函数 IR 文本 (128 行)
├── repl/                    # 交互式 REPL
│   ├── mod.rs               # REPL 主循环 + 元命令 (426 行)
│   ├── parser.rs            # 简写语法解析器 (317 行)
│   ├── executor.rs          # 执行上下文 (210 行)
│   └── display.rs           # 状态可视化 (299 行)
└── extension/               # 文本 → Stardust 转换
    ├── mod.rs               # 模块入口
    ├── unwind.rs            # 预处理/字符替换 (230 行)
    └── char2sd_list.rs      # ASCII 字符映射表 (67 行)
```

## 3. 核心类型体系

### 3.1 类型关系图

```
ErrorKind ────────────── StardustError
    │                        │
    │                    SourceSpan
    │
TokenType ───────────── Token
    │
    │  Parser ──── ParseResult ──── FunctionDef ──── Rc<Vec<Instruction>>
    │                  │                                     │
    │             main_instructions                    CallFrame
    │             main_marks                               │
    │             functions                          VM { frames, heap, ... }
    │
Instruction ──── InstrMeta ──── SourceSpan
```

### 3.2 关键数据结构

| 类型 | 定义位置 | 说明 |
|------|----------|------|
| `TokenType` | `mod.rs` | 16 种符号的枚举 |
| `Token` | `mod.rs` | { spaces, token_type, line, column, byte_pos } |
| `Instruction` | `mod.rs` | 36 种指令变体的枚举，各携带 InstrMeta |
| `InstrMeta` | `mod.rs` | { span: SourceSpan { line, column } } |
| `SourceSpan` | `mod.rs` | { line: usize, column: usize } |
| `CallFrame` | `mod.rs` | { instructions: Rc<Vec<Instruction>>, stack, pc, marks, ret_pc } |
| `FunctionDef` | `mod.rs` | { body: Rc<Vec<Instruction>>, marks: HashMap<usize, usize> } |
| `ParseResult` | `mod.rs` | { main_instructions, main_marks, functions } |
| `VM` | `mod.rs` | { functions, frames, heap, halted, debug, config } |
| `ErrorKind` | `mod.rs` | 35+ 种错误变体的枚举 |
| `StardustError` | `mod.rs` | { kind, span, message } |
| `VmConfig` | `config.rs` | { max_steps, max_stack_size, max_heap_entries, max_call_depth, time_limit_secs, char_mode } |
| `Debugger` | `debugger.rs` | { breakpoints: HashSet<usize>, single_step: bool } |
| `VmIO` (trait) | `vm.rs` | write_char / read_char / write_num / read_num |
| `StdIO` | `vm.rs` | VmIO 的 stdin/stdout 实现 |
| `VmSnapshot` | `vm.rs` | 错误时的 VM 状态快照 |

### 3.3 ParseResult 的结构

```rust
pub struct ParseResult {
    /// 主程序的指令序列
    pub main_instructions: Vec<Instruction>,

    /// 主程序的 Marks：名称 → 指令索引
    pub main_marks: HashMap<usize, usize>,

    /// 函数定义表：函数名 → FunctionDef
    /// FunctionDef { body: Rc<Vec<Instruction>>, marks: HashMap<usize, usize> }
    pub functions: HashMap<usize, FunctionDef>,
}
```

**设计意图**：ParseResult 是一个完全自包含的、与源码无关的程序表示。它可以直接送入 VM 执行或 LLVM 编译器。

### 3.4 CallFrame 的结构

```rust
pub struct CallFrame {
    /// 指令序列（Rc 共享，避免每次调用克隆）
    pub instructions: Rc<Vec<Instruction>>,

    /// 当前帧的独立栈
    pub stack: Vec<i64>,

    /// 程序计数器（指令索引）
    pub pc: usize,

    /// Mark 名称 → 对应指令索引
    pub marks: HashMap<usize, usize>,

    /// 返回后父帧应继续执行的 PC（Call 指令的下一条）
    pub ret_pc: usize,
}
```

## 4. 关键设计决策

### 4.1 Rc 共享函数体

函数体以 `Rc<Vec<Instruction>>` 存储在 `FunctionDef` 中。当函数被调用时，`Rc::clone()` 仅增加引用计数，不复制指令向量。VM 执行循环中使用 `Rc::clone(&frame.instructions)` 获取当前指令的共享引用。

### 4.2 预计算 Marks

Marks 在解析时预计算为 `HashMap<usize, usize>`（名称 → 指令索引）。`FunctionDef` 中包含预计算的 marks，在 `new CallFrame` 时直接 clone。避免了每次函数调用时重新扫描指令序列。

### 4.3 编译期 Mark 验证

引用完整性（Jump/UncondJump 引用的 Mark 是否存在）在解析阶段验证，而非运行时：

- `validate_mark_references()` — 验证主程序中的引用
- `validate_function_mark_references()` — 验证每个函数体中的引用
- 同时检测重复 Mark 声明

### 4.4 多栈帧 + 共享堆

- **栈**：每帧独立 (`CallFrame::stack: Vec<i64>`)
- **堆**：全局共享 (`VM::heap: HashMap<i64, i64>`)
- 函数返回时帧栈合并到父帧（`parent.stack.extend(finished.stack)`）

### 4.5 VmIO trait 抽象

I/O 通过 trait 抽象，支持依赖注入：

```rust
pub trait VmIO {
    fn write_char(&mut self, ch: char) -> io::Result<()>;
    fn read_char(&mut self) -> io::Result<u8>;
    fn write_num(&mut self, n: i64) -> io::Result<()>;
    fn read_num(&mut self) -> io::Result<i64>;
}
```

当前仅有 `StdIO` 实现（stdin/stdout）。`MockIO` 计划用于测试。

### 4.6 借用冲突解决

调试器需要 `&VM` 来显示状态，但 VM 的 `&mut self` 引用已被执行循环持有。解决方案：

```rust
// 在需要 &VM 之前
let mut debug = self.debug.take();  // 取出 Debugger

// 使用 &VM (通过 &self 方法)
debug.interact(self);  // self 是 &VM

// 交互完成后
self.debug = debug;  // 放回 Debugger
```

这是 `Option::take()` 模式的典型应用，避免了 `RefCell` 的运行时开销。

## 5. 错误处理模式

### 5.1 错误生成

```
┌──────────┐    StardustError::new(kind, Some(span))
│ ErrorKind │ ────────────────────────────────────────── StardustError
│ + span    │    .with_message("custom") 可选
└──────────┘    auto: kind.default_message()
```

### 5.2 错误传播

所有流水线阶段使用 `Result<T, StardustError>`。CLI 层统一通过 `print_error()` 输出，包含：

1. 错误消息（从 ErrorKind 自动生成）
2. 源码位置（文件名:行:列）
3. 源码上下文（错误行内容 + `^` 指针）

### 5.3 JSON 诊断

`--check` 和 VSCode 扩展使用 JSON 格式：
```json
{
  "status": "error",
  "diagnostics": [{
    "severity": "error",
    "line": 5, "column": 10,
    "message": "undefined mark '99'",
    "code": "UndefinedMark"
  }]
}
```

## 6. Cargo 配置

```toml
[package]
name = "esolang_stardust"
version = "0.1.6"
edition = "2024"
rust-version = "1.95"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[lints.rust]
unsafe_code = "forbid"
dbg_macro = "warn"
todo = "warn"
unwrap_used = "warn"
```

**Lint 策略**：禁止 unsafe 代码，warn unwrap 使用（测试中允许）。

---

> 相关文件：[vm.md](vm.md) · [pipeline.md](pipeline.md) · [../file-index.md](../file-index.md)
