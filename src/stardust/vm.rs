use crate::stardust::config::VmConfig;
use crate::stardust::{
    CallFrame, ErrorKind, InstrMeta, Instruction, ParseResult, StardustError, VM,
};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::rc::Rc;
use std::time::Instant;

/// 弹出帧栈顶，错误携带源码位置
fn pop_frame(frame: &mut CallFrame, meta: &InstrMeta) -> Result<i64, StardustError> {
    frame
        .stack
        .pop()
        .ok_or_else(|| StardustError::new(ErrorKind::StackUnderflow, Some(meta.span.clone())))
}

/// 创建带源码位置的错误
fn framed_err(kind: ErrorKind, meta: &InstrMeta) -> StardustError {
    StardustError::new(kind, Some(meta.span.clone()))
}

/// I/O 抽象 trait — 允许注入 mock 实现用于测试和 IDE 集成
pub trait VmIO {
    fn write_char(&mut self, ch: char) -> io::Result<()>;
    fn read_char(&mut self) -> io::Result<u8>;
    fn write_num(&mut self, n: i64) -> io::Result<()>;
    fn read_num(&mut self) -> io::Result<i64>;
}

/// 默认 I/O 实现：stdin/stdout
pub struct StdIO;

impl VmIO for StdIO {
    fn write_char(&mut self, ch: char) -> io::Result<()> {
        let mut stdout = io::stdout();
        write!(stdout, "{}", ch)?;
        stdout.flush()
    }
    fn read_char(&mut self) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        io::stdin().read_exact(&mut buf)?;
        Ok(buf[0])
    }
    fn write_num(&mut self, n: i64) -> io::Result<()> {
        let mut stdout = io::stdout();
        write!(stdout, "{}", n)?;
        stdout.flush()
    }
    fn read_num(&mut self) -> io::Result<i64> {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        input.trim().parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// VM 错误发生时的状态快照
#[derive(Debug, Clone)]
pub struct VmSnapshot {
    pub frames: Vec<FrameSnapshot>,
    pub heap: HashMap<i64, i64>,
    pub halted: bool,
    pub steps_executed: u64,
}

#[derive(Debug, Clone)]
pub struct FrameSnapshot {
    pub stack: Vec<i64>,
    pub pc: usize,
    pub ret_pc: usize,
}

impl VmSnapshot {
    pub fn from_vm(vm: &VM) -> Self {
        VmSnapshot {
            frames: vm
                .all_frames()
                .iter()
                .map(|f| FrameSnapshot { stack: f.stack.clone(), pc: f.pc, ret_pc: f.ret_pc })
                .collect(),
            heap: vm.heap_entries().clone(),
            halted: vm.halted,
            steps_executed: 0, // 由调用者填入
        }
    }
}

impl VM {
    /// 当前帧（栈顶帧）。
    ///
    /// `frames` 恒非空的不变式由 `VM::new` 建立：构造时压入主帧，
    /// 而 `run_with_io` 只在 `frames.len() == 1` 时弹出（即不会弹出最后一个帧）。
    /// 这里用 `expect` 把该不变式写成显式契约，取代散落的 `unwrap()`。
    fn top_frame(&self) -> &CallFrame {
        self.frames.last().expect("VM frames invariant: main frame is never popped")
    }

    fn top_frame_mut(&mut self) -> &mut CallFrame {
        self.frames.last_mut().expect("VM frames invariant: main frame is never popped")
    }
}

/// 在指定帧中执行单条指令
///
/// 返回 `Ok(Some(...))` 表示遇到 Call 指令，调用者需压入新帧。
/// 返回 `Ok(None)` 表示指令已被本函数完全处理。
fn execute_in_frame<I: VmIO>(
    frame: &mut CallFrame,
    heap: &mut HashMap<i64, i64>,
    inst: &Instruction,
    io: &mut I,
    config: &VmConfig,
) -> Result<Option<(usize, usize, InstrMeta)>, StardustError> {
    // 栈增长安全检查
    let check_push = |frame: &CallFrame, meta: &InstrMeta| -> Result<(), StardustError> {
        if frame.stack.len() >= config.max_stack_size {
            return Err(framed_err(ErrorKind::StackOverflow, meta));
        }
        Ok(())
    };

    match inst {
        // ── 栈操作 ──────────────────────────────────────────
        Instruction::Push(val, meta) => {
            check_push(frame, meta)?;
            frame.stack.push(*val);
            frame.pc += 1;
        }
        Instruction::Dup(meta) => {
            check_push(frame, meta)?;
            let top = *frame.stack.last().ok_or_else(|| {
                StardustError::new(ErrorKind::StackUnderflow, Some(meta.span.clone()))
            })?;
            frame.stack.push(top);
            frame.pc += 1;
        }
        Instruction::Swap(meta) => {
            let a = pop_frame(frame, meta)?;
            let b = pop_frame(frame, meta)?;
            frame.stack.push(a);
            frame.stack.push(b);
            frame.pc += 1;
        }
        Instruction::Rotate(meta) => {
            let c = pop_frame(frame, meta)?;
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(c);
            frame.stack.push(a);
            frame.stack.push(b);
            frame.pc += 1;
        }
        Instruction::Pop(meta) => {
            pop_frame(frame, meta)?;
            frame.pc += 1;
        }

        // ── 算术运算（带溢出检查）───────────────────────────
        Instruction::Add(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            let r = a.checked_add(b).ok_or_else(|| framed_err(ErrorKind::IntegerOverflow, meta))?;
            frame.stack.push(r);
            frame.pc += 1;
        }
        Instruction::Sub(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            let r = a.checked_sub(b).ok_or_else(|| framed_err(ErrorKind::IntegerOverflow, meta))?;
            frame.stack.push(r);
            frame.pc += 1;
        }
        Instruction::Mul(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            let r = a.checked_mul(b).ok_or_else(|| framed_err(ErrorKind::IntegerOverflow, meta))?;
            frame.stack.push(r);
            frame.pc += 1;
        }
        Instruction::Div(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            if b == 0 {
                return Err(framed_err(ErrorKind::DivisionByZero, meta));
            }
            frame.stack.push(a / b);
            frame.pc += 1;
        }
        Instruction::Mod(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            if b == 0 {
                return Err(framed_err(ErrorKind::ModuloByZero, meta));
            }
            frame.stack.push(a % b);
            frame.pc += 1;
        }
        Instruction::Reverse(_) => {
            frame.stack.reverse();
            frame.pc += 1;
        }

        // ── 数字 I/O（通过 VmIO trait）───────────────────────
        Instruction::NumOut(meta) => {
            let val = pop_frame(frame, meta)?;
            io.write_num(val)
                .map_err(|e| framed_err(ErrorKind::IoError { reason: e.to_string() }, meta))?;
            frame.pc += 1;
        }
        Instruction::NumIn(meta) => {
            check_push(frame, meta)?;
            let val = io
                .read_num()
                .map_err(|e| framed_err(ErrorKind::IoError { reason: e.to_string() }, meta))?;
            frame.stack.push(val);
            frame.pc += 1;
        }

        // ── 字符 I/O（支持 Unicode）──────────────────────────
        Instruction::CharOut(meta) => {
            let val = pop_frame(frame, meta)?;
            match config.char_mode {
                crate::stardust::config::CharMode::Ascii => {
                    if !(0..=127).contains(&val) {
                        return Err(framed_err(ErrorKind::InvalidAscii { value: val }, meta));
                    }
                    io.write_char(val as u8 as char).map_err(|e| {
                        framed_err(ErrorKind::IoError { reason: e.to_string() }, meta)
                    })?;
                }
                crate::stardust::config::CharMode::Unicode => {
                    let ch = char::from_u32(val as u32).ok_or_else(|| {
                        framed_err(ErrorKind::InvalidCodePoint { value: val }, meta)
                    })?;
                    io.write_char(ch).map_err(|e| {
                        framed_err(ErrorKind::IoError { reason: e.to_string() }, meta)
                    })?;
                }
            }
            frame.pc += 1;
        }
        Instruction::CharIn(meta) => {
            check_push(frame, meta)?;
            let byte = io
                .read_char()
                .map_err(|e| framed_err(ErrorKind::IoError { reason: e.to_string() }, meta))?;
            frame.stack.push(byte as i64);
            frame.pc += 1;
        }

        // ── 控制流 ──────────────────────────────────────────
        Instruction::Mark { .. } => {
            frame.pc += 1;
        }
        Instruction::Jump { name, meta } => {
            let target = *frame
                .marks
                .get(name)
                .ok_or_else(|| framed_err(ErrorKind::UndefinedMark { name: *name }, meta))?;
            let cond = pop_frame(frame, meta)?;
            if cond != 0 {
                frame.pc = target;
            } else {
                frame.pc += 1;
            }
        }
        Instruction::UnconditionalJump { name, meta } => {
            let target = *frame
                .marks
                .get(name)
                .ok_or_else(|| framed_err(ErrorKind::UndefinedMark { name: *name }, meta))?;
            frame.pc = target;
        }

        // ── 比较运算 ────────────────────────────────────────
        Instruction::Eq(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a == b { 1 } else { 0 });
            frame.pc += 1;
        }
        Instruction::Ne(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a != b { 1 } else { 0 });
            frame.pc += 1;
        }
        Instruction::Lt(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a < b { 1 } else { 0 });
            frame.pc += 1;
        }
        Instruction::Gt(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a > b { 1 } else { 0 });
            frame.pc += 1;
        }
        Instruction::Le(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a <= b { 1 } else { 0 });
            frame.pc += 1;
        }
        Instruction::Ge(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a >= b { 1 } else { 0 });
            frame.pc += 1;
        }

        // ── 逻辑运算 ────────────────────────────────────────
        Instruction::And(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a != 0 && b != 0 { 1 } else { 0 });
            frame.pc += 1;
        }
        Instruction::Or(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a != 0 || b != 0 { 1 } else { 0 });
            frame.pc += 1;
        }
        Instruction::Not(meta) => {
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if a == 0 { 1 } else { 0 });
            frame.pc += 1;
        }
        Instruction::Xor(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            frame.stack.push(if (a != 0) ^ (b != 0) { 1 } else { 0 });
            frame.pc += 1;
        }

        // ── 堆操作（带上限检查）──────────────────────────────
        Instruction::Store(meta) => {
            let addr = pop_frame(frame, meta)?;
            let val = pop_frame(frame, meta)?;
            if heap.len() >= config.max_heap_entries && !heap.contains_key(&addr) {
                return Err(framed_err(ErrorKind::HeapOverflow, meta));
            }
            heap.insert(addr, val);
            frame.pc += 1;
        }
        Instruction::Load(meta) => {
            check_push(frame, meta)?;
            let addr = pop_frame(frame, meta)?;
            let val = heap.get(&addr).copied().unwrap_or(0);
            frame.stack.push(val);
            frame.pc += 1;
        }

        // ── 栈扩展 ──────────────────────────────────────────
        Instruction::ShiftL(meta) => {
            if !frame.stack.is_empty() {
                let first = frame.stack.remove(0);
                frame.stack.push(first);
            } else {
                return Err(framed_err(ErrorKind::StackUnderflow, meta));
            }
            frame.pc += 1;
        }
        Instruction::Depth(meta) => {
            check_push(frame, meta)?;
            frame.stack.push(frame.stack.len() as i64);
            frame.pc += 1;
        }
        Instruction::Pick(meta) => {
            check_push(frame, meta)?;
            let n = pop_frame(frame, meta)?;
            let n = n.max(0) as usize;
            if n >= frame.stack.len() {
                return Err(framed_err(ErrorKind::StackUnderflow, meta));
            }
            let idx = frame.stack.len() - 1 - n;
            let val = frame.stack[idx];
            frame.stack.push(val);
            frame.pc += 1;
        }
        Instruction::ShiftR(meta) => {
            if let Some(last) = frame.stack.pop() {
                frame.stack.insert(0, last);
            } else {
                return Err(framed_err(ErrorKind::StackUnderflow, meta));
            }
            frame.pc += 1;
        }
        Instruction::DropN(meta) => {
            let n = pop_frame(frame, meta)?;
            let n = n.max(0) as usize;
            if n > frame.stack.len() {
                return Err(framed_err(ErrorKind::StackUnderflow, meta));
            }
            let new_len = frame.stack.len() - n;
            frame.stack.truncate(new_len);
            frame.pc += 1;
        }

        // ── 调试 ────────────────────────────────────────────
        Instruction::DumpStack(_meta) => {
            eprintln!("[DEBUG] Stack: {:?}", frame.stack);
            frame.pc += 1;
        }
        Instruction::DumpState(_meta) => {
            eprintln!(
                "[DEBUG] PC={}, StackDepth={}, HeapSize={}",
                frame.pc,
                frame.stack.len(),
                heap.len(),
            );
            frame.pc += 1;
        }
        Instruction::Breakpoint(_meta) => {
            frame.pc += 1;
        }

        // ── 函数调用 ────────────────────────────────────────
        Instruction::Call { name, argc, meta } => {
            frame.pc += 1;
            return Ok(Some((*name, *argc, meta.clone())));
        }
    }
    Ok(None)
}

