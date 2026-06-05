# 交互式调试器

> **实现位置**：`src/stardust/debugger.rs` (322 行)

## 1. 架构

```rust
pub struct Debugger {
    breakpoints: HashSet<usize>,  // Mark 名称的集合（作为断点目标）
    single_step: bool,             // 单步模式标志
}

pub enum DebugAction {
    Continue,  // 继续执行（单步或运行）
    Step,      // （保留，与 Continue 语义合并）
    Quit,      // 终止程序
}
```

## 2. 调试器生命周期

### 2.1 激活

```bash
stardust --debug program.sd
stardust -d program.sd
```

```rust
// main.rs
let mut vm = VM::new(parsed, config);
if debug_mode {
    vm.debug = Some(Debugger::new());
}
```

`Debugger::new()` 默认 `single_step = true`，程序在第一条指令处立即暂停。

### 2.2 执行循环集成

调试器在 VM 主循环中有两个集成点：

**① 断点检查**（每条指令执行前）：

```rust
let should_break = match &self.debug {
    Some(dbg) => dbg.should_break(frame.pc, &frame.marks, &frame.instructions[frame.pc]),
    None => false,
};
```

**② 交互循环**（should_break 为 true 时）：

```rust
if should_break {
    let mut debug = self.debug.take();  // Option::take()
    if let Some(ref mut dbg) = debug {
        match dbg.interact(self) {
            Ok(DebugAction::Quit) => { self.halted = true; break; }
            Ok(_) => {}  // Continue
            Err(e) => eprintln!("Debugger I/O error: {}", e),
        }
    }
    self.debug = debug;  // 放回
}
```

### 2.3 Option::take() 模式

这是解决 `&mut self.debug` 与 `&self`（interact 需要 `&VM` 引用）之间借用冲突的关键技术：

```rust
// 不能这样写（借用冲突）：
// self.debug.interact(self)  // debug: &mut, self: &mut

// 实际做法：
let mut debug = self.debug.take();  // 取出，self.debug = None
debug.as_mut().unwrap().interact(&self);  // 仅借用 &self
self.debug = debug;  // 放回
```

这避免了 `RefCell` 的运行时开销，同时满足 Rust 的借用规则。

## 3. 暂停条件

```rust
pub fn should_break(&self, pc: usize, marks: &HashMap<usize, usize>,
                     inst: &Instruction) -> bool
```

| 条件 | 检查 |
|------|------|
| `single_step == true` | 每条指令都暂停 |
| `matches!(inst, Breakpoint(_))` | 遇到 Breakpoint 指令 |
| `marks 中有断点名且对应 pc` | 到达 Mark(n) 断点 |

**注意**：断点通过 Mark **名称**（数字）设置，而非 PC 值。断点检查遍历 `breakpoints` 集合，通过 `marks.get(bp_name)` 查找对应 PC。

## 4. 调试器命令

交互提示为 `(sd-dbg) `。

### 4.1 执行控制

| 命令 | 别名 | 行为 |
|------|------|------|
| `step` | `s` | 设置 `single_step = true`，返回 Continue（执行一步后再次暂停） |
| `continue` | `c` | 设置 `single_step = false`，返回 Continue（运行到下一个断点或程序结束） |
| `quit` | `q` | 返回 Quit → VM 设置 `halted = true` |

### 4.2 断点管理

| 命令 | 行为 |
|------|------|
| `b <n>` 或 `break <n>` | 将 `n` 加入 `breakpoints` HashSet |
| `lb` | 列出所有断点（排序输出） |
| `db <n>` | 从 `breakpoints` 移除 `n` |

**断点语义**：`b 3` 表示 "当 PC 到达 Mark(3) 的位置时暂停"。如果程序包含多个同名的 Mark（在不同帧中），断点在当前帧的 marks 中查找。

### 4.3 状态查看

| 命令 | 输出内容 |
|------|----------|
| `p` / `stack` | 当前帧栈，栈顶→底排列。空栈显示 `(empty)` |
| `hp` / `heap` | 堆内容，按键排序，最多 50 条。超出显示 `... (N more)` |
| `i` / `info` | PC、栈深度、堆条目数、调用帧数、已定义函数数 |
| `l` / `list` | PC 前后各 8 条指令。当前行标 `→`。Mark 位置标注 `; Mark(N)` |
| `pc` | 当前 PC 和下一条指令的格式化显示 |
| `bt` / `frames` | 调用栈回溯：每帧显示 PC/指令总数/栈深度 |

### 4.4 帮助

| 命令 | 行为 |
|------|------|
| `h` / `help` | 显示所有命令的帮助文本 |

## 5. 交互循环实现

```rust
pub fn interact(&mut self, vm: &VM) -> Result<DebugAction, io::Error> {
    self.print_state(vm);  // 打印当前状态头
    loop {
        eprint!("(sd-dbg) ");
        io::stderr().flush()?;

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return Ok(DebugAction::Quit);  // stdin 错误 → 退出
        }
        let cmd = input.trim();

        match cmd {
            "s" | "step" => { self.single_step = true; return Ok(DebugAction::Continue); }
            "c" | "continue" => { self.single_step = false; return Ok(DebugAction::Continue); }
            "q" | "quit" => return Ok(DebugAction::Quit),
            // ... 其他命令 ...
            "" => {}  // 空行 → 重新提示
            _ => eprintln!("Unknown '{}'. Type 'h' for help.", cmd),
        }
    }
}
```

**设计要点**：

- 空行静默忽略（重新提示）
- stdin 读取失败视为退出信号
- 所有输出写入 stderr（保持程序 stdout 干净）

## 6. 状态显示格式

### 6.1 初始暂停

```
── Stardust Debugger ──────────────────────────────
  PC: 0  Frame: main  Stack: 0  Heap: 0
  Next: Push(7)  [line 1, col 1]
───────────────────────────────────────────────────
(sd-dbg)
```

### 6.2 stack 输出

```
  Stack (top→bottom): [72, 101, 108]
```

空栈：
```
  Stack: (empty)
```

### 6.3 list 输出

```
  Instructions around PC 5:
     1: Push(0)
     2: CharOut
     3: Push(1)
     4: CharOut    ; Mark(0)
   → 5: Push(0)
     6: Jump(0)
```

### 6.4 frames 输出

```
  Call stack (3 frames):
    #0 main: PC=10/25, depth=3
    #1 func: PC=5/12, depth=2
    #2 func: PC=3/8, depth=1
```

## 7. 输出隔离策略

所有调试器输出使用 `eprintln!`（写入 stderr），程序的正常 `CharOut`/`NumOut` 输出仍然走 stdout。这意味着：

```bash
stardust -d program.sd 2>/dev/null     # 隐藏调试器输出，只看程序输出
stardust -d program.sd >/dev/null      # 只看调试器交互
```

## 8. 辅助函数

### inst_meta()

```rust
pub fn inst_meta(inst: &Instruction) -> &InstrMeta
```

穷举 match 所有 36 种 Instruction 变体以提取 `&InstrMeta` 引用。用于调试器显示源码位置。

### format_inst()

```rust
pub fn format_inst(inst: &Instruction) -> String
```

委托给 `Instruction::format()`，返回 `"Push(5)"`, `"Add"`, `"Mark(3)"` 等人类可读字符串。

---

> 相关文件：[repl.md](repl.md) · [implementation/vm.md](implementation/vm.md)
