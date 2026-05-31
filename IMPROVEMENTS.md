# Stardust 项目改进建议

> 基于对项目结构、目的和实现的详细阅读分析，整理的可改进项目清单。

---

## 一、严重问题

### 1. `char2sd_list.rs` 中字符映射大量为空

**文件:** `src/extension/char2sd_list.rs`

只有 `A`-`I` (ASCII 65-73) 有实际 Stardust 代码映射，`J`-`Z`、`a`-`z`、`0`-`9` 全部为空字符串（约 50 个）。`preprocess()` 函数对大部分字母数字字符输出空串，导致"生成美观 Stardust 代码"功能实际上不可用。

`HELLO_WORLD` 常量已定义但从未被任何代码引用。

**建议:**
- 补全所有字符映射（为每个 ASCII 字母数字编写通过计算得到的 Stardust 代码）
- 或者移除 `preprocess` 复杂路径，统一使用 `simple_preprocess`（直接将 ASCII 码作为 Push 值压栈）
- 如果保留，将 `HELLO_WORLD` 用作示例或从代码中移除

### 2. ~~VM 中存在大量代码重复~~ ✅ 已解决

**文件:** `src/stardust/vm.rs`

~~`execute_instruction()`（操作 `self.main_stack`）和 `handle_function_call()` 中的函数体执行循环（操作 `new_stack`）对每条指令的实现几乎完全相同，区别仅在于操作的栈不同。约 200 行重复代码。~~

**解决方案:**
- 提取了 `StackExecutor` 结构体，封装了 `stack: &mut Vec<i64>`、`pc: &mut usize`、`marks: &HashMap<usize, usize>` 和统一的指令执行逻辑
- `execute_instruction()` 和函数体执行循环现在共享同一套代码（~180 行）
- VM 职责回归到生命周期管理（调用栈、函数库）

### 3. ~~运行时错误缺少源码位置信息~~ ✅ 已解决

**文件:** `src/stardust/vm.rs`, `src/stardust/mod.rs`, `src/stardust/parser.rs`

~~VM 中所有运行时错误通过 `self.error(kind)` 创建，`span` 字段永远为 `None`。~~

**解决方案:**
- 新增 `InstrMeta` 结构体（含 `SourceSpan`），附加到每条 `Instruction` 变体上
- Parser 在构造 Instruction 时从 Token 提取位置信息填入 InstrMeta
- VM 执行指令时使用 `InstrMeta.span` 创建带源码位置的错误
- 运行时错误（栈下溢、除零、无效 ASCII 等）现在都能精确定位到源码行号

---

## 二、语言设计层面

### 4. 函数不支持递归/嵌套调用

**文件:** `src/stardust/vm.rs`

`CallInsideFunction` 错误阻止了函数体内的函数调用。虽然这简化了实现（避免调用栈管理），但限制了语言表达能力。README 中未明确说明此限制。

**建议:**
- 短期：在 README 的"函数声明与调用"章节明确说明不支持嵌套/递归调用
- 长期：实现调用栈支持，允许嵌套调用（需注意防止无限递归，可考虑添加最大调用深度限制）

### 5. 算术运算无溢出检查

**文件:** `src/stardust/vm.rs`

`Add`/`Sub`/`Mul` 直接使用 `i64` 原生运算，无溢出保护：

```rust
Instruction::Add => {
    let b = self.pop_main()?;
    let a = self.pop_main()?;
    self.main_stack.push(a + b);  // 可能溢出
    self.pc += 1;
}
```

Debug 模式下会 panic，Release 模式下会静默环绕。

**建议:**
- 使用 `i64::checked_add`、`checked_sub`、`checked_mul` 等
- 溢出时返回新的 `ErrorKind::IntegerOverflow` 错误

### 6. 未定义符号的跳转仅在运行时才发现

**文件:** `src/stardust/parser.rs`, `src/stardust/vm.rs`

`Jump` 和 `UnconditionalJump` 引用的 Mark 如果不存在，只在执行到该跳转指令时才报错。对于无条件跳转，解析阶段至少可以检测非前向引用的情况。

**建议:**
- 在 `resolve_main_marks()` 之后再增加一个验证步骤，检查所有 `Jump`/`UnconditionalJump` 引用的 Mark 是否存在
- 对于前向引用（Mark 定义在 Jump 之后），可以保留运行时检查，但至少对明显未定义的引用给出解析错误

### 7. TokenType 中的保留关键字未实现

**文件:** `src/stardust/mod.rs`, `src/stardust/lexer.rs`, `src/stardust/parser.rs`

