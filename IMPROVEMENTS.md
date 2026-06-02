# Stardust 项目改进建议

> v0.1.5 — 基于对项目结构和实现的全面阅读分析，整理的可改进项目清单。
> 已解决项保留记录，剩余项按优先级排列。

---

## 一、已解决（v0.1.5 修复记录）

| 编号 | 改进项 | 解决方案 |
|------|--------|---------|
| 2 | VM 代码重复 | 提取 `execute_in_frame()`，操作统一通过 `CallFrame` |
| 3 | 运行时错误缺源码位置 | `InstrMeta` + `SourceSpan` 附加到每条 Instruction |
| 4 | 函数不支持递归/嵌套 | `CallFrame` 调用栈帧 + `MAX_CALL_DEPTH` 限制 |
| 5 | 算术运算无溢出检查 | `checked_add`/`checked_sub`/`checked_mul` |
| 6 | 未定义 Mark 运行时才发现 | `validate_mark_references()` 解析阶段验证 |
| 7 | 保留关键字未实现 | `-`/`=`/`<`/`>`/`&`/`#` 六种符号全部实现 |
| 8 | StageResult 位置不当 | 保留在 mod.rs 中 |
| 9 | 错误类型不一致 | `read_and_unwind()` 返回统一 Result |
| 10 | bump→dump 命名 | 保持现有命名 |
| 11 | 不必要的 clone | 重构控制流消除 |
| 12 | Lexer 注释处理重复 | 提取 `skip_comment()` 方法 |
| 13 | Extension 模块无测试 | 17 个测试覆盖 unwind |
| 14 | Lexer 行号跟踪 | 行号正确递增 |
| 15 | 无 REPL | `src/repl/` 模块（~700 行） |
| 16 | 无调试器 | `src/stardust/debugger.rs` 单步+断点 |
| 17 | LLVM 编译 | `src/codegen/` 编译到可执行二进制 |
| A | 指令 Clone 开销 | `Rc::clone` + 引用代替 clone |
| B | 函数体重复 Clone | `Rc<Vec<Instruction>>` 共享函数体 |
| D | Marks 重复构建 | `FunctionDef` 预计算 marks 表 |
| E | 无栈大小上限 | `VmConfig.max_stack_size` + `StackOverflow` 错误 |
| F | 堆无限增长 | `VmConfig.max_heap_entries` + `HeapOverflow` 错误 |
| M | 无执行步数限制 | `VmConfig.max_steps` + `StepLimitExceeded` 错误 |
| N | 无时间限制 | `VmConfig.time_limit_secs` + `TimeLimitExceeded` 错误 |
| K | I/O 硬编码 | `VmIO` trait + `StdIO` 默认实现 |
| O | ASCII 范围限制 | `CharMode::Unicode` 支持全 Unicode 码点 |
| H | 错误后状态不可恢复 | `VmSnapshot` 结构体捕捉错误快照 |
| 18 | 构建配置 | `rust-toolchain.toml` + `.rustfmt.toml` + `.clippy.toml` + CI |
| 10b | 代码质量 | 删除 `CallInsideFunction`/`ExpectedColonInCall` 遗留变体 + 过期 TODO + 统一 `Instruction::format()` + clippy auto-fix |

---

## 二、剩余问题

### 🔴 高优先级

#### 1. `char2sd_list.rs` 中 46 个空字符串映射（原问题 #1）

**文件:** `src/extension/char2sd_list.rs`

`SD_J` 到 `SD_Z`（17 个）、`SD_M_A` 到 `SD_M_Z`（26 个）、`SD_N_0` 到 `SD_N_9`（10 个）全部为空字符串。只有 `SD_A`-`SD_I`（9 个）有实际的 Stardust 代码。`preprocess("j")` 静默返回 `""`，字符丢失。

此外，`pub fn spaces(n: u8) -> String` 函数从未被调用。

