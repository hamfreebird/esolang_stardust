# Stardust Language Documentation

> **Version 0.1.6** — 完整语言规范、实现细节、错误类型与运行时行为

Stardust 是一种受 **Starry** 与 **Whitespace** 启发的深奥编程语言（esolang）。程序仅由**空格** ` ` 与 **16 种符号字符**构成，每条指令由 **前导空格数量 + 一个符号字符** 唯一定义。

## 核心特征

| 特征 | 描述 |
|------|------|
| **范式** | 基于栈的指令式编程 |
| **字符集** | 仅空格 (` `) 与 16 个 ASCII 符号 |
| **指令编码** | `spaces + symbol`，空格数量决定具体操作 |
| **执行模型** | 多栈帧调用架构，函数调用创建独立栈帧 |
| **类型系统** | 无类型，所有值为 `i64` 整数 |
| **内存模型** | 栈（每帧独立）+ 堆（全局共享，`i64 → i64` 映射） |
| **实现语言** | Rust 2024 Edition (MSRV 1.95) |
| **许可协议** | MIT |

## 与 Starry 的关键区别

1. **多栈帧调用**：函数调用时创建独立栈帧，返回时合并到主调帧（天然多值返回），而非 Starry 的单一全局栈
2. **LLVM 编译**：可将 Stardust 源码编译为 LLVM IR 再链接为可执行二进制
3. **运行时注解**：`#[key=value]` 语法配置 VM 参数（步数限制、栈/堆上限等）
4. **调试器**：内置交互式单步调试器，支持断点和状态检查
5. **错误定位**：所有运行时错误携带源码行列信息

## 文档索引

### 语言规范

| 文件 | 内容 |
|------|------|
| [language/lexer.md](language/lexer.md) | 词法规则 — 有效字符、Token 结构、状态机逻辑、边界行为 |
| [language/instructions.md](language/instructions.md) | 指令集完整参考 — 36 种指令的编码、栈变化、语义细节 |
| [language/annotations.md](language/annotations.md) | 运行时注解系统 — #[key=value] 语法与 6 个配置键 |

### 实现架构

| 文件 | 内容 |
|------|------|
| [implementation/architecture.md](implementation/architecture.md) | 总体架构 — 模块结构、类型系统、设计决策 |
| [implementation/vm.md](implementation/vm.md) | 虚拟机 — 多栈帧模型、执行循环、I/O 抽象、安全机制 |
| [implementation/pipeline.md](implementation/pipeline.md) | 编译流水线 — 从源码到执行/编译的完整数据流 |

### 工具与扩展

| 文件 | 内容 |
|------|------|
| [cli.md](cli.md) | CLI 运行模式 — 8 种模式的完整说明 |
| [repl.md](repl.md) | REPL 交互环境 — 双模式输入、简写语法、元命令 |
| [debugger.md](debugger.md) | 交互式调试器 — 12 种调试命令、断点机制 |
| [codegen.md](codegen.md) | LLVM 代码生成 — SSA 栈追踪、优化器、运行时内置函数 |
| [vscode-extension.md](vscode-extension.md) | VSCode 扩展 — 语法高亮、诊断、配置 |

### 参考

| 文件 | 内容 |
|------|------|
| [errors.md](errors.md) | 错误类型系统 — 40+ 种 ErrorKind 的完整参考 |
| [testing.md](testing.md) | 测试体系 — 425 个测试的覆盖范围 |
| [file-index.md](file-index.md) | 文件索引 — 所有源文件的位置与说明 |

---

> 本文档基于 Stardust v0.1.6 源码整理。
> 最后更新：2026-10-16
