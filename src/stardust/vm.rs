use std::collections::HashMap;
use std::io::{self, Read, Write};
use crate::stardust::{ErrorKind, InstrMeta, Instruction, ParseResult, StardustError, VM};

/// 通用栈执行器 — 统一主程序和函数体的指令执行逻辑
///
/// 通过借用外部的栈、PC 和 Marks 映射，消除 `execute_instruction()` 与
/// `handle_function_call()` 内部循环之间的 ~150 行重复代码。
struct StackExecutor<'a> {
    stack: &'a mut Vec<i64>,
    pc: &'a mut usize,
    marks: &'a HashMap<usize, usize>,
    /// `true` = 主程序上下文 (Call 合法)；`false` = 函数体内部 (Call 非法)
    is_main: bool,
}

impl<'a> StackExecutor<'a> {
    fn new(
        stack: &'a mut Vec<i64>,
        pc: &'a mut usize,
        marks: &'a HashMap<usize, usize>,
        is_main: bool,
    ) -> Self {
        StackExecutor { stack, pc, marks, is_main }
    }

    /// 弹出栈顶，若栈为空则返回带位置的 StackUnderflow 错误
    fn pop(&mut self, meta: &InstrMeta) -> Result<i64, StardustError> {
        self.stack.pop().ok_or_else(|| {
            StardustError::new(
                ErrorKind::StackUnderflow,
                Some(meta.span.clone()),
            )
        })
    }

    /// 创建带位置信息的 StardustError
    fn error(&self, kind: ErrorKind, meta: &InstrMeta) -> StardustError {
        StardustError::new(kind, Some(meta.span.clone()))
    }

    /// 执行单条指令，返回是否需要 VM 层处理函数调用
    ///
    /// - `Ok(None)`：指令已完全处理（大多数指令）
    /// - `Ok(Some(call_info))`：主程序遇到 Call 指令，需要 VM 层接管
    /// - `Err(...)`：执行错误
    fn execute(
        &mut self,
        inst: &Instruction,
    ) -> Result<Option<(usize, usize, InstrMeta)>, StardustError> {
        match inst {
            // ── 栈操作 ──────────────────────────────────────────
            Instruction::Push(val, _) => {
                self.stack.push(*val);
                *self.pc += 1;
            }
            Instruction::Dup(meta) => {
                let top = self.pop(meta)?;
                self.stack.push(top);
                self.stack.push(top);
                *self.pc += 1;
            }
            Instruction::Swap(meta) => {
                let a = self.pop(meta)?;
                let b = self.pop(meta)?;
                self.stack.push(a);
                self.stack.push(b);
                *self.pc += 1;
            }
            Instruction::Rotate(meta) => {
                let c = self.pop(meta)?;
                let b = self.pop(meta)?;
                let a = self.pop(meta)?;
                self.stack.push(c);
                self.stack.push(a);
                self.stack.push(b);
                *self.pc += 1;
            }
            Instruction::Pop(meta) => {
                self.pop(meta)?;
                *self.pc += 1;
            }

            // ── 算术运算 ────────────────────────────────────────
            Instruction::Add(meta) => {
                let b = self.pop(meta)?;
                let a = self.pop(meta)?;
                self.stack.push(a + b);
                *self.pc += 1;
            }
            Instruction::Sub(meta) => {
                let b = self.pop(meta)?;
                let a = self.pop(meta)?;
                self.stack.push(a - b);
                *self.pc += 1;
            }
            Instruction::Mul(meta) => {
                let b = self.pop(meta)?;
                let a = self.pop(meta)?;
                self.stack.push(a * b);
                *self.pc += 1;
            }
            Instruction::Div(meta) => {
                let b = self.pop(meta)?;
                let a = self.pop(meta)?;
                if b == 0 {
                    return Err(self.error(ErrorKind::DivisionByZero, meta));
                }
                self.stack.push(a / b);
                *self.pc += 1;
            }
            Instruction::Mod(meta) => {
                let b = self.pop(meta)?;
                let a = self.pop(meta)?;
                if b == 0 {
                    return Err(self.error(ErrorKind::ModuloByZero, meta));
                }
                self.stack.push(a % b);
                *self.pc += 1;
            }
            Instruction::Reverse(_) => {
                self.stack.reverse();
                *self.pc += 1;
            }

            // ── 数字 I/O ────────────────────────────────────────
            Instruction::NumOut(meta) => {
                let val = self.pop(meta)?;
                print!("{}", val);
                io::stdout().flush().map_err(|e| self.error(
                    ErrorKind::IoError { reason: e.to_string() }, meta
                ))?;
                *self.pc += 1;
            }
            Instruction::NumIn(meta) => {
                let mut input = String::new();
                io::stdin().read_line(&mut input).map_err(|e| self.error(
                    ErrorKind::IoError { reason: e.to_string() }, meta
                ))?;
                let val: i64 = input.trim().parse().map_err(|_| self.error(
                    ErrorKind::InvalidIntegerInput, meta
                ))?;
                self.stack.push(val);
                *self.pc += 1;
            }

            // ── 字符 I/O ────────────────────────────────────────
            Instruction::CharOut(meta) => {
                let val = self.pop(meta)?;
                if val < 0 || val > 127 {
                    return Err(self.error(ErrorKind::InvalidAscii { value: val }, meta));
                }
                print!("{}", val as u8 as char);
                io::stdout().flush().map_err(|e| self.error(
                    ErrorKind::IoError { reason: e.to_string() }, meta
                ))?;
                *self.pc += 1;
            }
            Instruction::CharIn(meta) => {
                let mut buf = [0u8; 1];
                io::stdin().read_exact(&mut buf).map_err(|e| self.error(
                    ErrorKind::IoError { reason: e.to_string() }, meta
                ))?;
                self.stack.push(buf[0] as i64);
                *self.pc += 1;
            }

            // ── 控制流 ──────────────────────────────────────────
            Instruction::Mark { .. } => {
                // Mark 在运行时为 NOP
                *self.pc += 1;
            }
            Instruction::Jump { name, meta } => {
                let target = *self.marks.get(name)
                    .ok_or_else(|| self.error(ErrorKind::UndefinedMark { name: *name }, meta))?;
                let cond = self.pop(meta)?;
                if cond != 0 {
                    *self.pc = target;
                } else {
                    *self.pc += 1;
                }
            }
            Instruction::UnconditionalJump { name, meta } => {
                let target = *self.marks.get(name)
                    .ok_or_else(|| self.error(ErrorKind::UndefinedMark { name: *name }, meta))?;
                *self.pc = target;
            }

            // ── 函数调用 ────────────────────────────────────────
            Instruction::Call { name, argc, meta } => {
                if self.is_main {
                    // 主程序 Call: 返回调用信息让 VM 层处理
                    return Ok(Some((*name, *argc, meta.clone())));
                } else {
                    return Err(self.error(ErrorKind::CallInsideFunction, meta));
                }
            }
        }
        Ok(None)
    }
}