// ============================================================================
// VM 实现 — 调用栈帧管理
// ============================================================================

impl VM {
    pub fn new(parse_result: ParseResult, config: VmConfig) -> Self {
        let main_frame =
            CallFrame::new(Rc::new(parse_result.main_instructions), parse_result.main_marks);
        VM {
            functions: parse_result.functions,
            frames: vec![main_frame],
            heap: HashMap::new(),
            halted: false,
            debug: None,
            config,
        }
    }

    /// 运行主程序直到结束或出错（使用默认 StdIO）
    pub fn run(&mut self) -> Result<(), StardustError> {
        let mut io = StdIO;
        self.run_with_io(&mut io)
    }

    /// 运行主程序，使用注入的 I/O 实现
    pub fn run_with_io<I: VmIO>(&mut self, io: &mut I) -> Result<(), StardustError> {
        let start_time = Instant::now();
        let mut steps: u64 = 0;

        while !self.halted {
            // ── 步数限制检查 ─────────────────────────────
            if let Some(limit) = self.config.max_steps
                && steps >= limit
            {
                // 注意：不要在这里构造 VmSnapshot —— 它会深拷贝所有帧的栈
                // 和整个堆，而快照在返回错误后就被丢弃，属于纯粹的无谓开销。
                return Err(StardustError::new(
                    ErrorKind::StepLimitExceeded { steps: limit },
                    None,
                ));
            }
            steps += 1;

            // ── 时间限制检查（每 1000 步检查一次）───────
            if steps.is_multiple_of(1000)
                && let Some(limit_secs) = self.config.time_limit_secs
                && start_time.elapsed().as_secs() >= limit_secs
            {
                return Err(StardustError::new(ErrorKind::TimeLimitExceeded, None));
            }

            let frame_done = {
                let frame = self.top_frame();
                frame.pc >= frame.instructions.len()
            };

            if frame_done {
                if self.frames.len() == 1 {
                    break;
                }
                let finished =
                    self.frames.pop().expect("frames.len() > 1 guarantees a frame to pop");
                let parent =
                    self.frames.last_mut().expect("popping from >=2 frames always leaves a parent");
                parent.stack.extend(finished.stack);
                parent.pc = finished.ret_pc;
                continue;
            }

            // ── 读取下一条指令（使用引用，消除 clone）─────
            let should_break = match &self.debug {
                Some(dbg) => {
                    let frame = self.top_frame();
                    dbg.should_break(frame.pc, &frame.marks, &frame.instructions[frame.pc])
                }
                None => false,
            };

            if should_break {
                let mut debug = self.debug.take();
                if let Some(ref mut dbg) = debug {
                    match dbg.interact(self) {
                        Ok(crate::stardust::debugger::DebugAction::Quit) => {
                            self.halted = true;
                            self.debug = debug;
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Debugger I/O error: {}", e);
                        }
                    }
                }
                self.debug = debug;
            }

            // ── 执行指令（Rc 共享，零 clone）──────────────
            let call_info = {
                let frame = self
                    .frames
                    .last_mut()
                    .expect("VM frames invariant: main frame is never popped");
                // 克隆 Rc（仅引用计数增减），解除对 frame 的借用
                let inst_rc = Rc::clone(&frame.instructions);
                let pc = frame.pc;
                let inst_ref = &inst_rc[pc];
                execute_in_frame(frame, &mut self.heap, inst_ref, io, &self.config)?
            };

            if let Some((name, argc, meta)) = call_info {
                self.handle_call(name, argc, &meta)?;
            }
        }
        Ok(())
    }

