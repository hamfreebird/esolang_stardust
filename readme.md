# Stardust Language

> **v0.1.4** — 一种受 Starry 启发的 esolang，程序仅由空格与少量符号构成，指令语义由前导空格数量决定。

---

## 概述

Stardust 是一种受 Starry 与 Whitespace 启发的深奥编程语言（esolang）。程序仅由**空格** ` ` 与 **16 种符号字符**构成，每条指令由 **前导空格数量 + 一个符号字符** 唯一定义。

与 Starry 不同，Stardust 采用**多栈调用帧架构**：主程序运行在主栈，函数调用时创建独立新栈帧执行，结束后将栈内容整体合并回主栈，天然支持多值返回。

本解释器使用 **Rust** 编写，配套 **VSCode 扩展**提供语法高亮、悬停提示和实时诊断。支持**解释执行**、**LLVM 编译为二进制**、**交互式 REPL** 和**单步调试器**。

---

## 快速开始

### Hello World!

```text
            +               +  *       +* +,
         +            +  *      +** +,            +* +, +,
        +* +,         +             +  * +,        +  *
              + * +,    + +,        +* +,           + * +,
             + *,        +               +  *        +*,
```

### 安装与运行

```bash
# 从源码构建
cargo build --release

# 运行程序
./target/release/esolang_stardust hello.sd

# 或安装后直接使用
stardust hello.sd
```

---

## CLI 命令参考

```
stardust <file.sd>                      运行 Stardust 程序
stardust --debug <file.sd>              以调试模式运行（单步执行、断点）
stardust -d <file.sd>                   同上

stardust --repl                          启动交互式 REPL
stardust -r                              同上
stardust -r -d [file.sd]                启动调试 REPL

stardust --check <file.sd>              语法检查，输出 JSON 诊断信息
stardust --tokens <file.sd>             输出 Token 流（JSON 格式）

stardust --compile <file.sd> [out.ll]   编译为 LLVM IR 文本
stardust -c <file.sd>                   同上
stardust --build <file.sd> [out]        编译为可执行二进制文件
stardust -b <file.sd>                   同上

stardust --stardust <input.txt> [out]   将文本文件转换为 Stardust 源码
stardust -s <input.txt> [out]           同上
stardust --dump <file.sd> [out]         分析 Stardust 源码（4 阶段流水线）

stardust --help                          显示此帮助
```

---

## 运行模式详解

### 模式一：解释执行（默认）

直接运行 `.stardust` 或 `.sd` 源文件，依次进行预处理、词法分析、语法解析和 VM 执行。

```bash
stardust program.sd
```

### 模式二：交互式 REPL

启动交互式 Stardust 环境，支持**简写语法**和**原始语法**自动检测。

```bash
stardust --repl
```

```
sd> Push(72) CharOut       # 简写语法
H
sd> Push(3) Push(5) Add    # 运算
sd> NumOut
8
sd> :stack                  # 查看栈
Stack (top→bottom): [8]
sd> "Hi"                    # 字符串自动展开
Hi
sd> :quit
```

**简写指令**：`Push(n)` `Dup` `Swap` `Add` `Sub` `Mul` `Div` `Mod` `CharOut` `NumOut` `Eq` `Lt` `And` `Or` `Not` `Mark(n)` `Jump(n)` `Call(n, m)` 等 30+ 种。

**原始语法**：直接输入空格+符号的 .sd 格式（自动检测）。

**REPL 元命令**：

| 命令 | 说明 |
|------|------|
| `:h`, `:help` | 帮助 |
| `:q`, `:quit` | 退出 |
| `:s`, `:stack` | 查看栈 |
| `:hp`, `:heap` | 查看堆 |
| `:i`, `:info` | VM 状态 |
| `:c`, `:clear` | 清空栈和堆 |
| `:l <file>`, `:load` | 加载并执行 .sd 文件 |
| `:f <n> <body>`, `:func` | 定义函数 |
| `:funcs` | 列出已定义函数 |
| `:raw` | 切换原始/简写输入模式 |
| `:history` | 命令历史 |

### 模式三：调试器

加载 .sd 文件并进入交互式调试器，支持单步执行、断点管理和状态检查。

