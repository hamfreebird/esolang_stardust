# 错误类型系统

> **实现位置**：`src/stardust/error.rs` (114 行) · `src/stardust/mod.rs` (ErrorKind 枚举定义)

## 1. 错误结构

```rust
pub struct StardustError {
    pub kind: ErrorKind,            // 错误分类
    pub span: Option<SourceSpan>,   // 源码位置（词法/语法/运行时错误均有）
    pub message: String,            // 人类可读消息
}

pub struct SourceSpan {
    pub line: usize,    // 1-based
    pub column: usize,  // 1-based
}
```

## 2. 错误生成

```rust
// 标准构造（自动从 ErrorKind 生成消息）
StardustError::new(kind, Some(span))

// 自定义消息覆盖
StardustError::new(kind, Some(span)).with_message("custom message")
```

## 3. ErrorKind 完整枚举（36 种变体）

### 3.1 词法错误（4 种）

| 变体 | 阶段 | 触发条件 | 消息示例 |
|------|------|----------|----------|
| `InvalidCharacter { ch }` | Lexer | 遇到非有效字符 | `"invalid character 'a'"` |
| `NonSymbolicCharacter` | Lexer | 空格后跟了非符号字符 | `"non-symbolic character"` |
| `TrailingSpaces` | Lexer | （保留，当前未使用） | — |
| `InvalidAnnotation` | Lexer | 单 `/` 而非 `//` | `"invalid annotation, use '//'"` |

### 3.2 语法/解析错误（20 种）

| 变体 | 阶段 | 触发条件 | 消息示例 |
|------|------|----------|----------|
| `UnexpectedToken { expected, found }` | Parser | 不期望的 token 类型 | `"expected instruction symbol, found ..."` |
| `DuplicateMark { name }` | Parser | 同一作用域内重复 Mark | `"duplicate mark '3'"` |
| `DuplicateFunction { name }` | Parser | 重复声明同名函数 | `"duplicate function '5'"` |
| `UndefinedMark { name }` | Parser | Jump 引用不存在的 Mark | `"undefined mark '99'"` |
| `UndefinedFunction { name }` | VM | Call 引用不存在的函数 | `"undefined function '7'"` |
| `UnclosedFunction { name }` | Parser | 函数声明缺少结束标记 | `"unclosed function '1'"` |
| `UnexpectedEof` | Parser | 解析过程中遇 EOF | `"unexpected end of file"` |
| `IncompleteFunctionCall` | Parser | Call 的 Colon 后无 Semicolon | `"incomplete function call"` |
| `ExpectedSemicolonAfterCall` | Parser | Colon 后不是 Semicolon | `"expected ';' at end of function call"` |
| `InvalidSpacesForPlus` | Parser | `+` 使用 0 空格 | `"invalid number of spaces for '+' instruction (must be >= 1)"` |
| `InvalidSpacesForStar { spaces }` | Parser | `*` 使用 6+ 空格 | `"invalid number of spaces (6) for '*' instruction (must be 0-5)"` |
| `InvalidSpacesForDot { spaces }` | Parser | `.` 使用 2+ 空格 | `"invalid number of spaces (2) for '.' instruction (must be 0 or 1)"` |
| `InvalidSpacesForComma { spaces }` | Parser | `,` 使用 2+ 空格 | 同上 |
| `InvalidSpacesForEquals { spaces }` | Parser | `=` 使用 6+ 空格 | `"...for '='... (must be 0-5)"` |
| `InvalidSpacesForAmpersand { spaces }` | Parser | `&` 使用 4+ 空格 | `"...for '&'... (must be 0-3)"` |
| `InvalidSpacesForHyphen { spaces }` | Parser | `-` 使用 2+ 空格 | `"...for '-'... (must be 0 or 1)"` |
| `InvalidSpacesForAngleLeft { spaces }` | Parser | `<` 使用 3+ 空格 | `"...for '<'... (must be 0-2)"` |
| `InvalidSpacesForAngleRight { spaces }` | Parser | `>` 使用 2+ 空格 | `"...for '>'... (must be 0 or 1)"` |
| `InvalidSpacesForHash { spaces }` | Parser | `#` 使用 3+ 空格 | `"...for '#'... (must be 0-2)"` |
| `InvalidInstructionContext` | Parser | 指令出现在非法上下文 | `"instruction not allowed in this context"` |
| `NotEnoughArguments { func, expected, actual }` | VM | 函数调用参数不足 | `"not enough arguments for function '1': expected 3, but stack has 2"` |

### 3.3 运行时错误（14 种）