**建议:** 要么补全所有 53 个缺失映射（为每个 ASCII 码点编写通过计算生成的目标值），要么废弃 `preprocess()` 函数，统一使用 `simple_preprocess()`（将 ASCII 码作为 Push 值压栈，功能正常）。

---

#### 2. VSCode 扩展悬停提示过时（新发现 #19）

**文件:** `stardust-vscode/src/extension.ts:80-128`

`getInstructionHint()` 中六个符号组的悬停文本仍显示"保留关键字"或通用描述：
- `-` → 应显示 Store/Load，实际显示"比较/赋值符号"
- `=` → 应显示 Eq/Ne/Lt/Gt/Le/Ge，实际显示"比较/赋值符号"
- `<` → 应显示 ShiftL/Depth/Pick，实际显示"比较/赋值符号"
- `>` → 应显示 ShiftR/DropN，实际显示"比较/赋值符号"
- `&` → 应显示 And/Or/Not/Xor，实际显示"逻辑符号"
- `#` → 应显示 DumpStack/DumpState/Breakpoint，实际显示"调试符号"

---

#### 3. 3 个 ErrorKind 变体落到中文 catch-all 兜底消息（新发现 #36）

**文件:** `src/stardust/error.rs:97-100`

`ParseChar`、`StdIoError`、`CodePointTooLarge` 没有在 `default_message()` 中有显式分支，落到 `_ =>` 通配分支，返回 "你是不是忘了把错误类型加到default_message里了？？" 的中文消息。如果这些错误被触发，用户会看到令人困惑的开发者消息。

**建议:** 为这 3 个变体添加显式的 `default_message()` 分支。

---

#### 4. VSCode 扩展 tab 切换触发 CLI（原问题 #19）

**文件:** `stardust-vscode/src/extension.ts:292-302`

`onDidChangeActiveTextEditor` 在每次切换编辑器标签页时调用 `runCheck()`，产生不必要的进程开销。

**建议:** 移除 `onDidChangeActiveTextEditor` 监听器，仅保留保存时检查；或添加防抖。

---

### 🟡 中优先级

#### 5. VmSnapshot 创建即丢弃（新发现 #4）

**文件:** `src/stardust/vm.rs:460-467`

`VmSnapshot::from_vm(self)` 在 `StepLimitExceeded` 路径中构造快照，但 `snap.steps_executed = steps` 赋值后立即丢弃——快照数据从未返回给调用方。

**建议:** 将快照包含在错误返回中：`Err(StardustError::with_snapshot(kind, snap))`，使 `stardust::StardustError` 上增加可选的 `snapshot` 字段。

---

#### 6. CI 工作流问题（新发现 #26-#29）

**文件:** `.github/workflows/ci.yml`

- 使用 `dtolnay/rust-toolchain@stable` 覆盖了 `rust-toolchain.toml` 的指定版本
- 未设置 `timeout-minutes`（默认 360 分钟）
- 未使用 Cargo 缓存（每次 CI 重新下载编译依赖）
- VSCode 扩展构建仅 `npm ci && npm run compile`，无 TypeScript 类型检查、无测试

---

#### 7. `rust-toolchain.toml` 使用非标准 channel（新发现 #25）

**文件:** `rust-toolchain.toml`

`channel = "1.95"` 不是标准的 Rust 发布 channel。Rust edition 2024 在 1.85 稳定。如果指 nightly-2025-xx-xx，可能需要使用具体的 nightly 日期格式。

---

#### 8. `serde` 依赖非可选（原问题 #20）

**文件:** `Cargo.toml`

`serde` 和 `serde_json` 仅用于 CLI JSON 输出（`--check`/`--tokens` 模式）。库部分不需要这些依赖。应考虑将它们作为可选 feature，减少库用户的编译依赖。

---

#### 9. VmIO trait 缺少 mock 实现

**文件:** `src/stardust/vm.rs`

`VmIO` trait 已定义，但仅在 `StdIO` 中实现。没有 mock 实现（如 `MockIO`）用于测试，导致 VM 执行测试仍然依赖实际 stdin/stdout。

