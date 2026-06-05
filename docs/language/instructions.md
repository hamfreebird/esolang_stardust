# 指令集完整参考

> **实现位置**：`src/stardust/mod.rs` (Instruction 枚举, 359 行) · `src/stardust/parser.rs` (解析逻辑, 890 行) · `src/stardust/vm.rs` (执行逻辑, 614 行)

## 1. 指令数据结构

```rust
pub enum Instruction {
    // ── 栈操作 (符号 +) ──
    Push(i64, InstrMeta),
    Dup(InstrMeta),
    Swap(InstrMeta),
    Rotate(InstrMeta),
    Pop(InstrMeta),

    // ── 算术运算 (符号 *) ──
    Add(InstrMeta),  Sub(InstrMeta),  Mul(InstrMeta),
    Div(InstrMeta),  Mod(InstrMeta),  Reverse(InstrMeta),

    // ── I/O (符号 . ,) ──
    NumOut(InstrMeta),  NumIn(InstrMeta),
    CharOut(InstrMeta), CharIn(InstrMeta),

    // ── 控制流 (符号 ` ' ~) ──
    Mark  { name: usize, meta: InstrMeta },
    Jump  { name: usize, meta: InstrMeta },
    UnconditionalJump { name: usize, meta: InstrMeta },

    // ── 函数 (符号 : ;) ──
    Call  { name: usize, argc: usize, meta: InstrMeta },

    // ── 比较运算 (符号 =) ──
    Eq(InstrMeta),  Ne(InstrMeta),  Lt(InstrMeta),
    Gt(InstrMeta),  Le(InstrMeta),  Ge(InstrMeta),

    // ── 逻辑运算 (符号 &) ──
    And(InstrMeta),  Or(InstrMeta),  Not(InstrMeta),  Xor(InstrMeta),

    // ── 堆操作 (符号 -) ──
    Store(InstrMeta),  Load(InstrMeta),

    // ── 栈扩展 (符号 < >) ──
    ShiftL(InstrMeta),  Depth(InstrMeta),  Pick(InstrMeta),
    ShiftR(InstrMeta),  DropN(InstrMeta),

    // ── 调试 (符号 #) ──
    DumpStack(InstrMeta),  DumpState(InstrMeta),  Breakpoint(InstrMeta),
}
```

每条指令附带的 `InstrMeta` 包含源码位置：
```rust
pub struct InstrMeta { pub span: SourceSpan }
pub struct SourceSpan { pub line: usize, pub column: usize }
```

## 2. 指令编码总表

### 2.1 栈操作 — 符号 `+`

| spaces | 指令 | 编码公式 | 栈变化 | VM 行为 |
|--------|------|----------|--------|---------|
| 0 | 错误 | — | — | `InvalidSpacesForPlus`（解析阶段报错） |
| 1 | `Dup` | — | `[a] → [a, a]` | 取栈顶值，再压入一份副本；空栈时报 StackUnderflow |
| 2 | `Swap` | — | `[a, b] → [b, a]` | 弹出两个值，交换顺序压回；不足时报 StackUnderflow |
| 3 | `Rotate` | — | `[a, b, c] → [c, a, b]` | 弹出三个值，按 c,a,b 顺序压回；不足时报 StackUnderflow |
| 4 | `Pop` | — | `[a] → []` | 弹出栈顶并丢弃；空栈时报 StackUnderflow |
| n ≥ 5 | `Push(n-5)` | `value = spaces - 5` | `[] → [value]` | 压入整数；压入前检查 max_stack_size，超限报 StackOverflow |

**Push 值范围**：理论上可达 `usize::MAX - 5`，实际受 max_stack_size 限制。

### 2.2 算术运算 — 符号 `*`

| spaces | 指令 | 栈变化 | VM 安全检查 |
|--------|------|--------|------------|
| 0 | `Add` | `[a, b] → [a+b]` | checked_add — 溢出时 IntegerOverflow |
| 1 | `Sub` | `[a, b] → [a-b]` | checked_sub — 溢出时 IntegerOverflow |
| 2 | `Mul` | `[a, b] → [a*b]` | checked_mul — 溢出时 IntegerOverflow |
| 3 | `Div` | `[a, b] → [a/b]` | 除数为 0 时 DivisionByZero；整数向零截断 |
| 4 | `Mod` | `[a, b] → [a%b]` | 除数为 0 时 ModuloByZero；（Rust 默认行为，结果符号 = 被除数符号） |
| 5 | `Reverse` | `[a₁..aₙ] → [aₙ..a₁]` | 空栈无操作（不报错）；O(n) 原地反转 |

**算术溢出**：使用 Rust 的 `checked_add`/`checked_sub`/`checked_mul`，溢出时返回 `IntegerOverflow` 错误。

**除法语义**：整数除法，向零截断（Rust 默认行为，与 C 一致）。

### 2.3 I/O — 符号 `.` `,`

| 编码 | 指令 | 栈变化 | I/O 细节 |
|------|------|--------|----------|
| `.` 0 | `NumOut` | `[val] → []` | 弹出 i64，十进制输出到 stdout（通过 `write_num()`） |
| `.` 1 | `NumIn` | `[] → [val]` | 读取一行 stdin，解析为 i64 压栈；解析失败报 InvalidIntegerInput |
| `,` 0 | `CharOut` | `[val] → []` | 弹出 i64，根据 char_mode 输出：Ascii 模式检查 0–127，Unicode 模式检查有效码点 |
| `,` 1 | `CharIn` | `[] → [byte]` | 从 stdin 读取 1 字节，以 i64 压栈（值范围 0–255） |

**CharOut 模式详解**（由 `#[char_mode]` 注解控制）：