```bash
stardust --debug program.sd
```

```
── Stardust Debugger ──────────────────────────────
  PC: 0  Frame: main  Stack: 0  Heap: 0
  Next: Push(7)  [line 1, col 1]
───────────────────────────────────────────────────
(sd-dbg) s           # 单步执行
(sd-dbg) b 0         # 在 Mark(0) 设断点
(sd-dbg) c           # 继续到断点
(sd-dbg) p           # 查看栈
(sd-dbg) l           # 查看附近指令
(sd-dbg) q           # 退出
```

**调试器命令**：

| 命令 | 说明 |
|------|------|
| `s`, `step` | 单步执行一条指令 |
| `c`, `continue` | 继续执行到下个断点 |
| `b <n>`, `break` | 在 Mark(n) 设断点 |
| `lb` | 列出所有断点 |
| `db <n>` | 删除断点 |
| `p`, `stack` | 查看栈 |
| `hp`, `heap` | 查看堆 |
| `i`, `info` | VM 状态 |
| `l`, `list` | PC 附近指令 |
| `pc` | 当前 PC 和指令 |
| `bt`, `frames` | 调用栈回溯 |
| `q`, `quit` | 终止 |

### 模式四：LLVM 编译

将 Stardust 源码编译为 LLVM IR 或可直接运行的二进制文件。

```bash
# 编译为 LLVM IR（可读文本）
stardust --compile hello.sd
# → hello.ll

# 一键编译为可执行文件
stardust --build hello.sd
# → hello (可执行文件)

# 运行
./hello
# Hello World!
```

编译器实现**编译期栈追踪**：基本块内将栈操作映射到 SSA 寄存器（零栈内存访问），控制流边界才 flush 到内存。包含**三阶段 Stardust IR 优化器**（常量折叠、死代码消除、窥孔优化）。

需要系统安装 `clang`（`llc` + `clang`）。

### 模式五：文本 → Stardust 源码

将可见 ASCII 文本转换为 Stardust 代码。

```bash
stardust --stardust message.txt
# → message.stardust
```

### 模式六：语法检查（JSON 诊断）

输出 JSON 格式的诊断信息，供 VSCode 扩展使用。

```bash
stardust --check code.sd
# {"status":"ok","diagnostics":[]}
```

---

## 语言参考

### 词法规则

- **空格** ` `：前导空格数量决定操作类型或标识符名称
- **指令符号**：`+` `*` `` ` `` `'` `:` `;` `.` `,` `-` `=` `<` `>` `&` `~` `#`
- **注释**：`//` 到行尾
- 换行、制表符：被解释器忽略
- 其他字符：词法错误

### 指令集速查

#### 栈操作（`+`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| n ≥ 5 | `Push(n-5)` | 压入整数 n-5 |
| 1 | `Dup` | 复制栈顶 |
| 2 | `Swap` | 交换栈顶两个元素 |
| 3 | `Rotate` | 旋转栈顶三个元素 a,b,c → c,a,b |
| 4 | `Pop` | 弹出栈顶并丢弃 |

#### 算术运算（`*`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| 0 | `Add` | a + b |
| 1 | `Sub` | a - b |
| 2 | `Mul` | a × b |
| 3 | `Div` | a ÷ b（整数） |
| 4 | `Mod` | a % b |
| 5 | `Reverse` | 反转整个栈 |

#### 输入输出（`.` `,`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| `.` 0 | `NumOut` | 弹出整数，十进制输出 |
| `.` 1 | `NumIn` | 读取整数压栈 |
| `,` 0 | `CharOut` | 弹出整数，ASCII 字符输出 |
| `,` 1 | `CharIn` | 读取字符压栈 |

#### 控制流（`` ` `` `'` `~`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| n | `Mark(n)` | 声明跳转标记 |
| n | `Jump(n)` | 弹出栈顶，非零则跳转到 Mark(n) |
| n | `UncondJump(n)` | 无条件跳转到 Mark(n) |

#### 函数（`:` `;`）

| 语法 | 说明 |
|------|------|
| `(n): <body> (n):` | 声明函数 n |
| `(n1): (n2);` | 调用函数 n1，传入 n2 个参数 |

