# REPL 交互环境

> **实现位置**：`src/repl/mod.rs` (426 行) · `src/repl/parser.rs` (317 行) · `src/repl/executor.rs` (210 行) · `src/repl/display.rs` (299 行)

## 1. 启动

```bash
stardust --repl              # 标准 REPL
stardust -r                  # 同上
stardust -r -d               # 调试 REPL
stardust -r -d program.sd    # 加载文件后进入调试 REPL
```

欢迎信息：
```
Stardust REPL 0.1.6 [debug mode]
Type Stardust instructions or :help for help.
─────────────────────────────────────────────────
sd>
```

## 2. 输入模式

REPL 支持两种输入模式，**自动检测**，可用 `:raw` 强制切换。

### 2.1 自动检测逻辑

```rust
fn detect_mode(input: &str) -> InputMode {
    match input.trim().chars().next() {
        Some('a'..='z') | Some('A'..='Z') | Some('"') => InputMode::Shorthand,
        _ => InputMode::Raw,
    }
}
```

| 首字符 | 判定模式 |
|--------|----------|
| 字母 a–z / A–Z | 简写模式 |
| `"` | 简写模式（字符串展开） |
| 空格、符号、数字等 | 原始模式 |

### 2.2 简写模式

使用人类可读的指令名，由 `parser::parse_shorthand()` 解析：

```
sd> Push(72) CharOut
H
sd> Push(3) Push(5) Add NumOut
8
sd> "Hello"
Hello
```

### 2.3 原始模式

直接输入空格+符号的 .sd 源格式，经过标准 tokenize → parse_program 流水线：

```
sd>      +,
H
sd> +*
```

（`+*` 产生两个 token，对应 Dup 和 Add 无法执行，但语法合法）

## 3. 简写指令参考

### 3.1 基础指令

| 简写 | 对应 | 说明 |
|------|------|------|
| `Push(n)` | n≥5 空格 + `+` | 压入整数 n |
| `Dup` | 1 空格 + `+` | 复制栈顶 |
| `Swap` | 2 空格 + `+` | 交换栈顶两个 |
| `Rotate` | 3 空格 + `+` | 旋转栈顶三个 |
| `Pop` | 4 空格 + `+` | 弹出栈顶 |

### 3.2 算术

| 简写 | 说明 |
|------|------|
| `Add` / `Sub` / `Mul` / `Div` / `Mod` / `Reverse` | 对应 0–5 空格 `*` |

### 3.3 I/O

| 简写 | 说明 |
|------|------|
| `NumOut` / `NumIn` | 数字输出/输入 |
| `CharOut` / `CharIn` | 字符输出/输入 |

### 3.4 控制流

| 简写 | 说明 |
|------|------|
| `Mark(n)` | 声明标记 |
| `Jump(n)` | 条件跳转 |
| `UncondJump(n)` | 无条件跳转 |
| `Call(n, m)` | 调用函数 n，传入 m 个参数 |

### 3.5 比较 & 逻辑

| 简写 | 说明 |
|------|------|
| `Eq` / `Ne` / `Lt` / `Gt` / `Le` / `Ge` | 比较运算 |
| `And` / `Or` / `Not` / `Xor` | 逻辑运算 |

### 3.6 堆 & 栈扩展 & 调试

| 简写 | 说明 |
|------|------|
| `Store` / `Load` | 堆操作 |
| `ShiftL` / `Depth` / `Pick` / `ShiftR` / `DropN` | 栈扩展 |
| `DumpStack` / `DumpState` / `Breakpoint` | 调试 |

### 3.7 字符串展开

`"Hello"` 自动展开为：
```
Push(72) CharOut Push(101) CharOut Push(108) CharOut Push(108) CharOut Push(111) CharOut
```

## 4. 元命令

### 4.1 通用命令

| 命令 | 别名 | 说明 |
|------|------|------|
| `:help` | `:h` | 显示帮助 |
| `:quit` | `:q` | 退出 REPL |
| `:stack` | `:s` | 查看当前栈（栈顶→底，最多 20 项） |
| `:heap` | `:hp` | 查看堆内容（按键排序，最多 30 条） |
| `:info` | `:i` | VM 状态（栈深度、堆条目、帧数、函数数） |
| `:clear` | `:c` | 清空栈和堆 |
| `:load <file>` | `:l` | 加载并执行 .sd 文件（状态持久化，函数定义合并） |
| `:func <n> <body>` | `:f` | 定义函数 n，body 为简写指令序列 |
| `:funcs` | — | 列出所有已定义函数 |
| `:raw` | — | 切换原始/简写输入模式 |
| `:history` | — | 显示命令历史 |

### 4.2 调试 REPL 附加命令

| 命令 | 说明 |
|------|------|
| `:step` / `:st` | 启用单步模式 |
| `:continue` / `:co` | 继续执行 |
| `:break <n>` / `:b` | 设置断点（文件调试模式下） |
| `:breaks` / `:lb` | 列出断点 |
| `:db` | 删除断点 |
| `:list` | 显示附近指令（文件调试模式下） |
| `:pc` | 显示当前 PC |
| `:bt` | 调用栈回溯 |

## 5. REPL 架构

### 5.1 核心组件

| 模块 | 文件 | 行数 | 职责 |
|------|------|------|------|
| 主循环 | `repl/mod.rs` | 426 | 输入循环、模式检测、元命令分发、文件加载 |
| 简写解析器 | `repl/parser.rs` | 317 | `parse_shorthand()` — 人类可读指令名 → Instruction |
| 执行上下文 | `repl/executor.rs` | 210 | `ReplContext` — 持久化栈/堆/函数表；状态注入/取回 |
| 状态显示 | `repl/display.rs` | 299 | 栈/堆/VM 信息/附近指令/调用帧的格式化输出 |

### 5.2 ReplContext

```rust
pub struct ReplContext {
    pub stack: Vec<i64>,                       // 持久化栈
    pub heap: HashMap<i64, i64>,               // 持久化堆
    pub functions: HashMap<usize, FunctionDef>, // 持久化函数表
}
```

### 5.3 状态注入机制

每次执行 REPL 行时：
1. 从 `ReplContext` 取出当前栈/堆/函数表
2. 创建临时 `ParseResult`（空主指令 + 用户输入的指令）
3. 创建临时 `VM`，通过 `set_main_stack()` / `set_heap()` 注入状态
4. VM 执行
5. 通过 `take_main_stack()` / `take_heap()` 取回状态
6. 更新 `ReplContext`

这确保了 REPL 的每条指令都从上次的状态继续。

### 5.4 文件加载

`:load file.sd` 执行完整流水线：
1. 读取并解析 .sd 文件
2. 将文件中的函数定义合并到 ReplContext（不覆盖同名函数）
3. 执行文件的主指令序列（在 REPL 的状态上下文中）

## 6. 提示符

栈非空时显示当前深度：
```
sd[3]>
```
表示栈中有 3 个元素。空栈时仅显示：
```
sd>
```

---

> 相关文件：[debugger.md](debugger.md) · [cli.md](cli.md)