- **Ascii 模式**（默认）：`val` 必须在 0–127 范围内，否则 `InvalidAscii { value }` 错误
- **Unicode 模式**：`val` 作为 `u32` 通过 `char::from_u32()` 验证，无效码点报 `InvalidCodePoint { value }`

**NumIn 行为**：调用 `io::stdin().read_line()` 读取整行，`.trim().parse::<i64>()` 解析。空白行或非数字输入导致 `InvalidIntegerInput`。

### 2.4 控制流 — 符号 `` ` `` `'` `~`

| 编码 | 指令 | 行为 |
|------|------|------|
| `` ` `` n | `Mark(n)` | 声明名称 n 的跳转标记；执行时 PC+1（无操作） |
| `'` n | `Jump(n)` | 弹出栈顶值：若 ≠ 0 则 PC 跳转到 Mark(n) 的指令索引；若 = 0 则 PC+1 |
| `~` n | `UncondJump(n)` | PC 无条件跳转到 Mark(n) 的指令索引 |

**解析阶段验证**：

- 所有 Mark 引用（Jump 和 UncondJump 的 `name`）在解析结束时验证
- 引用不存在的 Mark → 编译期 `UndefinedMark { name }` 错误（非运行时错误）
- 同一作用域内重复 Mark → `DuplicateMark { name }` 错误
- 验证覆盖主程序和每个函数体内部

**Marks 存储**：`HashMap<usize, usize>`，键为 Mark 名称（数字），值为对应指令在指令数组中的索引。在解析时预计算，VM 执行时直接查表。

### 2.5 函数 — 符号 `:` `;`

| 语法模式 | 语义 |
|----------|------|
| `(n): <body> (n):` | 声明函数 n，其中 n = 第一个 Colon 的前导空格数 |
| `(n1): (n2);` | 调用函数 n1，传入 n2 个栈顶参数 |

**声明规则**：

- 函数体的开始和结束标记必须是**具有相同 spaces 数的 Colon**（作为配对标识）
- 函数体内不允许嵌套函数声明，但允许函数调用
- 函数体指令在解析时收集，存入 `functions: HashMap<usize, Vec<Instruction>>`
- 解析后包装为 `FunctionDef { body: Rc<Vec<Instruction>>, marks: HashMap }`

**调用规则**：

- Parser 通过检查 Colon 后紧跟的 token 是否为 Semicolon 来区分声明和调用
- `n1` = 第一个 Colon 的 spaces（函数名）
- `n2` = Semicolon 的 spaces（参数个数 argc）
- 调用时从当前帧栈顶依次弹出 `argc` 个值（栈顶为最后一个参数）
- 若当前栈元素数 < argc → `NotEnoughArguments { func, expected, actual }`

**返回语义**：

- 函数帧执行完毕时（PC 到达帧指令末尾）
- 帧的**整个栈**通过 `parent.stack.extend(finished.stack)` 合并回父帧
- 父帧 PC 设为 `finished.ret_pc`（Call 指令的下一条指令）
- 天然多值返回：函数栈内有多少元素就返回多少值

**共享机制**：

- 函数体以 `Rc<Vec<Instruction>>` 存储，每次调用仅增加引用计数
- Marks 在 `FunctionDef` 中预计算（名称→索引映射），调用时直接 clone HashMap

### 2.6 比较运算 — 符号 `=`

