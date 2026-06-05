# LLVM 代码生成

> **实现位置**：`src/codegen/compiler.rs` (1950 行) · `src/codegen/optimizer.rs` (745 行) · `src/codegen/intrinsics.rs` (128 行) · `src/codegen/mod.rs` (228 行)

## 1. 架构总览

Stardust LLVM 编译器将 `ParseResult` 转换为可读的 LLVM IR 文本，再通过系统 `llc` 和 `clang` 工具链编译为原生可执行文件。

```
ParseResult
    │
    ▼
FuncCompiler::compile()        生成 LLVM IR 文本 (String)
    │
    ▼
optimizer::optimize()          三阶段 Stardust IR 级优化
    │
    ▼
[.ll 文件]  →  llc  →  [.o 文件]  →  clang  →  [可执行文件]
```

## 2. CodeGenConfig

```rust
pub struct CodeGenConfig {
    pub stack_size: usize,       // 默认 1,048,576
    pub heap_size: usize,        // 默认 1,048,576
    pub max_call_depth: usize,   // 默认 256
    pub optimization: u8,        // 0-3（默认 3）
    pub keep_temp: bool,         // 保留临时 .ll 文件
}
```

## 3. 全局状态布局

编译器生成以下 LLVM 全局变量：

```llvm
@stack = global [1048576 x i64] zeroinitializer    ; 全局栈数组
@sp = global i64 0                                   ; 栈指针（当前栈顶索引）
@heap = global [1048576 x i64] zeroinitializer       ; 堆数组（扁平化存储）
@heap_used = global [1048576 x i1] zeroinitializer   ; 堆使用位图
@frame_base_stack = global [256 x i64] zeroinitializer  ; 帧基址栈
@frame_depth = global i64 0                           ; 当前调用深度
```

**设计说明**：

- `@stack` 是静态分配的大数组（编译时确定大小，基于 `stack_size`）
- `@sp` 是当前栈顶索引，指向下一个空闲槽位
- `@frame_base_stack[f]` 存储第 f 个调用帧的栈基地址（帧 f 的栈 = @stack[frame_base..sp]）
- `@frame_depth` 是当前活跃帧数

## 4. 编译期栈追踪（核心优化）

### 4.1 概念

编译器维护一个**编译时栈**（`compile_stack: Vec<StackEntry>`），用于追踪基本块（BB）内的栈值。在 BB 内，所有栈操作直接映射到 SSA 寄存器，**不产生任何内存访问**。

```rust
enum StackEntry {
    Const(i64),      // 编译期已知常量（如 Push(5) 的值）
    Reg(String),     // LLVM SSA 寄存器名（如 "%r42"）
}
```

### 4.2 栈操作映射

```
BB 内执行:
  Push(5)        → compile_stack.push(Const(5))         tracked_depth++
  Push(3)        → compile_stack.push(Const(3))         tracked_depth++
  Add            → Const(3) + Const(5) → Reg("%add_7")  tracked_depth -= 1
  Dup            → top = compile_stack.last()             tracked_depth++
                    compile_stack.push(top.clone())

BB 边界 (Mark / Jump / Call / BB 结束):
  flush()        → for each entry in compile_stack:
                       %ptr = getelementptr @stack, 0, %sp
                       store entry, %ptr
                       %sp = add %sp, 1
                   compile_stack.clear()
  reload()       → 下个 BB 开始时:
                       pop_ssa() 从 @stack 加载值到 SSA 寄存器
```

### 4.3 优势

- BB 内的 Push/Pop 链产生**零内存访问**
- 常量在编译期折叠（如 `Push(3) Push(5) Add` 直接在编译期算出 `8`）
- 仅在必须提交到共享状态时 flush（控制流边界、I/O 调用前）

### 4.4 关键方法

| 方法 | 行为 |
|------|------|
| `push_ssa(entry)` | 压入编译栈，`tracked_depth++` |
| `pop_ssa()` | 从编译栈弹出；若为空则从 @stack 生成 load IR |
| `flush()` | 将编译栈全部写入 @stack，更新 @sp，清空编译栈 |

## 5. 指令编译

`compile_instruction()` 将每条 Stardust 指令映射到 LLVM IR。

### 5.1 算术运算

**Add/Sub/Mul** — 使用 checked 内置函数：

```llvm
%r = call i64 @checked_add(i64 %a, i64 %b)
; checked_add 内部:
;   %pair = call {i64, i1} @llvm.sadd.with.overflow.i64(i64 %a, i64 %b)
;   %val = extractvalue {i64, i1} %pair, 0
;   %ovf = extractvalue {i64, i1} %pair, 1
;   br i1 %ovf, label %overflow_err, label %ok
; overflow_err:
;   call void @runtime_overflow()
;   unreachable
```

**Div** — 零除检查：

```llvm
%div_zero = icmp eq i64 %b, 0
br i1 %div_zero, label %div_err, label %div_ok
div_err:
  call void @runtime_divzero()
  unreachable
div_ok:
  %r = sdiv i64 %a, %b
```

**Mod** — 同样模式，调用 `@runtime_modzero()`。

### 5.2 CharOut

```llvm
; ASCII 范围检查
%ch_ok_low = icmp sge i64 %val, 0
%ch_ok_high = icmp sle i64 %val, 127
%ch_ok = and i1 %ch_ok_low, %ch_ok_high
br i1 %ch_ok, label %ch_put, label %ch_bad
ch_bad:
  call void @runtime_bad_ascii(i64 %val)
  unreachable
ch_put:
  %ch_trunc = trunc i64 %val to i32
  %ch_call = call i32 @putchar(i32 %ch_trunc)
```

### 5.3 Reverse