// ============================================================================
// VM 实现 — 生命周期管理 + 函数调用栈
// ============================================================================

impl VM {
    pub fn new(parse_result: ParseResult) -> Self {
        VM {
            main_instructions: parse_result.main_instructions,
            main_marks: parse_result.main_marks,
            functions: parse_result.functions,
            main_stack: Vec::new(),
            pc: 0,
            halted: false,
        }
    }

    /// 运行主程序直到结束或出错
    pub fn run(&mut self) -> Result<(), StardustError> {
        while !self.halted && self.pc < self.main_instructions.len() {
            // 先读取下一条指令（self.pc 是 Copy，不产生借用冲突）
            let pc = self.pc;
            let inst = self.main_instructions[pc].clone();

            let mut executor = StackExecutor::new(
                &mut self.main_stack,
                &mut self.pc,
                &self.main_marks,
                true,
            );

            match executor.execute(&inst)? {
                Some((name, argc, meta)) => {
                    // drop executor 释放对 self 的借用
                    drop(executor);
                    self.handle_function_call(name, argc, meta)?;
                    self.pc += 1;
                }
                None => { /* 指令已被 executor 处理 */ }
            }
        }
        Ok(())
    }

    /// 处理函数调用 — 创建独立栈、执行函数体、合并结果
    fn handle_function_call(
        &mut self,
        func_name: usize,
        argc: usize,
        call_meta: InstrMeta,
    ) -> Result<(), StardustError> {
        let body = self.functions.get(&func_name)
            .ok_or_else(|| StardustError::new(
                ErrorKind::UndefinedFunction { name: func_name },
                Some(call_meta.span.clone()),
            ))?
            .clone();

        // 从主栈弹出 argc 个参数
        if self.main_stack.len() < argc {
            return Err(StardustError::new(
                ErrorKind::NotEnoughArguments {
                    func: func_name,
                    expected: argc,
                    actual: self.main_stack.len(),
                },
                Some(call_meta.span.clone()),
            ));
        }

        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.main_stack.pop().unwrap());
        }
        args.reverse();
        let mut new_stack: Vec<i64> = args;

        // 解析函数体内的标志
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

        // 使用 StackExecutor 逐条执行函数体
        let mut local_pc: usize = 0;
        while local_pc < body.len() {
            let inst = body[local_pc].clone();

            let mut executor = StackExecutor::new(
                &mut new_stack,
                &mut local_pc,
                &local_marks,
                false, // 函数内不允许 Call
            );

            executor.execute(&inst)?; // 函数内 Call 会返回 CallInsideFunction 错误
        }

        // 将新栈内容合并回主栈
        self.main_stack.extend(new_stack);
        Ok(())
    }
}
