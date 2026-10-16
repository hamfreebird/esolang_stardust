# 文件索引

> 项目所有源文件的位置、行数和功能说明

## 核心语言实现 (`src/stardust/`)

| 文件 | 行数 | 说明 |
|------|------|------|
| `mod.rs` | 346 | **核心类型定义** — TokenType, Token, Instruction (39 变体), CallFrame, VM, ParseResult, FunctionDef, SourceSpan, InstrMeta, ErrorKind (41 变体), StardustError, StageResult. |
| `config.rs` | 262 | **运行时配置** — VmConfig 结构体 (max_steps/stack/heap/call_depth/time_limit/char_mode), CharMode 枚举 (Ascii/Unicode), parse_config() 注解提取, apply_config(), ParseConfigError. 12 个测试 |
| `lexer.rs` | 1138 | **词法分析器** — Lexer 结构体 (Peekable<Chars>, line/column/byte_pos 跟踪), advance(), skip_comment(), next_token() 状态机, tokenize() 入口. 辅助函数: is_symbol(), is_anno(), char_to_token_type(). 97 个测试 |
| `parser.rs` | 918 | **语法分析器** — Parser 结构体, parse() 主循环, parse_top_level(), parse_colon_at_top_level() (声明 vs 调用区分), parse_simple_instruction() (spaces→Instruction 映射), parse_instruction_in_function(), resolve_main_marks(), validate_mark_references() (编译期验证), validate_function_mark_references(), parse_program() 入口. 63 个测试 |
| `vm.rs` | 608 | **虚拟机** — VmIO trait (write_char/read_char/write_num/read_num), StdIO 实现, VmSnapshot/FrameSnapshot, pop_frame(), framed_err(), execute_in_frame() (核心执行函数，39 条指令，栈/堆越界检查), VM::new(), VM::run() 主循环, VM::run_with_io(), handle_call() (帧压入). |
| `error.rs` | 112 | **错误系统** — StardustError 构造器, ErrorKind::default_message() (35 变体消息 + catch-all), Display for SourceSpan, Display for StardustError |
| `utils.rs` | 382 | **工具函数** — generate_print_string() (字符串→Push+CharOut), compile_file_to_stardust() (文本→Stardust，所有可见 ASCII 含符号字符统一转 Push+CharOut), bump_source()/bump_run_source() (--dump 4 阶段分析), format_results(), print_error() (源码上下文错误输出), print_usage() (CLI 帮助), is_symbol()/is_anno() (重复定义) |
| `debugger.rs` | 318 | **交互式调试器** — Debugger 结构体 (breakpoints: HashSet, single_step: bool), DebugAction 枚举, should_break() (暂停判断), interact() (REPL 循环, 12 种命令), print_* 系列 (状态显示), inst_meta()/format_inst() (辅助) |

## LLVM 代码生成 (`src/codegen/`)

| 文件 | 行数 | 说明 |
|------|------|------|
| `mod.rs` | 220 | **公开 API** — CodeGenConfig, compile_to_ir(), compile_to_ir_file(), compile_to_object(), compile_to_exe(), check_toolchain() (llc/clang 检测) |
| `compiler.rs` | 1721 | **LLVM IR 编译器** — FuncCompiler 结构体, StackEntry (Const/Reg), compile_stack (编译期栈追踪), emit_label() (自动补齐块终结指令), push_ssa()/pop_ssa()/flush(), compile_instruction() (39 条指令→LLVM IR), 全局状态声明 (@stack/@sp/@heap/@frame_base_stack/@frame_depth), 函数编译, checked_add/sub/mul (带溢出检查), Div/Mod (零除检查), Reverse (malloc+memmove+循环) |
| `optimizer.rs` | 773 | **IR 优化器** — 三阶段不动点迭代: (1) 常量折叠 (所有算术/比较/逻辑), (2) 死代码消除 (Push+Pop, Dup+Pop), (3) 窥孔优化 (Push(0)+Add, Push(1)+Mul, Reverse+Reverse, ShiftL+ShiftR). 38 个测试，含 mark 索引重映射与不动点终止回归测试 |
| `intrinsics.rs` | 137 | **运行时内置函数** — INTRINSICS_IR: checked_add/sub/mul (使用 LLVM sadd.with.overflow 等), runtime_divzero, runtime_modzero, runtime_bad_ascii, runtime_stack_underflow, runtime_call_depth, runtime_heap_oob. EXTERNAL_DECLS: printf/putchar/getchar/scanf/fprintf/malloc/free/exit |

## REPL (`src/repl/`)

