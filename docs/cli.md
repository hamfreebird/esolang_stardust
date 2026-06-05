# CLI 运行模式

> **实现位置**：`src/main.rs` (395 行)

## 1. 入口点

```rust
fn main() {
    let args: Vec<String> = env::args().collect();
    stardust(args);
}
```

`stardust()` 函数首先提取标志参数（`--repl`、`--debug`、`--help`），然后根据第一个位置参数分发到对应的命令处理函数。

## 2. 模式总览

| # | 模式 | 命令格式 | 处理函数 |
|---|------|----------|----------|
| 1 | 解释执行 | `stardust <file.sd>` | `cmd_run()` |
| 2 | 调试执行 | `stardust -d <file.sd>` | `cmd_run()` with debug |
| 3 | REPL | `stardust -r` | `repl::repl_main()` |
| 4 | 调试 REPL | `stardust -r -d [file]` | `repl::repl_main()` with debug |
| 5 | 语法检查 | `stardust --check <file.sd>` | `cmd_check()` |
| 6 | Token 导出 | `stardust --tokens <file.sd>` | `cmd_tokens()` |
| 7 | LLVM IR 编译 | `stardust -c <file.sd> [out.ll]` | `cmd_compile()` |
| 8 | 二进制编译 | `stardust -b <file.sd> [out]` | `cmd_build()` |
| 9 | 文本转换 | `stardust -s <input.txt> [out]` | `cmd_stardust()` |
| 10 | 流水线分析 | `stardust --dump <file.sd> [out]` | `cmd_dump()` |
| 11 | 帮助 | `stardust -h` | `print_cli_usage()` |

## 3. 模式一：解释执行（默认）

```
stardust program.sd
stardust --debug program.sd
stardust -d program.sd
```

### 完整流水线

```
read file → parse_config → simple_preprocess → tokenize → parse_program
    → VM::new(parsed, config) → [debugger setup] → VM::run()
```

### 参数验证

- 严格需要 1 个文件参数
- 文件扩展名必须为 `.sd` 或 `.stardust`
- 错误时退出码 1

### 调试模式行为

- `--debug` / `-d` 标志在参数提取阶段识别
- `vm.debug = Some(Debugger::new())` → 第一条指令处自动暂停
- 调试器输出写入 stderr

### 错误处理流程

```
任何阶段失败:
  print_error(&error, &source, filename)
    → 打印错误消息 + 源码位置 + 源码上下文
  process::exit(1)
```

## 4. 模式二：LLVM IR 编译

```
stardust --compile hello.sd
stardust --compile hello.sd output.ll
stardust -c hello.sd
```

### 流水线

```
parse_input_file() → ParseResult
    → codegen::compile_to_ir(&parse_result, &CodeGenConfig::default())
    → fs::write(output_path, ir_string)
```

### 输出

- 默认输出路径：替换 `.sd` 扩展名为 `.ll`
- 输出纯文本 LLVM IR（可被 `llc`/`clang` 处理）
- 控制台打印：`LLVM IR generated: path/to/output.ll`
- 提示：`Compile with: clang output.ll -o program`

## 5. 模式三：二进制编译

```
stardust --build hello.sd
stardust --build hello.sd myprogram
stardust -b hello.sd
```

### 预检查

```rust
let tools = codegen::check_toolchain();
if !tools.can_compile() {
    eprintln!("Error: LLVM toolchain not found.");
    eprintln!("  llc:  {}", if tools.llc { "✓" } else { "✗ not found" });
    eprintln!("  clang: {}", if tools.clang { "✓" } else { "✗ not found" });
    process::exit(1);
}
```

### 编译流水线

```
ParseResult
    → compile_to_exe()
        → compile_to_ir() — 生成 LLVM IR
        → 写入临时 .ll 文件
        → llc — 编译为 .o 目标文件
        → clang — 链接为可执行文件
```

## 6. 模式四：语法检查

```
stardust --check code.sd
```

### 流水线

```
read → simple_preprocess → tokenize → parse_program
```

### 输出

**成功时**：
```json
{"status":"ok","diagnostics":[]}
```

**失败时**：
```json
{
  "status":"error",
  "diagnostics":[
    {"severity":"error","line":5,"column":10,"message":"...","code":"UndefinedMark"}
  ]
}
```

退出码：语法正确 0，语法错误 0（错误信息体现在 JSON 中）。

## 7. 模式五：Token 导出

```
stardust --tokens code.sd
```

### 输出

```json
{
  "tokens": [
    {"spaces":5,"token_type":"Plus","line":1,"column":1,"byte_pos":5}
  ]
}
```

## 8. 文本转换

```
stardust --stardust message.txt
stardust --stardust message.txt output.sd
stardust -s message.txt
```

将可见 ASCII 文本转换为 Stardust 源码（Push+CharOut 序列）。输出文件默认扩展名 `.stardust`。

## 9. 流水线分析

```
stardust --dump program.sd
stardust --dump program.sd analysis.txt
```

执行完整流水线，输出全部 4 个阶段的中间结果（原始源码、预处理后源码、Token 流、解析结果）。

## 10. REPL 模式

```
stardust --repl
stardust -r
stardust -r -d
stardust -r -d program.sd
```

详见 [repl.md](repl.md)。

`-r` 和 `-d` 标志在参数提取阶段被优先处理，直接调用 `repl::repl_main(has_debug, file)` 后返回。

## 11. 辅助函数

### parse_input_file()

CLI 使用的统一解析流水线：
```rust
fn parse_input_file(filename: &str) -> ParseResult {
    let source = fs::read_to_string(filename)?;
    let unwind_source = simple_preprocess(&source)?;
    let tokens = tokenize(&unwind_source)?;
    parse_program(tokens)?
    // 失败时 print_error() 并 exit(1)
}
```

### read_and_unwind()

用于 `--check` 和 `--tokens` 模式：
```rust
fn read_and_unwind(filename: &str)
    -> Result<(String, String), StardustError>
```
返回 `(原始源码, 预处理后源码)` 元组。

### print_json_diagnostics()

将 `&[&StardustError]` 格式化为 VSCode 兼容的 JSON 诊断输出。

---

> 相关文件：[repl.md](repl.md) · [debugger.md](debugger.md) · [codegen.md](codegen.md)