| spaces | 指令 | 栈变化 | 说明 |
|--------|------|--------|------|
| 0 | `Eq` | `[a, b] → [result]` | result = 1 if a == b else 0 |
| 1 | `Ne` | `[a, b] → [result]` | result = 1 if a != b else 0 |
| 2 | `Lt` | `[a, b] → [result]` | result = 1 if a < b else 0 |
| 3 | `Gt` | `[a, b] → [result]` | result = 1 if a > b else 0 |
| 4 | `Le` | `[a, b] → [result]` | result = 1 if a <= b else 0 |
| 5 | `Ge` | `[a, b] → [result]` | result = 1 if a >= b else 0 |

所有比较结果为 0 或 1。

### 2.7 逻辑运算 — 符号 `&`

| spaces | 指令 | 栈变化 | 说明 |
|--------|------|--------|------|
| 0 | `And` | `[a, b] → [result]` | a ≠ 0 且 b ≠ 0 则 1，否则 0 |
| 1 | `Or` | `[a, b] → [result]` | a ≠ 0 或 b ≠ 0 则 1，否则 0 |
| 2 | `Not` | `[a] → [result]` | a = 0 则 1，否则 0（仅弹出 1 个值） |
| 3 | `Xor` | `[a, b] → [result]` | (a ≠ 0) xor (b ≠ 0) 则 1，否则 0 |

**注意**：逻辑运算以 "非零即真" 为语义，但结果始终归约为 0 或 1。这与 C 语言的逻辑运算符行为一致。

### 2.8 堆操作 — 符号 `-`

| spaces | 指令 | 栈变化 | 行为 |
|--------|------|--------|------|
| 0 | `Store` | `[addr, val] → []` | 弹出 addr 和 val；heap[addr] = val |
| 1 | `Load` | `[addr] → [val]` | 弹出 addr；压入 heap[addr]（不存在返回 0） |

**堆上限检查**（仅 Store 时）：

- 若 `heap.len() >= max_heap_entries` **且** `addr` 不在 heap 中已存在 → `HeapOverflow`
- 更新已存在的键不触发检查（不增加条目数）
- 该检查每个 Store 指令执行一次

### 2.9 栈扩展 — 符号 `<` `>`

| 编码 | 指令 | 栈变化 | 错误条件 |
|------|------|--------|----------|
| `<` 0 | `ShiftL` | `[a₁..aₙ] → [a₂..aₙ, a₁]` | 空栈 → StackUnderflow |
| `<` 1 | `Depth` | `[] → [n]` | Push 前检查栈上限 |
| `<` 2 | `Pick` | `[a₁..aₙ, n] → [a₁..aₙ, a_{n-n}]` | n≥stack.len() → StackUnderflow |
| `>` 0 | `ShiftR` | `[a₁..aₙ] → [aₙ, a₁..aₙ₋₁]` | 空栈 → StackUnderflow |
| `>` 1 | `DropN` | `[a₁..aₙ, n] → [a₁..a_{n-n}]` | n>stack.len() → StackUnderflow |

**索引处理**：

- `Pick` 的 n 被截断为 `max(0, n)`（负值视为 0）
- `DropN` 的 n 被截断为 `max(0, n)`
- `Pick` 索引 `n` 的含义："复制栈顶往下第 n 个元素"（n=0 即栈顶本身）

### 2.10 调试 — 符号 `#`

| spaces | 指令 | 行为 |
|--------|------|------|
| 0 | `DumpStack` | 向 stderr 输出 `[DEBUG] Stack: [...]`（当前帧） |
| 1 | `DumpState` | 向 stderr 输出 `[DEBUG] PC=X, StackDepth=Y, HeapSize=Z` |
| 2 | `Breakpoint` | 调试模式下触发暂停；非调试模式下为无操作 |

## 3. 指令的 Display 格式

`Instruction::format()` 方法返回人类可读的指令描述：

| 指令 | 格式示例 |
|------|----------|
| `Push(72, _)` | `"Push(72)"` |
| `Dup(_)` | `"Dup"` |
| `Mark { name: 3, .. }` | `"Mark(3)"` |
| `Jump { name: 5, .. }` | `"Jump(5)"` |
| `Call { name: 1, argc: 2, .. }` | `"Call(func=1, argc=2)"` |

用于调试器 `list` 命令和 REPL 状态显示。

## 4. InstrMeta 元数据提取

`inst_meta()` 函数（`src/stardust/debugger.rs`）为每条指令变体提取其 `&InstrMeta` 引用：通过穷举 match 每个变体来获取内嵌的 meta 字段。用于调试器显示源码位置。

---

> 相关文件：[lexer.md](lexer.md) · [../implementation/vm.md](../implementation/vm.md)