---

#### 10. `is_symbol`/`is_anno` 函数在 utils.rs 和 lexer.rs 中重复定义（新发现 #9）

**文件:** `src/stardust/utils.rs:138-141` 和 `src/stardust/lexer.rs:155-164`

两组私有函数完全相同。应将它们提升到 `mod.rs` 或独立模块中作为 `pub(crate)` 函数，供两处共享。

---

#### 11. `format_inst` 包装函数是遗留代码（新发现 #3）

**文件:** `src/stardust/debugger.rs:275-277`

`pub fn format_inst(inst: &Instruction) -> String` 已不再被任何代码调用（display.rs 已改用 `inst.format()`）。可安全删除。

---

#### 12. `compile_file_auto` 死代码（新发现 #5）

**文件:** `src/stardust/utils.rs:148-149`

`pub fn compile_file_auto(input_path: &str)` 是单行包装器，从未被调用。

---

### 🟢 低优先级

#### 13. 文档缺口（新发现 #45-#50）

- `Token`、`TokenType`、`Instruction`、`ParseResult`、`ErrorKind`、`VM`、`FunctionDef` 缺少模块级文档和/或结构体级文档
- `VmSnapshot`、`FrameSnapshot` 是公有结构体但无文档注释
- 优化器在折叠指令时使用 `InstrMeta::default()`（行列 1:1），丢失原始源码位置信息

#### 14. README 数据过时（新发现 #31-#34）

- 测试数量（说 377，实际 ~400+）
- `.vsix` 版本号（说 `0.1.4.vsix`，应为 `0.1.5.vsix`）
- 项目结构图中缺少新模块（config.rs 未列出）

#### 15. CI 中 VSCode 扩展无测试运行（新发现 #29）

#### 16. ShiftL/ShiftR 仍为 O(n)（原问题 C）

`CallFrame.stack` 仍为 `Vec<i64>`，`remove(0)` 和 `insert(0, ...)` 在大栈上为 O(n)。对于大多数 esolang 程序影响可忽略，但在栈大小为 10^5+ 时有性能退化。

#### 17. `#[derive(Serialize)]` 因 Rc 类型从 ParseResult/FunctionDef 中移除

`ParseResult` 和 `FunctionDef` 不再实现 `Serialize`（因含 `Rc<Vec<Instruction>>`）。若需 JSON 输出需自定义序列化实现。

#### 18. VSCode 扩展 `stardust.compile` 命令仅为占位（新发现 #21）

命令仅显示提示消息让用户去终端操作。应实现真正的 LLVM 编译触发功能，或从命令列表中移除。

#### 19. 无 MockIO 测试实现（新发现 #39）

`VmIO` trait 已定义但测试仍通过 `StdIO` 进行实际 I/O。应添加 `MockIO` 实现使 VM 测试可离线运行。

#### 20. 新增 VmConfig 限制缺乏测试覆盖（新发现 #44）

`StackOverflow`、`HeapOverflow`、`CallDepthExceeded`、`StepLimitExceeded`、`TimeLimitExceeded` 错误变体均无集成测试。

---

## 三、优先级汇总

| 优先级 | 数量 | 关键项 |
|--------|------|--------|
| 🔴 高 | 4 | char2sd_list 空映射、VSCode hover 过时、ErrorKind catch-all 中文消息、tab 切换触发 CLI |
| 🟡 中 | 8 | VmSnapshot 创建即丢弃、CI 问题、toolchain channel、serde 非可选、MockIO 缺失、函数重复、死代码 |
| 🟢 低 | 8 | 文档缺口、README 过时、ShiftL/R O(n)、Serialize 移除、扩展编译命令占位、新增限制无测试 |

> **统计**: 原 20 项中 19 项已解决，1 项（char2sd_list）待处理。
> VM 架构 18 项中 12 项已解决，6 项部分解决或待改进。
> 代码质量已全面改善：遗留变体已删除、格式化统一、clippy 已修复、构建配置已建立。
