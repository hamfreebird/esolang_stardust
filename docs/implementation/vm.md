# 虚拟机

> **实现位置**：`src/stardust/vm.rs` (614 行)

## 1. 架构概览

Stardust VM 是一个**基于多栈帧的栈式虚拟机**，支持函数调用、堆存储和可配置的安全边界。

```
┌───────────────────────────────────────────┐
│                    VM                      │
│  ┌─────────────────────────────────────┐  │
│  │ functions: HashMap<usize, FuncDef>  │  │  函数表（共享）
│  ├─────────────────────────────────────┤  │
│  │ frames: Vec<CallFrame>              │  │  调用帧栈
│  │  ┌─────────────────────────────┐    │  │
│  │  │ Frame[0]: main              │    │  │  主程序帧
│  │  │  instructions, stack, pc    │    │  │
│  │  │  marks, ret_pc              │    │  │
│  │  ├─────────────────────────────┤    │  │
│  │  │ Frame[1]: func(3)           │    │  │  函数调用帧
│  │  │  instructions (Rc shared)   │    │  │
│  │  │  stack, pc, marks, ret_pc   │    │  │
│  │  └─────────────────────────────┘    │  │
│  ├─────────────────────────────────────┤  │
│  │ heap: HashMap<i64, i64>             │  │  全局堆（所有帧共享）
│  ├─────────────────────────────────────┤  │
│  │ halted: bool                        │  │  终止标志
│  │ debug: Option<Debugger>             │  │  调试器
│  │ config: VmConfig                    │  │  运行时配置
│  └─────────────────────────────────────┘  │
└───────────────────────────────────────────┘
```

## 2. CallFrame 详解

```rust
pub struct CallFrame {
    pub instructions: Rc<Vec<Instruction>>,  // Rc 共享，零克隆
    pub stack: Vec<i64>,                      // 帧独立栈
    pub pc: usize,                            // 程序计数器
    pub marks: HashMap<usize, usize>,         // Mark名 → 指令索引
    pub ret_pc: usize,                        // 返回地址
}
```

### 2.1 帧生命周期

```
创建帧:
  CallFrame::new(instructions, marks)
    → stack = []
    → pc = 0
    → ret_pc = 0 (由 handle_call 设置)

执行帧:
  while pc < instructions.len():
    execute_in_frame(frame, heap, &instructions[pc], io, config)
    pc++ 或 jump

帧结束 (pc >= instructions.len()):
  if frames.len() == 1: halt (主程序结束)
  else:
    finished = frames.pop()
    parent.stack.extend(finished.stack)  // 多值返回
    parent.pc = finished.ret_pc           // 恢复执行点
```

### 2.2 多值返回机制

函数帧结束时的处理：

```rust
let finished = self.frames.pop().unwrap();
let parent = self.frames.last_mut().unwrap();

// 将函数帧的整个栈合并到父帧
parent.stack.extend(finished.stack);

// 恢复到调用点之后
parent.pc = finished.ret_pc;
```

这意味着函数可以返回任意数量的值——函数帧栈内有多少元素，就"返回"多少值到父帧。这种设计天然支持多值返回，无需额外指令。

### 2.3 调用帧压入

```rust
fn handle_call(&mut self, func_name: usize, argc: usize, call_meta: &InstrMeta)
    -> Result<(), StardustError>
```

1. **深度检查**：`frames.len() >= max_call_depth` → `CallDepthExceeded`
2. **查找函数**：`functions.get(&func_name)` → 不存在则 `UndefinedFunction`
3. **Rc clone**：`Rc::clone(&func_def.body)` — 仅引用计数+1
4. **弹出参数**：从当前帧栈顶取出 `argc` 个值
   - 不足 → `NotEnoughArguments { func, expected, actual }`
5. **压入新帧**：`CallFrame::new(body, marks)`，设置 stack = args，ret_pc = 当前 pc

## 3. 执行循环

### 3.1 主循环结构

```rust
pub fn run_with_io<I: VmIO>(&mut self, io: &mut I) -> Result<(), StardustError> {
    let start_time = Instant::now();
    let mut steps: u64 = 0;

    while !self.halted {
        // ① 步数限制检查
        if steps >= self.config.max_steps { → StepLimitExceeded }

        steps += 1;

        // ② 时间限制检查（每 1000 步）
        if steps % 1000 == 0 {
            if elapsed >= time_limit_secs { → TimeLimitExceeded }
        }

        // ③ 帧结束检查
        if frame_done {
            if frames.len() == 1 { break; }  // 主程序结束
            pop_frame_and_merge();
            continue;
        }

        // ④ 断点检查（调试模式）
        if debug.should_break(pc, marks, inst) {
            debug.take(); debug.interact(&self); self.debug = debug;
        }

        // ⑤ 执行指令
        let call_info = execute_in_frame(frame, heap, inst, io, config)?;

        // ⑥ 函数调用处理
        if let Some((name, argc, meta)) = call_info {
            handle_call(name, argc, &meta)?;
        }
    }
    Ok(())
}
```

### 3.2 安全边界检查