支持嵌套/递归调用，最大深度 256 层，天然支持多值返回。

#### 比较运算（`=`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| 0 | `Eq` | a == b |
| 1 | `Ne` | a != b |
| 2 | `Lt` | a < b |
| 3 | `Gt` | a > b |
| 4 | `Le` | a <= b |
| 5 | `Ge` | a >= b |

#### 逻辑运算（`&`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| 0 | `And` | a && b |
| 1 | `Or` | a \|\| b |
| 2 | `Not` | !a |
| 3 | `Xor` | a xor b |

#### 堆操作（`-`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| 0 | `Store` | heap[addr] = val |
| 1 | `Load` | push heap[addr] |

#### 栈扩展（`<` `>`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| `<` 0 | `ShiftL` | 循环左移 |
| `<` 1 | `Depth` | 压入栈深度 |
| `<` 2 | `Pick` | 复制栈中第 n 个元素 |
| `>` 0 | `ShiftR` | 循环右移 |
| `>` 1 | `DropN` | 丢弃栈顶 n 个元素 |

#### 调试（`#`）

| 空格数 | 指令 | 说明 |
|--------|------|------|
| 0 | `DumpStack` | 栈内容 → stderr |
| 1 | `DumpState` | VM 状态 → stderr |
| 2 | `Breakpoint` | 调试断点（调试模式下暂停） |

---

## VSCode 扩展

安装 `stardust-vscode/stardust-0.1.4.vsix` 获得：

- **语法高亮**：`.sd` / `.stardust` 文件的符号着色
- **悬停提示**：鼠标悬停显示指令语义和栈变化
- **实时诊断**：保存时自动运行 `--check`，错误标注到行
- **一键运行**：`F5` 在集成终端中运行当前文件
- **快捷键**：`Ctrl+Shift+C` 语法检查

配置项：
- `stardust.cliPath`：CLI 二进制路径（默认 `stardust`）
- `stardust.runInTerminal`：在集成终端中运行（默认 `true`）
- `stardust.checkOnSave`：保存时自动检查（默认 `true`）

---

## 项目结构

```
esolang_stardust/
├── src/
│   ├── main.rs                     # CLI 入口（8 种运行模式）
│   ├── lib.rs                      # 库入口
│   ├── stardust/
│   │   ├── mod.rs                  # 核心类型、VM 结构体
│   │   ├── lexer.rs                # 词法分析器
│   │   ├── parser.rs               # 语法分析器
│   │   ├── vm.rs                   # 栈式虚拟机
│   │   ├── error.rs                # 错误类型系统
│   │   ├── utils.rs                # 工具函数
│   │   └── debugger.rs             # 交互式调试器
│   ├── codegen/
│   │   ├── mod.rs                  # LLVM 代码生成入口
│   │   ├── compiler.rs             # Stardust → LLVM IR
│   │   ├── optimizer.rs            # IR 优化器
│   │   └── intrinsics.rs           # 运行时内建函数
│   ├── repl/
│   │   ├── mod.rs                  # REPL 主循环
│   │   ├── parser.rs               # 简写语法解析器
│   │   ├── executor.rs             # 执行上下文
│   │   └── display.rs              # 状态可视化
│   └── extension/
│       ├── char2sd_list.rs         # ASCII 字符映射
│       └── unwind.rs               # 预处理/字符替换
├── stardust-vscode/                # VSCode 扩展
├── tests/
│   └── integration_test.rs         # 集成测试（99 个）
├── hello_world.sd                  # Hello World 示例
└── Cargo.toml
```

---

## 技术细节

- **语言**：Rust 2024 Edition
- **依赖**：`serde` + `serde_json`（JSON 序列化）
- **测试**：377 个测试，覆盖词法/语法/VM/优化器/编译器/REPL/调试器
- **错误处理**：30+ 错误类型，均带源码行列定位
- **编译器**：零额外 Rust 依赖，生成 LLVM IR 文本 → `clang` 编译为二进制
- **调试器**：基于 `Option::take()` 模式解除 `&mut debug` / `&VM` 借用冲突

---

## License

MIT

## Author

freebird <freebirdflyinthesky@outlook.com>
