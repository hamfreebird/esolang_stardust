use std::collections::HashMap;
use std::io::{self, Read, Write};
use crate::stardust::{CallFrame, ErrorKind, InstrMeta, Instruction, ParseResult, StardustError, VM, MAX_CALL_DEPTH};

/// 弹出帧栈顶，错误携带源码位置
fn pop_frame(frame: &mut CallFrame, meta: &InstrMeta) -> Result<i64, StardustError> {
    frame.stack.pop().ok_or_else(|| {
        StardustError::new(ErrorKind::StackUnderflow, Some(meta.span.clone()))
    })
}

/// 创建带源码位置的错误
fn framed_err(kind: ErrorKind, meta: &InstrMeta) -> StardustError {
    StardustError::new(kind, Some(meta.span.clone()))
}

/// 在指定帧中执行单条指令
///
/// 返回 `Ok(Some(...))` 表示遇到 Call 指令，调用者需压入新帧。
/// 返回 `Ok(None)` 表示指令已被本函数完全处理。
fn execute_in_frame(
    frame: &mut CallFrame,
    heap: &mut HashMap<i64, i64>,
    inst: &Instruction,
) -> Result<Option<(usize, usize, InstrMeta)>, StardustError> {
    match inst {
        // ── 栈操作 ──────────────────────────────────────────
        Instruction::Push(val, _) => {
            frame.stack.push(*val);
            frame.pc += 1;
        }
        Instruction::Dup(meta) => {
            let top = *frame.stack.last().ok_or_else(||
                StardustError::new(ErrorKind::StackUnderflow, Some(meta.span.clone())))?;
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
            let r = a.checked_add(b)
                .ok_or_else(|| framed_err(ErrorKind::IntegerOverflow, meta))?;
            frame.stack.push(r);
            frame.pc += 1;
        }
        Instruction::Sub(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            let r = a.checked_sub(b)
                .ok_or_else(|| framed_err(ErrorKind::IntegerOverflow, meta))?;
            frame.stack.push(r);
            frame.pc += 1;
        }
        Instruction::Mul(meta) => {
            let b = pop_frame(frame, meta)?;
            let a = pop_frame(frame, meta)?;
            let r = a.checked_mul(b)
                .ok_or_else(|| framed_err(ErrorKind::IntegerOverflow, meta))?;
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

        // ── 数字 I/O ────────────────────────────────────────
        Instruction::NumOut(meta) => {
            let val = pop_frame(frame, meta)?;
            print!("{}", val);
            io::stdout().flush().map_err(|e|
                framed_err(ErrorKind::IoError { reason: e.to_string() }, meta))?;
            frame.pc += 1;
        }
        Instruction::NumIn(meta) => {
            let mut input = String::new();
            io::stdin().read_line(&mut input).map_err(|e|
                framed_err(ErrorKind::IoError { reason: e.to_string() }, meta))?;
            let val: i64 = input.trim().parse().map_err(|_|
                framed_err(ErrorKind::InvalidIntegerInput, meta))?;
            frame.stack.push(val);
            frame.pc += 1;
        }

        // ── 字符 I/O ────────────────────────────────────────
        Instruction::CharOut(meta) => {
            let val = pop_frame(frame, meta)?;
            if val < 0 || val > 127 {
                return Err(framed_err(ErrorKind::InvalidAscii { value: val }, meta));
            }
            print!("{}", val as u8 as char);
            io::stdout().flush().map_err(|e|
                framed_err(ErrorKind::IoError { reason: e.to_string() }, meta))?;
            frame.pc += 1;
        }
        Instruction::CharIn(meta) => {
            let mut buf = [0u8; 1];
            io::stdin().read_exact(&mut buf).map_err(|e|
                framed_err(ErrorKind::IoError { reason: e.to_string() }, meta))?;
            frame.stack.push(buf[0] as i64);
            frame.pc += 1;
        }

        // ── 控制流 ──────────────────────────────────────────
        Instruction::Mark { .. } => {
            frame.pc += 1;
        }
        Instruction::Jump { name, meta } => {
            let target = *frame.marks.get(name)
                .ok_or_else(|| framed_err(ErrorKind::UndefinedMark { name: *name }, meta))?;
            let cond = pop_frame(frame, meta)?;
            if cond != 0 {
                frame.pc = target;
            } else {
                frame.pc += 1;
            }
        }
        Instruction::UnconditionalJump { name, meta } => {
            let target = *frame.marks.get(name)
                .ok_or_else(|| framed_err(ErrorKind::UndefinedMark { name: *name }, meta))?;
            frame.pc = target;
        }

        // ── 比较运算（符号 =）────────────────────────────────
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

        // ── 逻辑运算（符号 &）────────────────────────────────
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

        // ── 堆操作（符号 -）──────────────────────────────────
        Instruction::Store(meta) => {
            let addr = pop_frame(frame, meta)?;
            let val = pop_frame(frame, meta)?;
            heap.insert(addr, val);
            frame.pc += 1;
        }
        Instruction::Load(meta) => {
            let addr = pop_frame(frame, meta)?;
            let val = heap.get(&addr).copied().unwrap_or(0);
            frame.stack.push(val);
            frame.pc += 1;
        }

        // ── 栈扩展（符号 < >）───────────────────────────────
        Instruction::ShiftL(meta) => {
            if !frame.stack.is_empty() {
                let first = frame.stack.remove(0); // 栈底
                frame.stack.push(first);            // → 栈顶
            } else {
                return Err(framed_err(ErrorKind::StackUnderflow, meta));
            }
            frame.pc += 1;
        }
        Instruction::Depth(_meta) => {
            frame.stack.push(frame.stack.len() as i64);
            frame.pc += 1;
        }
        Instruction::Pick(meta) => {
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
                frame.stack.insert(0, last); // 栈顶 → 栈底
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

        // ── 调试（符号 #）────────────────────────────────────
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
            // 当前实现为 NOP；后续 --debug 模式会在此暂停并等待用户输入
            frame.pc += 1;
        }

        // ── 函数调用（支持嵌套/递归）─────────────────────────
        Instruction::Call { name, argc, meta } => {
            frame.pc += 1; // 调用方 PC 前进到 Call 的下一条（作为 ret_pc）
            return Ok(Some((*name, *argc, meta.clone())));
        }
    }
    Ok(None)
}

// ============================================================================
// VM 实现 — 调用栈帧管理
// ============================================================================

impl VM {
    pub fn new(parse_result: ParseResult) -> Self {
        let main_frame = CallFrame::new(
            parse_result.main_instructions,
            parse_result.main_marks,
        );
        VM {
            functions: parse_result.functions,
            frames: vec![main_frame],
            heap: HashMap::new(),
            halted: false,
            debug: None,
        }
    }

    /// 运行主程序直到结束或出错
    pub fn run(&mut self) -> Result<(), StardustError> {
        while !self.halted {
            let frame_done = {
                let frame = self.frames.last().unwrap();
                frame.pc >= frame.instructions.len()
            };

            if frame_done {
                if self.frames.len() == 1 {
                    break;
                }
                let finished = self.frames.pop().unwrap();
                let parent = self.frames.last_mut().unwrap();
                parent.stack.extend(finished.stack);
                parent.pc = finished.ret_pc;
                continue;
            }

            // 读取下一条指令
            let inst = {
                let frame = self.frames.last().unwrap();
                frame.instructions[frame.pc].clone()
            };

            // ── 调试钩子 ────────────────────────────────
            let should_break = match &self.debug {
                Some(dbg) => {
                    let frame = self.frames.last().unwrap();
                    dbg.should_break(frame.pc, &frame.marks, &inst)
                }
                None => false,
            };

            if should_break {
                // 临时取出 debugger，解除借用冲突
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
            // ── 调试钩子结束 ──────────────────────────

            // 执行指令
            let call_info = {
                let frame = self.frames.last_mut().unwrap();
                execute_in_frame(frame, &mut self.heap, &inst)?
            };

            if let Some((name, argc, meta)) = call_info {
                self.handle_call(name, argc, &meta)?;
            }
        }
        Ok(())
    }

    /// 压入新的函数调用帧
    fn handle_call(
        &mut self,
        func_name: usize,
        argc: usize,
        call_meta: &InstrMeta,
    ) -> Result<(), StardustError> {
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(StardustError::new(
                ErrorKind::CallDepthExceeded,
                Some(call_meta.span.clone()),
            ));
        }

        // 查找函数体
        let body = self.functions.get(&func_name)
            .ok_or_else(|| StardustError::new(
                ErrorKind::UndefinedFunction { name: func_name },
                Some(call_meta.span.clone()),
            ))?
            .clone();

        // 构建函数体内 marks 表（同时检查重复）
        let mut local_marks: HashMap<usize, usize> = HashMap::new();
        for (idx, inst) in body.iter().enumerate() {
            if let Instruction::Mark { name, meta } = inst {
                if local_marks.insert(*name, idx).is_some() {
                    return Err(StardustError::new(
                        ErrorKind::DuplicateMark { name: *name },
                        Some(meta.span.clone()),
                    ));
                }
            }
        }

        // 从当前帧弹出参数（在独立作用域中）
        let (args, ret_pc) = {
            let frame = self.frames.last().unwrap();

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

            let mut popped = Vec::with_capacity(argc);
            // 不能用 &mut self while iterating, 收集索引后弹出
            let stack = &frame.stack;
            let start = stack.len() - argc;
            for i in start..stack.len() {
                popped.push(stack[i]);
            }
            (popped, frame.pc)
        };

        // 从当前帧中真正弹出（此时借用已释放）
        {
            let frame = self.frames.last_mut().unwrap();
            for _ in 0..argc {
                frame.stack.pop();
            }
        }

        // 压入新帧
        let mut new_frame = CallFrame::new(body, local_marks);
        new_frame.stack = args;
        new_frame.ret_pc = ret_pc;
        self.frames.push(new_frame);

        Ok(())
    }
}