| 检查项 | 位置 | 时机 |
|--------|------|------|
| 步数限制 | 循环开头 | 每条指令前 |
| 时间限制 | 循环内部 | 每 1000 步 |
| 栈大小 | `check_push()` 闭包 | Push/Dup/NumIn/CharIn/Depth/Pick/Load 前 |
| 堆条目 | `execute_in_frame` Store 分支 | 写入新键前 |
| 调用深度 | `handle_call()` | 压入新帧前 |
| 算术溢出 | `execute_in_frame` Add/Sub/Mul 分支 | 计算时 |
| 除零 | `execute_in_frame` Div/Mod 分支 | 操作数检查 |

## 4. execute_in_frame 核心函数

```rust
fn execute_in_frame<I: VmIO>(
    frame: &mut CallFrame,
    heap: &mut HashMap<i64, i64>,
    inst: &Instruction,
    io: &mut I,
    config: &VmConfig,
) -> Result<Option<(usize, usize, InstrMeta)>, StardustError>
```

**返回值**：
- `Ok(None)` — 指令已完全处理，PC 已递增
- `Ok(Some((name, argc, meta)))` — 遇到了 Call 指令，调用者需压入新帧（PC 已递增到 Call 的下一条）
- `Err(e)` — 运行时错误，携带源码位置

### 4.1 内置的 check_push 闭包

```rust
let check_push = |frame: &CallFrame, meta: &InstrMeta| -> Result<(), StardustError> {
    if frame.stack.len() >= config.max_stack_size {
        return Err(framed_err(ErrorKind::StackOverflow, meta));
    }
    Ok(())
};
```

该闭包在以下指令执行前调用：`Push`, `Dup`, `NumIn`, `CharIn`, `Depth`, `Pick`, `Load`。

### 4.2 弹出辅助函数

```rust
fn pop_frame(frame: &mut CallFrame, meta: &InstrMeta) -> Result<i64, StardustError> {
    frame.stack.pop().ok_or_else(|| {
        StardustError::new(ErrorKind::StackUnderflow, Some(meta.span.clone()))
    })
}

fn framed_err(kind: ErrorKind, meta: &InstrMeta) -> StardustError {
    StardustError::new(kind, Some(meta.span.clone()))
}
```

这两个辅助函数确保每个运行时错误都携带触发指令的源码位置（通过 `InstrMeta` 中的 `SourceSpan`）。

## 5. VmIO trait 与 StdIO

### 5.1 Trait 定义

```rust
pub trait VmIO {
    fn write_char(&mut self, ch: char) -> io::Result<()>;
    fn read_char(&mut self) -> io::Result<u8>;
    fn write_num(&mut self, n: i64) -> io::Result<()>;
    fn read_num(&mut self) -> io::Result<i64>;
}
```

### 5.2 StdIO 实现

| 方法 | 具体行为 |
|------|----------|
| `write_char(ch)` | `write!(stdout, "{}", ch)` + `flush()` |
| `read_char()` | `stdin().read_exact(&mut [0u8; 1])` — 读取 1 字节 |
| `write_num(n)` | `write!(stdout, "{}", n)` + `flush()` |
| `read_num()` | `stdin().read_line()` + `.trim().parse::<i64>()` |

### 5.3 注入机制

```rust
// 默认执行（使用 StdIO）
vm.run() → vm.run_with_io(&mut StdIO)

// 注入自定义 I/O（测试/嵌入场景）
vm.run_with_io(&mut my_io_impl)
```

I/O 错误（`io::Error`）在 `execute_in_frame` 中被转换为 `ErrorKind::IoError { reason }` 并携带指令源码位置。

## 6. VmSnapshot — 错误状态快照

当运行时错误发生时，`VmSnapshot` 捕获完整的 VM 状态：

```rust
pub struct VmSnapshot {
    pub frames: Vec<FrameSnapshot>,   // 所有帧的栈和 PC
    pub heap: HashMap<i64, i64>,      // 堆内容
    pub halted: bool,
    pub steps_executed: u64,
}

pub struct FrameSnapshot {
    pub stack: Vec<i64>,
    pub pc: usize,
    pub ret_pc: usize,
}
```

`VmSnapshot::from_vm(&vm)` 从当前 VM 状态创建快照（clone 所有必要数据）。目前快照用于步数限制超限错误，未来可扩展到所有运行时错误。

## 7. VM 公共接口

| 方法 | 用途 |
|------|------|
| `new(ParseResult, VmConfig)` | 创建 VM 实例 |
| `run()` | 执行主程序（使用 StdIO） |
| `run_with_io(io)` | 执行主程序（使用注入的 I/O） |
| `current_frame()` | 获取当前执行帧 |
| `current_frame_index()` | 当前帧索引（0 = main） |
| `current_stack()` | 当前帧栈的引用 |
| `heap_entries()` | 堆引用 |
| `heap_len()` | 堆条目数 |
| `function_count()` | 函数表条目数 |
| `all_frames()` | 所有调用帧的切片 |
| `set_main_stack(Vec<i64>)` | REPL 状态注入 |
| `take_main_stack()` | REPL 状态保存 |
| `set_heap(HashMap)` | REPL 状态注入 |
| `take_heap()` | REPL 状态保存 |

---

> 相关文件：[architecture.md](architecture.md) · [pipeline.md](pipeline.md)
