# 词法规则

> **实现位置**：`src/stardust/lexer.rs` (1109 行, 61 个单元测试)

## 1. 有效字符

Stardust 源码仅由以下字符组成：

| 类别 | 字符 | 用途 |
|------|------|------|
| **空白** | ` ` (U+0020) | 前导空格，空格数量决定指令语义 |
| **指令符号** | `+` `*` `` ` `` `'` `:` `;` `.` `,` `-` `=` `<` `>` `&` `~` `#` | 16 种合法符号 |
| **注释起始** | `/` | 后必须紧跟第二个 `/` 构成 `//` |
| **忽略字符** | `\t` (U+0009) `\n` (U+000A) `\r` (U+000D) | 制表符、换行、回车 |

任何其他字符（字母、数字、标点符号等）均导致词法错误。

## 2. Token 结构

词法分析器将源码流转换为 `Token` 序列。每个 Token 包含：

```rust
pub struct Token {
    pub spaces: usize,          // 前导空格数量
    pub token_type: TokenType,  // 符号类型（16 种之一）
    pub line: usize,            // 源代码行号（从 1 开始）
    pub column: usize,          // 源代码列号（从 1 开始）
    pub byte_pos: usize,        // 符号字符在源文件中的字节偏移
}
```

**关键约定**：

- `line` 和 `column` 记录 Token 的**前导空格起始位置**（非符号字符位置）
- `byte_pos` 记录**符号字符本身**的字节偏移
- 这保证了错误信息可以精确定位到指令的起始列

## 3. TokenType 枚举

```rust
pub enum TokenType {
    Plus,       // '+'
    Star,       // '*'
    Backtick,   // '`'
    Quote,      // '\''
    Colon,      // ':'
    Semicolon,  // ';'
    Dot,        // '.'
    Comma,      // ','
    Hyphen,     // '-'
    Equals,     // '='
    AngleLeft,  // '<'
    AngleRight, // '>'
    Ampersand,  // '&'
    Tilde,      // '~'
    Hash,       // '#'
    Annotation, // '/'  (内部使用，仅用于注释)
}
```

| 枚举变体 | 字符 | 语义分组 | 有效空格范围 |
|----------|------|----------|-------------|
| `Plus` | `+` | 栈操作 / Push | 1, 2, 3, 4, 5+ (0 非法) |
| `Star` | `*` | 算术运算 | 0–5 |
| `Dot` | `.` | 数字 I/O | 0–1 |
| `Comma` | `,` | 字符 I/O | 0–1 |
| `Backtick` | `` ` `` | Mark 声明 | 无限制（作为标识符） |
| `Quote` | `'` | 条件跳转 | 无限制（作为标识符） |
| `Colon` | `:` | 函数声明/调用 | 无限制（作为标识符） |
| `Semicolon` | `;` | 函数调用参数计数 | 无限制（作为标识符） |
| `Hyphen` | `-` | 堆操作 | 0–1 |
| `Equals` | `=` | 比较运算 | 0–5 |
| `AngleLeft` | `<` | 栈扩展（左向） | 0–2 |
| `AngleRight` | `>` | 栈扩展（右向） | 0–1 |
| `Ampersand` | `&` | 逻辑运算 | 0–3 |
| `Tilde` | `~` | 无条件跳转 | 无限制（作为标识符） |
| `Hash` | `#` | 调试指令 | 0–2 |
| `Annotation` | `/` | 注释起始 | 内部使用，不对外产生 Token |

## 4. 词法分析器实现

### 4.1 Lexer 结构体

```rust
pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,  // UTF-8 字符迭代器（支持 peek）
    line: usize,                  // 当前行号（1-based）
    column: usize,                // 当前列号（1-based）
    byte_pos: usize,              // 当前字节偏移
}
```

### 4.2 核心方法

| 方法 | 功能 |
|------|------|
| `new(input: &str)` | 初始化 Lexer，行/列/字节偏移均从 1/1/0 开始 |
| `advance()` | 消耗一个字符并更新 line/column/byte_pos；`\n` 触发行号+1、列号归1 |
| `peek()` | 查看下一个字符而不消耗 |
| `skip_comment(span)` | 验证 `//`（消耗第二个 `/`），跳过直到行尾或 EOF |
| `next_token()` | 核心状态机：返回 `Option<Result<Token, StardustError>>` |

### 4.3 next_token() 状态机

```
loop {
    peek_next_char:
    ┌─ ' ' → 进入空格计数模式
    │         ├─ 后跟 16 种符号 → 产生 Token(spaces, symbol_type)，返回
    │         ├─ 后跟 '/'       → 验证 '//' 并 skip_comment()；单 '/' 报 InvalidAnnotation
    │         ├─ 后跟空白       → 丢弃已计空格，continue（重新开始计数）
    │         ├─ 后跟其他字符   → 错误: NonSymbolicCharacter（返回 Error）
    │         └─ EOF            → 正常结束（返回 None）
    │
    ├─ 符号 → 产生 Token(0, symbol_type)，返回
    │
    ├─ '/'  → advance() 消耗第一个 '/'，调用 skip_comment()；
    │         若 skip_comment 失败 → InvalidAnnotation 错误
    │         成功则 continue（不产生 Token）
    │
    ├─ 空白 → advance()，continue
    │
    ├─ 其他 → 错误: InvalidCharacter { ch }，返回 Error
    │
    └─ EOF  → 返回 None
}
```

### 4.4 辅助函数