词法分析器识别了 `Hyphen`、`Equals`、`AngleLeft`、`AngleRight`、`Ampersand`、`Hash` 六种符号，但在语法分析中遇到这些符号时产生 `UnexpectedToken` 错误。README 称它们为"保留关键字"但未说明任何语义。

**建议:**
- 要么为这些符号实现对应语义（比较运算、逻辑运算、调试指令等）
- 要么从词法分析和 TokenType 中移除，让它们产生 `InvalidCharacter` 错误
- 至少应在 README 中说明这些是预留的未实现功能

---

## 三、代码质量

### 8. `StageResult` 枚举位置不当

**文件:** `src/stardust/mod.rs`

`StageResult` 枚举（含 `Source`、`UnwindSource`、`Tokens`、`Parsed`、`Error`、`None` 变体）定义在核心类型模块中，但实际上只在 `utils.rs` 的 dump 功能中使用。

**建议:**
- 将 `StageResult` 移到 `utils.rs`，或创建独立的 `dump.rs` 模块
- 保持 `mod.rs` 仅包含核心的公开类型定义

### 9. 错误类型不一致

**文件:** `src/main.rs`

`read_and_unwind()` 返回 `Result<(String, String), String>`，使用裸 `String` 作为错误类型：

```rust
fn read_and_unwind(filename: &str) -> Result<(String, String), String> {
    let source = fs::read_to_string(filename)
        .map_err(|e| format!("Error reading file '{}': {}", filename, e))?;

    let unwind_source = simple_preprocess(&source)
        .map(|uw| uw.into_owned())
        .map_err(|e| format!("Preprocess error: {}", e))?;

    Ok((source, unwind_source))
}
```

而项目已有完善的 `StardustError` 类型体系。

**建议:**
- 改为返回 `Result<(String, String), StardustError>`
- 在 `From<std::io::Error>` 的实现已有，可以直接使用 `?`

### 10. 命名不当：`bump` 应为 `dump`

**文件:** `src/stardust/utils.rs`

`bump_source`、`bump_run_source`、`bump_film` 中的 "bump" 应是 "dump" 的误译。

**建议:**
- 重命名为 `dump_source`、`dump_run_source`、`dump_file`
- 保持与 CLI 参数 `--dump` 一致的术语

### 11. `natural_source_code` 中不必要的克隆

**文件:** `src/stardust/utils.rs`

```rust
let copy_content = content.clone();      // 第61行 — 整个源文件内容的克隆
let copy_chars_vec = chars_vec.clone();  // 第106行 — 整个字符 Vec 的克隆
```

`chars_vec.clone()` 将整个字符向量复制一份，仅用于在循环中检测注释：

```rust
if is_anno(_char) && is_anno(copy_chars_vec[index].parse()?) {
    in_anno = true;
}
```

**建议:**
- 用一个 `bool` 标志追踪是否处于注释中，无需克隆整个向量
- `copy_content` 可以通过重构控制流消除

### 12. Lexer 中注释处理逻辑重复

**文件:** `src/stardust/lexer.rs`

注释处理逻辑（`//` 的检测和跳过）在 `next_token()` 中出现了两次：一次在空格后遇到 `/` 的分支，一次在无空格遇到 `/` 的分支。代码完全相同。

**建议:**
- 提取为 `skip_comment(&mut self)` 私有方法
- 两处调用该方法

---

## 四、测试覆盖

### 13. Extension 模块无测试

**文件:** `src/extension/char2sd_list.rs`, `src/extension/unwind.rs`

`preprocess()` 和 `simple_preprocess()` 是关键功能（特别是 `simple_preprocess` 在运行和 dump 模式中都会调用）但完全没有测试。

**建议:**
- 为 `simple_preprocess` 添加测试：ASCII 替换正确性、非法字符处理、边界情况
- 为 `preprocess` 添加测试：大写/小写/数字字符替换、注释保留、混合内容
- 可以复用 lexer 的 tokenize 来验证预处理输出是否能被正确词法分析

### 14. Lexer 中行号跟踪可能需要验证

**文件:** `src/stardust/lexer.rs`

测试 `test_multiple_newlines_ignored` 中：

```rust
let source = "+\n\n\n\n*";  // + 在第1行, 3个空行, * 在第4行
// ...
assert_eq!(tokens[1].line, 1);  // 期望 line 为 1？
assert_eq!(tokens[1].token_type, TokenType::Star);
```

第二个 token `*` 在第 4 行，但测试期望 `line == 1`。这可能是因为跳过空白行的逻辑中 `line` 没有正确更新。需要检查是否是实际 bug。

