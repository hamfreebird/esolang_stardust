# 编译流水线

> 从 .sd 源码到 VM 执行或 LLVM 编译的完整数据流

## 1. 流水线阶段

```
源文件 (.sd)
    │
    ▼
┌──────────────────────────────────────────────────────┐
│ 阶段 0: 文件读取                                      │
│   fs::read_to_string(filename) → String              │
│   错误: io::Error → StardustError (文件不存在/权限)     │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 阶段 1: 配置提取                                      │
│   parse_config(&source) → (VmConfig, String)          │
│   提取 #[key=value] 行，返回清洁源码                    │
│   错误: ParseConfigError (未知键/无效值)                │
│   实现: src/stardust/config.rs                        │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 阶段 2: 预处理                                        │
│   simple_preprocess(&clean_source) → Cow<str>         │
│   文本字符替换（如 ASCII 字母展开为 Stardust 序列）      │
│   错误: 预处理错误 → StardustError                     │
│   实现: src/extension/unwind.rs                       │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 阶段 3: 词法分析                                      │
│   tokenize(&unwind_source) → Vec<Token>              │
│   字符流 → 结构化 Token 序列                           │
│   错误: InvalidCharacter / NonSymbolicCharacter /     │
│         InvalidAnnotation                             │
│   实现: src/stardust/lexer.rs                         │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 阶段 4: 语法分析                                      │
│   parse_program(tokens) → ParseResult                │
│   Token 序列 → 结构化指令 + 函数表 + Marks              │
│   验证: Mark 引用完整性、重复定义检测                    │
│   错误: UndefinedMark / DuplicateFunction /           │
│         InvalidSpacesFor* / 等 20+ 种                  │
│   实现: src/stardust/parser.rs                        │
└──────────────────────┬───────────────────────────────┘
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
    VM::run()   编译为 LLVM IR   --check (JSON)
```

## 2. 阶段 1 详解：配置提取

```rust
pub fn parse_config(source: &str) -> Result<(VmConfig, String), ParseConfigError>
```

**处理逻辑**：

1. 从 `VmConfig::default()` 开始
2. 逐行扫描：
   - 若行以 `#[` 开头、以 `]` 结尾、含 `=` → 提取 key=value，调用 `apply_config()`
   - 否则保留到清洁源码
3. 清洁源码用 `\n` 连接保留行，保持原始行号

**为什么在词法分析之前**：注解行被完全移除，不会产生 Token，确保词法分析器只看到"纯净的" Stardust 代码。

## 3. 阶段 2 详解：预处理

```rust
pub fn simple_preprocess(source: &str) -> Result<Cow<str>, StardustError>
```

**处理内容**：

- 将 ASCII 字母数字字符转换为对应的 Stardust Push+CharOut 序列
- 无替换时返回 `Cow::Borrowed`（零分配）
- 当前实现较为基础（带改进空间）

**为什么需要预处理**：允许用户混合普通文本和 Stardust 代码，命令行工具（`--stardust`）将纯文本完全转换为 Stardust 格式。

## 4. 阶段 3 详解：词法分析

参见 [../language/lexer.md](../language/lexer.md) 获取完整词法规则。

**关键接口**：
```rust
pub fn tokenize(source: &str) -> Result<Vec<Token>, StardustError>
```

一次性解析整个源文件，在第一个错误处停止。

## 5. 阶段 4 详解：语法分析

```rust
pub fn parse_program(tokens: Vec<Token>) -> Result<ParseResult, StardustError>
```

**解析顺序**（`Parser::parse()`）：

1. 遍历所有 Token，调用 `parse_top_level()`
2. 顶层 Colons 分派到 `parse_colon_at_top_level()`（区分声明/调用）
3. 其他 Token 分派到 `parse_simple_instruction()`（spaces+symbol → Instruction）
4. 收集函数体（遇到匹配的 Colon 结束）
5. 解析完成后：
   - `resolve_main_marks()` — 收集 Marks 并检测重复
   - `validate_mark_references()` — 验证 Jump 引用
   - `validate_function_mark_references()` — 验证函数体内 Jump 引用
   - 包装函数体为 `FunctionDef { body: Rc, marks }`
6. 返回 `ParseResult`

## 6. 解析后的分支

### 6.1 VM 解释执行

```rust
let mut vm = VM::new(parse_result, config);
if debug_mode { vm.debug = Some(Debugger::new()); }
vm.run()?;
```

### 6.2 LLVM 编译

```rust
let config = CodeGenConfig::default();
let ir = compile_to_ir(&parse_result, &config);  // → String (LLVM IR)
// 或
compile_to_exe(&parse_result, &output_path, &config)?;  // → 二进制
```

### 6.3 语法检查

```rust
// --check 模式：仅运行到解析成功，输出 JSON
match parse_program(tokens) {
    Ok(_) => println!(r#"{{"status":"ok","diagnostics":[]}}"#),
    Err(e) => print_json_diagnostics(&[&e]),
}
```

## 7. 错误传播

所有阶段统一使用 `Result<T, StardustError>`：

```
阶段失败 → StardustError { kind, span, message }
    │
    ▼
CLI 层 print_error() 或 print_json_diagnostics()
    │
    ▼
退出码 1（检查模式退出码 0 但输出错误 JSON）
```

**错误上下文输出格式**：
```
Error: undefined mark '99' at line 5, column 10
  → file.sd:5:10
  5 |   ' ~
    |       ^
```

## 8. --dump 模式

`stardust --dump file.sd` 执行完整流水线并输出每个阶段的中间结果：

1. 原始源码
2. 预处理后源码
3. Token 流
4. 解析结果（指令列表 + Marks + 函数表）

使用 `StageResult` 枚举表示每个阶段的结果：
```rust
pub enum StageResult {
    Source(String),
    UnwindSource(String),
    Tokens(Vec<Token>),
    Parsed(ParseResult),
    Error(String),
    None,
}
```

---

> 相关文件：[../language/lexer.md](../language/lexer.md) · [vm.md](vm.md) · [../cli.md](../cli.md)