    /// 压入新的函数调用帧（Rc 共享函数体，预计算的 marks）
    fn handle_call(
        &mut self,
        func_name: usize,
        argc: usize,
        call_meta: &InstrMeta,
    ) -> Result<(), StardustError> {
        if self.frames.len() >= self.config.max_call_depth {
            return Err(StardustError::new(
                ErrorKind::CallDepthExceeded,
                Some(call_meta.span.clone()),
            ));
        }

        // 查找函数定义（含预计算的 marks）
        let func_def = self.functions.get(&func_name).ok_or_else(|| {
            StardustError::new(
                ErrorKind::UndefinedFunction { name: func_name },
                Some(call_meta.span.clone()),
            )
        })?;

        // Rc::clone 仅增加引用计数，不复制指令数据
        let body = Rc::clone(&func_def.body);
        let marks = func_def.marks.clone();

        // 从当前帧弹出参数
        let (args, ret_pc) = {
            let frame = self.top_frame();

            if frame.stack.len() < argc {
                return Err(StardustError::new(
                    ErrorKind::NotEnoughArguments {
                        func: func_name,
                        expected: argc,
                        actual: frame.stack.len(),
                    },
                    Some(call_meta.span.clone()),
                ));
            }

            let stack = &frame.stack;
            let start = stack.len() - argc;
            let popped = stack[start..].to_vec();
            (popped, frame.pc)
        };

        // 真正弹出
        {
            let frame = self.top_frame_mut();
            for _ in 0..argc {
                frame.stack.pop();
            }
        }

        // 压入新帧
        let mut new_frame = CallFrame::new(body, marks);
        new_frame.stack = args;
        new_frame.ret_pc = ret_pc;
        self.frames.push(new_frame);

        Ok(())
    }
}