同样的问题存在于 `test_line_column_after_newline_with_whitespace` 和 `test_line_column_multi_line` 中。

**建议:**
- 审查 lexer 中行号跟踪逻辑，确认 line 在跳过空行后是否正确递增
- 如果测试反映了期望行为（lexer 不关心实际行号，只关心 token 的逻辑顺序），需要明确文档化

---

## 五、功能缺失

### 15. 没有 REPL 交互模式

对于 esolang 来说，交互式 REPL 非常有助于学习和实验。目前只能通过文件运行。

**建议:**
- 添加 `--repl` 或 `-r` 模式，使用 `rustyline` 等 crate
- 支持逐条输入指令并立即查看栈状态

### 16. 没有调试/单步执行功能

`main.rs` 顶部有 TODO 注释：

```rust
// TODO:转译为Rust/C代码，实现编译为可执行文件
// TODO:增加调试处理，单点执行
```

调试和单步执行尚未实现。

**建议:**
- 添加 `--debug` 模式，支持：
  - 单步执行（每次执行一条指令后暂停，等待用户输入）
  - 查看当前栈内容
  - 查看当前 PC 位置和下一条指令
  - 设置断点（基于 Mark 名称）

### 17. 无 CI/CD 配置

缺少自动化构建和测试流水线。

**建议:**
- 添加 GitHub Actions 配置（`.github/workflows/ci.yml`）
- 包括：`cargo build`、`cargo test`、`cargo clippy`、`cargo fmt --check`
- 添加 VSCode 扩展的 CI（`npm run compile`）

---

## 六、工程化

### 18. 缺少代码风格配置

项目使用 Rust 2024 edition 但没有格式化配置和 clippy 规则。

**建议:**
- 添加 `.rustfmt.toml` 统一代码风格
- 运行 `cargo clippy --fix` 修复现有警告
- 考虑添加 `Cargo.toml` 的 `[lints]` 部分启用额外 lint

### 19. VSCode 扩展中语法检查过于频繁

**文件:** `stardust-vscode/src/extension.ts`

`onDidChangeActiveTextEditor` 事件触发语法检查——每次切换编辑器标签页都会调用 CLI，可能过于频繁。

**建议:**
- 仅在保存时触发检查（移除 `onDidChangeActiveTextEditor` 监听）
- 或者添加防抖（debounce），避免短时间内多次触发

### 20. 未使用的依赖和 `edition = "2024"` 注意事项

**文件:** `Cargo.toml`

`serde` 和 `serde_json` 仅用于 CLI JSON 输出（`--check`/`--tokens` 模式）。库部分不需要这些依赖。可以考虑将它们作为可选 feature。

同时 `edition = "2024"` 是非常新的版本，部分场景可能有兼容性问题。

**建议:**
- 将 `serde`/`serde_json` 作为 optional feature，或确认它们确实在库代码中使用（`Serialize` derive 用于部分类型）
- 在 CI 中固定 Rust 工具链版本以确保可重现构建

---

## 七、优先级汇总

| 优先级 | 编号 | 改进项 |
|--------|------|--------|
| 🔴 高 | 1 | 补全 `char2sd_list.rs` 字符映射或统一使用 simple_preprocess |
| 🔴 高 | 2 | ✅ 重构 VM 消除重复代码 |
| 🔴 高 | 3 | ✅ 为 VM 运行时错误添加源码位置信息 |
| 🟡 中 | 5 | 添加算术溢出检查 |
| 🟡 中 | 6 | 解析阶段验证 Mark 引用 |
| 🟡 中 | 12 | 消除 Lexer 中注释处理的代码重复 |
| 🟡 中 | 13 | 为 extension 模块添加测试 |
| 🟡 中 | 9 | 统一错误类型（移除裸 String 错误） |
| 🟡 中 | 14 | 审查 Lexer 行号跟踪逻辑 |
| 🟢 低 | 4 | 文档化函数调用限制（或实现嵌套调用） |
| 🟢 低 | 7 | 实现或移除保留关键字 |
| 🟢 低 | 8 | 移动 StageResult 到正确位置 |
| 🟢 低 | 10 | 重命名 bump → dump |
| 🟢 低 | 11 | 消除不必要的 clone |
| 🟢 低 | 15 | 添加 REPL 模式 |
| 🟢 低 | 16 | 实现调试/单步执行 |
| 🟢 低 | 17 | 添加 CI/CD 配置 |
| 🟢 低 | 18 | 添加 rustfmt/clippy 配置 |
| 🟢 低 | 19 | 优化 VSCode 扩展检查频率 |
| 🟢 低 | 20 | 依赖和 edition 审查 |
