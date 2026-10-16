# 测试体系

> **实现位置**：各源文件内联 `#[cfg(test)] mod tests` + `tests/integration_test.rs` (1373 行)

## 1. 测试概况

| 模块 | 位置 | 测试数（约） | 类型 |
|------|------|------------|------|
| 词法分析器 | `src/stardust/lexer.rs` | 97 | 单元测试 |
| 语法分析器 | `src/stardust/parser.rs` | 63 | 单元测试 |
| 配置解析 | `src/stardust/config.rs` | 12 | 单元测试 |
| IR 优化器 | `src/codegen/optimizer.rs` | 38 | 单元测试 |
| LLVM 编译器 | `src/codegen/compiler.rs` | 47 | 单元测试 |
| REPL 解析器 | `src/repl/parser.rs` | 14 | 单元测试 |
| REPL 执行器 | `src/repl/executor.rs` | 8 | 单元测试 |
| REPL 显示 | `src/repl/display.rs` | 14 | 单元测试 |
| REPL 主循环 | `src/repl/mod.rs` | 3 | 单元测试 |
| 预处理 | `src/extension/unwind.rs` | 20 | 单元测试 |
| 集成测试 | `tests/integration_test.rs` | 109 | 集成测试 |
| LLVM 集成测试 | `tests/integration_test.rs` (codegen_tests) | 18 | 集成测试 |
| **总计** | | **425** | |

### 1.1 运行测试

```bash
# 运行全部测试
cargo test

# 仅运行单元测试
cargo test --lib

# 仅运行集成测试
cargo test --test integration_test

# 运行特定模块
cargo test lexer
cargo test parser
cargo test codegen
```

### 1.2 Clippy 配置

`.clippy.toml`:
```toml
allow-unwrap-in-tests = true
```

测试中允许 `unwrap()` 使用（生产代码中 `unwrap_used` 是 warn 级别）。

## 2. 词法分析器测试（97 个）

**文件**：`src/stardust/lexer.rs` 内联 `#[cfg(test)] mod tests`

### 测试分类

| 类别 | 测试数 | 覆盖 |
|------|--------|------|
| 单符号识别 | 16 | 全部 16 种符号的无空格 Token 生成 |
| 前导空格 + 符号 | 10 | 1/3/5/10 空格 + 各种符号 |
| 行号列号跟踪 | 5 | 多 token 同行、多行、空行跳过、列号重置 |
| 空白字符处理 | 6 | Tab、\r、混合空白、多换行、空格跨行不累计 |
| 注释处理 | 9 | 整行注释、指令后注释、注释含符号、多注释交替 |
| 单斜杠错误 | 4 | `/`、`/a`、`/ `、`  /` 四种 InvalidAnnotation |
| 无效字符错误 | 6 | 字母、数字、标点、空格+字母、空格+数字 |
| EOF 处理 | 5 | 空输入、仅空格、尾部空格、尾部换行、仅空白 |
| byte_pos 跟踪 | 4 | 简单、带空格、多 token、换行后 |
| tokenize() 包装 | 5 | 正常、空串、错误停止、仅注释 |
| 综合场景 | 8 | Hello World 片段、函数声明/调用、Mark+Jump |
| 辅助函数 | 6 | is_symbol、is_anno、char_to_token_type 映射 |
| 边界情况 | 7 | EOF 无换行、连续符号、大空格数(200)、注释+EOF |

### 辅助工具函数

```rust
fn tok(spaces, tt, line, col, bp) -> Token  // 快速构造测试 Token
fn collect_all(source) -> Vec<Token>         // 收集所有 Token（unwrap）
fn collect_until_err(source) -> (Vec<Token>, Option<StardustError>)  // 收集到错误
```

## 3. 语法分析器测试（63 个）

**文件**：`src/stardust/parser.rs` 内联 `#[cfg(test)] mod tests`

### 测试分类

| 类别 | 测试数 | 覆盖 |
|------|--------|------|
| Push / 栈操作 | 7 | Push(0/1/5/100), Dup, Swap, Rotate, Pop, InvalidSpacesForPlus |
| 算术运算 | 7 | Add, Sub, Mul, Div, Mod, Reverse, InvalidSpacesForStar |
| I/O | 6 | NumOut, NumIn, CharOut, CharIn, InvalidSpacesForDot/Comma |
| 控制流 | 6 | Mark(0/5), Jump(3), UncondJump(7), UndefinedMark(99) (compile-time) |
| 重复 Mark | 2 | DuplicateMark, 多个不同 Mark |
| 函数声明 | 5 | 简单声明、含 Mark+Jump、DuplicateFunction、UnclosedFunction、嵌套 Call |
| 函数调用 | 4 | 顶层 Call、零参数 Call、声明后调用、argc 来源 |
| 混合指令 | 2 | 多指令序列、Hello World 风格 |
| 比较运算 | 7 | Eq, Ne, Lt, Gt, Le, Ge, InvalidSpacesForEquals |
| 逻辑运算 | 2 | And, Not |
| 堆操作 | 2 | Store, Load |
| 栈扩展 | 5 | ShiftL, Depth, Pick, ShiftR, DropN |
| 调试 | 3 | DumpStack, DumpState, Breakpoint |
| SourceSpan | 1 | Mark 携带正确行列信息 |
| 空程序 | 1 | 空 Token 列表 |

