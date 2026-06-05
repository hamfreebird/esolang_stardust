# 运行时注解系统

> **实现位置**：`src/stardust/config.rs` (221 行, 6 个单元测试)

## 1. 概述

Stardust 支持通过 `#[key=value]` 注解在源码中配置 VM 运行时参数。注解在**词法分析之前**被提取并移除，不影响正常的词法/语法分析。

## 2. 语法

```text
#[max_steps=5000000]
#[max_stack=524288]
#[max_heap=262144]
#[max_call_depth=128]
#[time_limit=30]
#[char_mode=unicode]
```

**规则**：

- 每条注解独占一行
- 格式严格为 `#[key=value]`（`#` `[` 和 `]` 之间无空格）
- 注解可在文件任意位置（通常在顶部），但每行独立处理
- 仅解析以 `//` 开头的行，其他行保持原样

## 3. VmConfig 结构

```rust
#[derive(Debug, Clone)]
pub struct VmConfig {
    /// 最大执行步数，None = 无限制
    pub max_steps: Option<u64>,

    /// 每帧最大栈大小（元素个数）
    pub max_stack_size: usize,

    /// 全局堆的最大条目数
    pub max_heap_entries: usize,

    /// 最大函数调用深度
    pub max_call_depth: usize,

    /// Wall-clock 时间限制（秒），None = 无限制
    pub time_limit_secs: Option<u64>,

    /// 字符输出模式
    pub char_mode: CharMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CharMode {
    Ascii,    // 仅输出 0-127，无效码点报错
    Unicode,  // 支持全部 Unicode 码点
}
```

## 4. 配置键参考

| 键 | 值类型 | 默认值 | 0 的含义 | 说明 |
|----|--------|--------|----------|------|
| `max_steps` | u64 | `10,000,000` | 无限制 | 最大执行步数，达到后报 `StepLimitExceeded` |
| `max_stack` | usize | `1,048,576` (1M) | 不适用 | 每帧最大栈大小，超过报 `StackOverflow` |
| `max_heap` | usize | `1,048,576` (1M) | 不适用 | 最大堆条目数，超出报 `HeapOverflow` |
| `max_call_depth` | usize | `256` | 不适用 | 最大调用深度，超出报 `CallDepthExceeded` |
| `time_limit` | u64 | 无限制 | 无限制 | Wall-clock 秒数限制，达到后报 `TimeLimitExceeded` |
| `char_mode` | 枚举 | `ascii` | 不适用 | `ascii` 或 `unicode` |

## 5. 配置键详解

### 5.1 max_steps

控制 VM 最大执行步数（每条指令执行一次计为一步）。

```text
#[max_steps=5000000]   # 限制 500 万步
#[max_steps=0]          # 无限制
```

- 检查时机：每条指令执行前（`VM::run()` 主循环开头）
- 未设置（或设为 0）→ `max_steps = None`，不检查
- 超出限制时错误不携带源码位置（全局限制性错误）

### 5.2 max_stack

控制**每帧**栈的最大元素个数。

```text
#[max_stack=1048576]   # 1M 元素
```

- 检查时机：每次 `Push`/`Dup`/`NumIn`/`CharIn`/`Depth`/`Pick`/`Load` 指令执行前
- 这些指令调用内部 `check_push()` 闭包进行验证
- 超出限制 → `StackOverflow` 错误（携带触发指令的源码位置）

### 5.3 max_heap

控制全局堆 `HashMap<i64, i64>` 的最大条目数。

```text
#[max_heap=262144]     # 256K 条目
```

- 检查时机：`Store` 指令执行时
- 仅检查**新增键**（`heap.contains_key(&addr)` 为 false 时）
- 更新已有键不增加条目数，不触发检查

### 5.4 max_call_depth

控制函数调用栈的最大深度（CallFrame 数量 - 1）。

```text
#[max_call_depth=128]  # 允许 128 层嵌套调用
```

- 检查时机：`handle_call()` 中压入新帧前
- 帧数 ≥ max_call_depth 时拒绝调用
- 默认 256 层，配合无限递归保护

### 5.5 time_limit

Wall-clock 时间限制（秒）。

```text
#[time_limit=30]       # 30 秒限制
#[time_limit=0]        # 无限制
```

- 检查时机：每执行 1000 步检查一次（`steps % 1000 == 0`）
- 使用 `std::time::Instant` 计时
- 粒度约 1000 条指令，不是逐指令检查（性能优化）

### 5.6 char_mode

控制 `CharOut` 指令的输出行为。

```text
#[char_mode=ascii]     # 仅允许 0-127（默认）
#[char_mode=unicode]   # 允许全部 Unicode 码点
```

- **Ascii 模式**：弹出值必须在 0–127 范围内；不在范围报 `InvalidAscii { value }`，输出 `(value as u8) as char`
- **Unicode 模式**：弹出值通过 `char::from_u32(value as u32)` 验证；无效码点报 `InvalidCodePoint { value }`

## 6. 解析过程

```rust
pub fn parse_config(source: &str) -> Result<(VmConfig, String), ParseConfigError>
```

**实现细节**：

1. 从默认 `VmConfig::default()` 开始
2. 逐行扫描源码：
   - 若行匹配 `#[key=value]` 格式 → 调用 `apply_config()` 应用配置，跳过该行
   - 否则 → 保留该行到清洁源码
3. 返回 `(最终配置, 不含注解行的源码)`

**行保留逻辑**：非注解行之间用 `\n` 连接，保持行号不变（保证后续错误定位的行号准确）。

## 7. 配置错误

| 错误 | 触发条件 | 示例 |
|------|----------|------|
| `ParseConfigError::UnknownKey { key }` | 未识别的配置键 | `#[foo=bar]` |
| `ParseConfigError::InvalidValue { key, value }` | 值格式不正确 | `#[max_stack=abc]`, `#[char_mode=utf8]` |

配置错误在解析阶段（词法分析前）直接退出程序，不进入后续流水线。

## 8. 默认配置

```rust
impl Default for VmConfig {
    fn default() -> Self {
        VmConfig {
            max_steps: Some(10_000_000),  // 1000 万步
            max_stack_size: 1_048_576,    // 1M 元素
            max_heap_entries: 1_048_576,  // 1M 条目
            max_call_depth: 256,           // 256 层
            time_limit_secs: None,         // 无时间限制
            char_mode: CharMode::Ascii,    // ASCII 模式
        }
    }
}
```

所有这些默认值均可通过注解覆盖。

---

> 相关文件：[lexer.md](lexer.md) · [../implementation/vm.md](../implementation/vm.md)