| 变体 | 阶段 | 触发条件 | 消息示例 |
|------|------|----------|----------|
| `StackUnderflow` | VM | 栈空时尝试弹出 | `"stack underflow"` |
| `DivisionByZero` | VM | Div 除数为 0 | `"division by zero"` |
| `ModuloByZero` | VM | Mod 除数为 0 | `"modulo by zero"` |
| `InvalidAscii { value }` | VM | ASCII 模式 CharOut 值超出 0–127 | `"ASCII value 200 out of range (0-127)"` |
| `InvalidIntegerInput` | VM | NumIn 无法解析为整数 | `"invalid integer input"` |
| `IoError { reason }` | VM | stdin/stdout 操作失败 | `"I/O error: ..."` |
| `IntegerOverflow` | VM | Add/Sub/Mul 结果溢出 i64 | `"integer overflow"` |
| `CallDepthExceeded` | VM | 调用深度超 max_call_depth | `"maximum call depth exceeded — possible infinite recursion"` |
| `StackOverflow` | VM | 栈大小超 max_stack_size | `"stack overflow — maximum stack size exceeded"` |
| `HeapOverflow` | VM | 堆条目超 max_heap_entries | `"heap overflow — maximum heap entries exceeded"` |
| `InvalidCodePoint { value }` | VM | Unicode 模式 CharOut 值非有效码点 | `"invalid Unicode code point: 1114113"` |
| `StepLimitExceeded { steps }` | VM | 步数达 max_steps | `"step limit exceeded (10000000)"` |
| `TimeLimitExceeded` | VM | 执行时间超 time_limit | `"time limit exceeded"` |
| `InvalidAnnotation` | Lexer | 单 `/` 而非 `//` | `"invalid annotation, use '//'"` |

### 3.4 保留/未实现变体（3 种）

这三个变体在 `ErrorKind` 枚举中定义但 `default_message()` 未覆盖，遇到时显示 catch-all 消息：

| 变体 | 状态 | 默认消息 |
|------|------|----------|
| `ParseChar` | 保留 | catch-all（开发者提示） |
| `StdIoError` | 保留 | catch-all（开发者提示） |
| `CodePointTooLarge` | 保留 | catch-all（开发者提示） |

**Catch-all 消息**（`error.rs:97-101`）：
```
"unknown error
 这条消息面向开发者，对，说的就是你，freebird，你是不是忘了把错误类型加到default_message里了？？
 Ignore this error message. It's a problem with the interpreter itself"
```

## 4. 错误格式化

### 4.1 Display 实现

```rust
impl fmt::Display for StardustError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(span) = &self.span {
            write!(f, "{} at {}", self.message, span)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}
```

**输出示例**：
- 带位置：`"division by zero at line 5, column 10"`
- 无位置：`"time limit exceeded"`

### 4.2 CLI 错误输出（print_error）

`print_error()` 函数（`src/stardust/utils.rs`）提供带源码上下文的格式化输出：

```
Error: undefined mark '99' at line 5, column 10
  → file.sd:5:10
  5 |   ' ~
    |       ^
```

### 4.3 JSON 诊断输出（--check / VSCode）

```json
{
  "status": "error",
  "diagnostics": [
    {
      "severity": "error",
      "line": 5,
      "column": 10,
      "message": "undefined mark '99'",
      "code": "UndefinedMark"
    }
  ]
}
```

## 5. 错误生命周期

```
┌──────────────┐
│ ErrorKind     │  枚举变体（36 种）
│ + span        │  可选的源码位置
└──────┬───────┘
       │ StardustError::new(kind, span)
       ▼
┌──────────────┐
│ StardustError │  { kind, span, message }
│   .message    │  由 ErrorKind::default_message() 自动生成
└──────┬───────┘
       │ Display trait
       ▼
┌──────────────┐
│ 用户可见输出   │  CLI: print_error() 带源码上下文
│               │  JSON: print_json_diagnostics()
└──────────────┘
```

## 6. 实现细节

### 6.1 ErrorKind::default_message()

每个变体硬编码了默认消息（大多数为英文）。`_` 通配分支提供中文 catch-all 消息。

### 6.2 from impl

`StardustError` 实现了 `From<io::Error>`，允许文件读取错误自动转换（不携带源码位置）。

### 6.3 Serialize

`TokenType`, `Token`, `Instruction`, `InstrMeta`, `SourceSpan`, `ErrorKind`, `StardustError` 均实现了 `serde::Serialize`，支持 JSON 序列化（用于 `--tokens` 和 `--check` 模式）。

---

> 相关文件：[implementation/pipeline.md](implementation/pipeline.md)