| 文件 | 行数 | 说明 |
|------|------|------|
| `mod.rs` | 394 | **REPL 主循环** — repl_main() 入口, detect_mode() (简写/原始自动检测), parse_input() (路由到简写或原始解析器), handle_meta() (14 种元命令 + 6 种调试命令), load_and_execute_file(), show_stack_status(). 3 个测试 |
| `parser.rs` | 309 | **简写语法解析器** — parse_shorthand() (指令名→Instruction 映射, 30+ 指令), expand_strings() (字符串→Push+CharOut 序列). 14 个测试 |
| `executor.rs` | 189 | **执行上下文** — ReplContext 结构体 (stack/heap/functions 持久化), execute_repl() (状态注入→临时 VM 执行→状态取回). 8 个测试 |
| `display.rs` | 275 | **状态可视化** — format_stack() (栈顶→底, 截断), format_heap() (按键排序, 截断), format_vm_info(), format_nearby() (PC 附近指令), format_frames() (调用帧回溯), repl_help() (帮助文本). 14 个测试 |

## 扩展 (`src/extension/`)

| 文件 | 行数 | 说明 |
|------|------|------|
| `mod.rs` | — | 模块入口 (pub mod unwind; pub mod char2sd_list) |
| `unwind.rs` | 312 | **预处理/字符替换** — preprocess() (A-Z/a-z/0-9→Stardust 常量), simple_preprocess() (ASCII 码+5 空格+'+' 方式，注释内容原样保留). Cow<str> 返回 (零分配). 20 个测试 |
| `char2sd_list.rs` | 67 | **字符映射表** — 62 个常量 (SD_A..SD_Z, SD_M_A..SD_M_Z, SD_N_0..SD_N_9). 仅有 9 个 (SD_A..SD_I) 有实际映射, 其余为空. spaces() 定义但未使用 |

## CLI 入口

| 文件 | 行数 | 说明 |
|------|------|------|
| `src/main.rs` | 419 | **CLI 入口** — main()→stardust(), 标志提取 (--repl/-r, --debug/-d, --help/-h), 8 种模式分发: cmd_run() (解释执行, 完整流水线), cmd_check() (JSON 诊断), cmd_tokens() (JSON Token 流), cmd_compile() (LLVM IR), cmd_build() (二进制), cmd_stardust() (文本转换), cmd_dump() (4 阶段分析), read_and_unwind(), parse_input_file(), print_json_diagnostics(), print_cli_usage() |
| `src/lib.rs` | — | **库入口** — 声明所有公开模块 |

## VSCode 扩展

| 文件 | 说明 |
|------|------|
| `stardust-vscode/package.json` | 扩展清单 (名称/版本/激活事件/配置项/命令/快捷键) |
| `stardust-vscode/src/extension.ts` | 扩展逻辑 (activate, 悬停提示, 诊断, 命令注册) |
| `stardust-vscode/syntaxes/sd.tmLanguage.json` | TextMate 语法高亮定义 |
| `stardust-vscode/language-configuration.json` | 语言配置 (注释: //, 无括号匹配) |
| `stardust-vscode/tsconfig.json` | TypeScript 编译配置 |
| `stardust-vscode/out/extension.js` | 编译后的扩展 JS |

## 测试与配置

| 文件 | 行数 | 说明 |
|------|------|------|
| `tests/integration_test.rs` | 1373 | **集成测试** — 109 个端到端测试 (完整流水线: tokenize→parse→VM run). 21 个分类模块: 空程序、Push/栈、算术、I/O、控制流、函数、Hello World、错误处理(12 种)、解析错误、比较、逻辑、堆、栈扩展、调试、嵌套调用、溢出、Mark 验证. + 18 个 codegen_tests (LLVM 编译) |
| `Cargo.toml` | — | Rust 项目配置 (v0.1.6, edition 2024, rust-version 1.95, deps: serde+serde_json, lints: forbid unsafe) |
| `Cargo.lock` | — | 依赖锁定 |
| `build.rs` | — | Windows 构建脚本 (图标: nant.png) |
| `rust-toolchain.toml` | — | channel "1.95", components: rustfmt, clippy |
| `.rustfmt.toml` | — | edition 2024, max_width 100, tab_spaces 4, Unix newlines |
| `.clippy.toml` | — | allow-unwrap-in-tests = true |
| `.cargo/config.toml` | — | 交叉编译链接器 (Windows GNU, ARM64) |
| `.gitignore` | — | Git 忽略规则 |

## 示例与文档

| 文件 | 说明 |
|------|------|
| `hello_world.sd` | Hello World 示例 (5 行) |
| `hello_world.ll` | Hello World 编译后的 LLVM IR |
| `hello_world_bin` | Hello World 编译后的二进制 |
| `readme.md` | 项目 README (中文) |
| `LICENSE` | MIT 许可证 |

## 预构建二进制 (`build/`)

| 文件 | 说明 |
|------|------|
| `stardust-linux-x86_64` | Linux x86_64 二进制 |
| `stardust-linux-aarch64` | Linux ARM64 二进制 |
| `stardust-windows-x86_64.exe` | Windows x86_64 二进制 |
| `stardust-0.1.6.vsix` | VSCode 扩展包 |

## CI/CD

| 文件 | 说明 |
|------|------|
| `.github/workflows/ci.yml` | GitHub Actions CI 流水线 (构建 + 测试) |

---

> 项目总计约 **8,900 行 Rust 源码** (不含测试和配置)，**425 个测试** (316 单元 + 109 集成)。