## 4. VM 运行时测试（通过集成测试）

VM 本身没有内联单元测试（测试通过 `tests/integration_test.rs` 中的端到端测试覆盖）。这包括：

### 4.1 基础指令测试

- Push + Dup/Swap/Rotate/Pop 栈操作
- Add/Sub/Mul/Div/Mod/Reverse 算术运算
- NumOut/NumIn/CharOut/CharIn I/O（使用 MockIO 或字符串捕获）

### 4.2 控制流测试

- Mark 声明和 Jump 条件/无条件跳转
- 循环行为（Mark + Jump 实现循环计数）
- 跳转到不存在的 Mark 在运行时也触发错误

### 4.3 函数测试

- 函数声明和零参数调用
- 带参数函数调用（argc > 0）
- 嵌套函数调用（多帧）
- 递归函数（阶乘、斐波那契）
- 多值返回（函数栈合并回父帧）
- 函数调用参数不足 → NotEnoughArguments
- 调用未定义函数 → UndefinedFunction
- 调用深度超限 → CallDepthExceeded

### 4.4 错误测试（12 种运行时错误）

| 错误类型 | 测试方式 |
|----------|----------|
| StackUnderflow | 空栈上执行 Pop/Dup/Swap 等 |
| DivisionByZero | Push(0) Push(5) Div |
| ModuloByZero | Push(0) Push(5) Mod |
| InvalidAscii | Push(128) CharOut（ASCII 模式） |
| UndefinedMark | Jump(99)（运行时回退检查） |
| NotEnoughArguments | 调用函数但栈不足 |
| IntegerOverflow | Push(i64::MAX) Push(1) Add |
| CallDepthExceeded | 无限递归 |
| StepLimitExceeded | 无限循环 + 低 max_steps |
| StackOverflow | 大量 Push + 低 max_stack |
| HeapOverflow | 大量 Store + 低 max_heap |

## 5. LLVM 编译器测试（47 个）

**文件**：`src/codegen/compiler.rs` 内联测试

- 测试每条指令生成有效的 LLVM IR
- 验证 IR 语法（通过 llc 解析）
- 可选：编译并运行，验证输出正确性

**codegen_tests 模块**（`tests/integration_test.rs` 内）：

- 完整的端到端 LLVM 编译测试：Hello World、算术、控制流、函数调用
- 需要系统安装 `clang` 和 `llc`

### 5.1 IR 合法性校验（关键护栏）

`every_single_instruction_produces_valid_ir` 与
`deep_expression_program_produces_valid_ir` / `function_program_produces_valid_ir`
为**每条指令**构造最小程序，生成 IR 后用 `clang -c -o /dev/null` 校验，
clang 拒绝 IR 时测试直接 panic。

这组测试的存在意义：编译器单元测试此前只做 `ir.contains("@malloc")` 这类
字符串断言，无法发现非法 IR，导致以下缺陷长期存在且测试全绿——

- `Reverse` / `Pick` 连续两行定义同名 SSA 变量 → `multiple definition`
- `DumpStack` / `DumpState` 调用从未定义的 `@fprintf_stderr` → 链接失败
- `compile_ujump` 的死块缺少终结指令 → 后续 `mark_` 标签使整份 IR 非法
- 错误消息 `at %s` 无对应实参 → printf 读出乱码字节

> 修改 `src/codegen/` 时务必同步在此添加用例。新增指令不仅要能生成 IR，
> 还必须通过 clang 的解析与验证。

## 6. REPL 测试

### 6.1 简写解析器（14 个）

- Push/算术/I/O/控制流/函数调用的简写解析
- 字符串展开
- 错误输入处理

### 6.2 执行器（8 个）

- ReplContext 创建和状态持久化
- 栈/堆状态注入和取回
- 函数定义持久化

### 6.3 显示（14 个）

- stack/heap/info/nearby/frames 格式化输出
- 截断行为（栈超过 20 项、堆超过 30 条）
- 空状态显示

### 6.4 主循环（3 个）

- detect_mode 简写/原始/空输入判定

## 7. 其他测试

### 配置解析（12 个）

- 默认值验证
- max_steps/time_limit 设为 0 = 无限制
- 单配置/多配置/无配置解析
- 未知键错误

### 预处理（20 个）

- ASCII 字母/数字映射
- 字符串正确性
- 无替换时的零分配（Cow::Borrowed）

### IR 优化器（38 个）

- 常量折叠：所有算术/比较/逻辑操作的 Push+Op 模式
- 死代码消除：Push+Pop、Dup+Pop
- 窥孔优化：Push(0)+Add、Push(1)+Mul、Reverse+Reverse、ShiftL+ShiftR
- 不动点迭代（组合优化）

---

> 相关文件：[../file-index.md](../file-index.md) · [implementation/pipeline.md](implementation/pipeline.md)