```rust
// 判断是否为 16 种指令符号之一
fn is_symbol(ch: char) -> bool {
    matches!(ch, '+' | '*' | '`' | '\'' | ':' | ';' | '.' |
                 ',' | '-' | '=' | '<' | '>' | '&' | '~' | '#')
}

// 判断是否为注释起始字符
fn is_anno(ch: char) -> bool { matches!(ch, '/') }

// 将字符映射到 TokenType
fn char_to_token_type(ch: char) -> Option<TokenType>
```

**注意**：`is_symbol()` 和 `is_anno()` 在 `lexer.rs` 和 `utils.rs` 中均有定义（代码重复，已知问题）。

## 5. 边界行为详解

### 5.1 空格计数的终止条件

空格计数在以下情况**被丢弃**（不产生 Token）：
- 空格后紧跟换行符 `\n` → 丢弃空格，行号增加，重新开始
- 空格后紧跟制表符 `\t` 或回车 `\r` → 丢弃空格，重新开始
- 空格后遇到 EOF → 正常结束，不产生 Token

**换行不跨行累计空格**。例如：
```
   +    ← 3 空格 + '+'
  *     ← 2 空格 + '*'
```
这是两个独立的 Token，空格数分别为 3 和 2。

### 5.2 注释处理

注释以 `//` 开头，到行尾结束：

- `//` 之后的所有字符（包括符号字符）被跳过
- 注释不产生任何 Token
- 只有 `//` 在同一行才能形成注释；单个 `/` 不构成注释
- 空格后的 `//` 也被视为注释（空格被丢弃）

**错误场景**：

| 输入 | 错误 |
|------|------|
| `/` (EOF) | `InvalidAnnotation` |
| `/a` | `InvalidAnnotation` |
| `/ comment` | `InvalidAnnotation` |
| `  /` (空格+单斜杠) | `InvalidAnnotation`，span 指向空格起始位置 |

### 5.3 无效字符处理

| 输入 | 错误 |
|------|------|
| `a` (字母) | `InvalidCharacter { ch: 'a' }` |
| `5` (数字) | `InvalidCharacter { ch: '5' }` |
| `!` (标点) | `InvalidCharacter { ch: '!' }` |
| `@` | `InvalidCharacter { ch: '@' }` |
| `   x` (空格+字母) | `NonSymbolicCharacter`，span 指向空格起始列 |

### 5.4 字节位置跟踪

`byte_pos` 指向**符号字符本身**在源文件中的字节偏移（从 0 开始计数）：

```
输入: "   +"
字节:  0123
         ↑ byte_pos = 3 (符号 '+' 的位置)
```

由于 Stardust 源码仅使用 ASCII 字符（空格 + 16 符号），`byte_pos` 始终等于非换行字符的列偏移总和。

### 5.5 连续符号处理

无空格符号可以连续出现，各自产生独立的 Token：

```
输入: "+*"
→ Token { spaces: 0, Plus, line: 1, column: 1, byte_pos: 0 }
→ Token { spaces: 0, Star, line: 1, column: 2, byte_pos: 1 }
```

## 6. 词法阶段错误

三种词法错误在 `next_token()` 中直接产生，不进入解析阶段：

| 错误变体 | 触发条件 | 示例 |
|----------|----------|------|
| `InvalidCharacter { ch }` | 遇到非有效字符（字母、数字、标点） | `a`, `5`, `!` |
| `NonSymbolicCharacter` | 空格后跟了非符号字符 | `   x`, ` 9` |
| `InvalidAnnotation` | 单个 `/` 而非 `//` | `/`, `/a`, `/ ` |

### 错误产生时的行为

- 错误产生后，`tokenize()` 包装函数在第一个错误处立即返回 `Err`
- 不会尝试错误恢复或继续解析
- 集成测试中验证了部分错误恢复行为：如 `+x` — 解析出 `+` Token 后在 `x` 处报错

## 7. 公开 API

```rust
/// 将源代码字符串解析为 Token 序列。
/// 在第一个词法错误处停止并返回 Err。
pub fn tokenize(source: &str) -> Result<Vec<Token>, StardustError>
```

## 8. 测试覆盖

61 个单元测试覆盖以下方面（`lexer.rs` 内联 `#[cfg(test)] mod tests`）：

| 测试类别 | 测试数量 | 覆盖内容 |
|----------|---------|----------|
| 单符号识别 | 16 | 全部 16 种符号的无空格情况 |
| 前导空格 | 10 | 各种空格数量组合 |
| 行列跟踪 | 5 | 多 token 位置、多行、空行跳过 |
| 空白处理 | 6 | tab、\r、混合空白、多换行 |
| 注释处理 | 9 | 整行注释、行尾注释、注释含符号、空格前缀注释 |
| 注释错误 | 4 | 单斜杠 `\` 的多种情况 |
| 无效字符 | 6 | 字母、数字、标点、空格+字母、空格+数字 |
| EOF 处理 | 5 | 空输入、仅空格、文件末尾空格 |
| byte_pos | 4 | 简单、带空格、多 token、换行后 |
| tokenize() 包装 | 5 | 正常路径、空串、错误停止、仅注释 |
| 综合场景 | 8 | Hello World 片段、函数声明/调用模式、Mark+Jump |
| 辅助函数 | 6 | is_symbol、is_anno、char_to_token_type |
| 边界情况 | 7 | EOF 无换行、注释后 EOF、空格重置、大空格数 |

---

> 相关文件：[instructions.md](instructions.md) · [../errors.md](../errors.md)