最复杂的单一指令编译，因为需要反转 `@stack` 中从帧基址到 `@sp` 的整个区间：

1. 获取帧基址（`@frame_base_stack[@frame_depth - 1]`）
2. 计算 `len = @sp - frame_base`
3. 若 `len <= 1` 则跳过
4. `malloc(len * 8)` 分配临时缓冲区
5. `memmove` 复制 @stack[frame_base..sp] → tmp
6. 反向复制 tmp[len-1-j] → @stack[frame_base+j]（循环）
7. `free(tmp)`

### 5.4 控制流

**Mark**：生成 LLVM label `mark_N:`，然后 `br label %bb_X`

**Jump**：弹出条件值，生成 `br i1 %cond, label %mark_N, label %bb_next`

**UncondJump**：生成 `br label %mark_N`

**Call**：
1. Flush 编译栈到 @stack
2. 保存帧基址：`@frame_base_stack[@frame_depth] = @sp - argc`
3. `@frame_depth++`
4. `br label %func_N_entry`
5. 返回后恢复：`@frame_depth--`，从 @stack reload

## 6. 函数编译

每个 Stardust 函数编译为独立的 LLVM label 链：

```llvm
func_N_entry:
  ; 从 @stack 加载参数（argc 个值）
  ...
  br label %bb_0

; 函数体基本块
bb_0:
  ...
  br label %bb_1
  ...

func_N_return:
  ; flush 到 @stack
  ; @frame_depth--
  ; 跳转到调用者的 ret 标签
  ret void
```

**多值返回**：函数帧的整个栈区间 `@stack[frame_base_N..@sp]` 在返回时对外可见。调用者退栈时 reload 整个区间。

## 7. 优化器

**实现位置**：`src/codegen/optimizer.rs` (745 行, 32 个单元测试)

在**Stardust 指令级别**（而非 LLVM IR 级别）进行优化，运行到不动点。

### 7.1 常量折叠

模式匹配连续指令序列：

```
Push(a) Push(b) Add  → Push(a+b)    (若 a+b 不溢出)
Push(a) Push(b) Sub  → Push(a-b)
Push(a) Push(b) Mul  → Push(a*b)
Push(a) Push(b) Div  → Push(a/b)    (b ≠ 0)
Push(a) Push(b) Mod  → Push(a%b)    (b ≠ 0)
Push(a) Push(b) Eq   → Push(a==b)
Push(a) Push(b) Ne   → Push(a!=b)
... 等所有比较和逻辑运算
```

### 7.2 死代码消除

```
Push(x) Pop  → (删除两者)
Dup Pop      → (删除两者)
```

### 7.3 窥孔优化

```
Push(0) Add  → (删除)         ; a + 0 = a
Push(0) Sub  → (删除)         ; a - 0 = a
Push(1) Mul  → (删除)         ; a * 1 = a
Reverse Reverse → (删除两者)  ; 两次反转 = 无变化
ShiftL ShiftR → (删除两者)    ; 左移+右移 = 无变化
```

### 7.4 运行策略

优化器重复运行所有 pass 直到不再产生变化（固定点迭代）。通常 2–4 轮收敛。

## 8. 运行时内置函数

**实现位置**：`src/codegen/intrinsics.rs` (128 行)

编译器将 LLVM IR 文本常量嵌入输出：

```llvm
; 溢出检查
define i64 @checked_add(i64 %a, i64 %b) {
  %pair = call {i64, i1} @llvm.sadd.with.overflow.i64(i64 %a, i64 %b)
  %val = extractvalue {i64, i1} %pair, 0
  %ovf = extractvalue {i64, i1} %pair, 1
  br i1 %ovf, label %overflow, label %done
overflow:
  call void (i8*, ...) @printf(i8* getelementptr (...) @overflow_msg)
  call void @exit(i32 1)
  unreachable
done:
  ret i64 %val
}
```

类似定义 `@checked_sub` 和 `@checked_mul`。

**错误处理函数**：
- `@runtime_divzero()` — 打印 "Division by zero" 并 exit(1)
- `@runtime_modzero()` — 打印 "Modulo by zero" 并 exit(1)
- `@runtime_bad_ascii(i64)` — 打印 "Invalid ASCII: N" 并 exit(1)
- `@runtime_stack_underflow()` — 打印错误并 exit(1)
- `@runtime_call_depth()` — 打印 "Call depth exceeded" 并 exit(1)
- `@runtime_heap_oob()` — 打印 "Heap out of bounds" 并 exit(1)

**C 库外部声明**：
```llvm
declare i32 @printf(i8*, ...)
declare i32 @putchar(i32)
declare i32 @getchar()
declare i32 @scanf(i8*, ...)
declare i32 @fprintf(i8*, i8*, ...)
declare i8* @malloc(i64)
declare void @free(i8*)
declare void @exit(i32)
```

## 9. 编译 API

```rust
// 生成 LLVM IR 字符串
pub fn compile_to_ir(parse_result: &ParseResult, config: &CodeGenConfig) -> String

// 写入 LLVM IR 文件
pub fn compile_to_ir_file(parse_result: &ParseResult, path: &Path, config: &CodeGenConfig)
    -> io::Result<()>

// 编译为目标文件 (.o)
pub fn compile_to_object(parse_result: &ParseResult, path: &Path, config: &CodeGenConfig)
    -> io::Result<()>

// 编译为可执行文件
pub fn compile_to_exe(parse_result: &ParseResult, path: &Path, config: &CodeGenConfig)
    -> io::Result<()>

// 检查 LLVM 工具链可用性
pub fn check_toolchain() -> ToolchainStatus
```

---

> 相关文件：[implementation/vm.md](implementation/vm.md) · [cli.md](cli.md)
