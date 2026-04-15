use std::collections::HashMap;
use std::io::{self, Read, Write};
use crate::stardust::{ErrorKind, Instruction, ParseResult, StardustError, VM};

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

    /// 构造一个不带位置信息的 StardustError
    fn error(&self, kind: ErrorKind) -> StardustError {
        StardustError::new(kind, None)
    }

    /// 运行主程序直到结束或出错
    pub fn run(&mut self) -> Result<(), StardustError> {
        while !self.halted && self.pc < self.main_instructions.len() {
            let inst = self.main_instructions[self.pc].clone();
            self.execute_instruction(inst, true)?;
        }
        Ok(())
    }

    /// 执行单条指令，is_main 表示是否在主程序中（影响 Mark 和 Jump 的上下文）
    fn execute_instruction(&mut self, inst: Instruction, is_main: bool) -> Result<(), StardustError> {
        match inst {
            Instruction::Push(val) => {
                self.main_stack.push(val);
                self.pc += 1;
            }
            Instruction::Dup => {
                let top = self.pop_main()?;
                self.main_stack.push(top);
                self.main_stack.push(top);
                self.pc += 1;
            }
            Instruction::Swap => {
                let a = self.pop_main()?;
                let b = self.pop_main()?;
                self.main_stack.push(a);
                self.main_stack.push(b);
                self.pc += 1;
            }
            Instruction::Rotate => {
                let c = self.pop_main()?;
                let b = self.pop_main()?;
                let a = self.pop_main()?;
                self.main_stack.push(c);
                self.main_stack.push(a);
                self.main_stack.push(b);
                self.pc += 1;
            }
            Instruction::Pop => {
                self.pop_main()?;
                self.pc += 1;
            }
            Instruction::Add => {
                let b = self.pop_main()?;
                let a = self.pop_main()?;
                self.main_stack.push(a + b);
                self.pc += 1;
            }
            Instruction::Sub => {
                let b = self.pop_main()?;
                let a = self.pop_main()?;
                self.main_stack.push(a - b);
                self.pc += 1;
            }
            Instruction::Mul => {
                let b = self.pop_main()?;
                let a = self.pop_main()?;
                self.main_stack.push(a * b);
                self.pc += 1;
            }
            Instruction::Div => {
                let b = self.pop_main()?;
                let a = self.pop_main()?;
                if b == 0 {
                    return Err(self.error(ErrorKind::DivisionByZero));
                }
                self.main_stack.push(a / b);
                self.pc += 1;
            }
            Instruction::Mod => {
                let b = self.pop_main()?;
                let a = self.pop_main()?;
                if b == 0 {
                    return Err(self.error(ErrorKind::ModuloByZero));
                }
                self.main_stack.push(a % b);
                self.pc += 1;
            }
            Instruction::Reverse => {
                self.main_stack.reverse();
                self.pc += 1;
            }
            Instruction::NumOut => {
                let val = self.pop_main()?;
                print!("{}", val);
                io::stdout().flush().map_err(|e| self.error(ErrorKind::IoError { reason: e.to_string() }))?;
                self.pc += 1;
            }
            Instruction::NumIn => {
                let mut input = String::new();
                io::stdin().read_line(&mut input).map_err(|e| self.error(ErrorKind::IoError { reason: e.to_string() }))?;
                let val: i64 = input.trim().parse().map_err(|_| self.error(ErrorKind::InvalidIntegerInput))?;
                self.main_stack.push(val);
                self.pc += 1;
            }
            Instruction::CharOut => {
                let val = self.pop_main()?;
                if val < 0 || val > 127 {
                    return Err(self.error(ErrorKind::InvalidAscii { value: val }));
                }
                print!("{}", val as u8 as char);
                io::stdout().flush().map_err(|e| self.error(ErrorKind::IoError { reason: e.to_string() }))?;
                self.pc += 1;
            }
            Instruction::CharIn => {
                let mut buf = [0u8; 1];
                io::stdin().read_exact(&mut buf).map_err(|e| self.error(ErrorKind::IoError { reason: e.to_string() }))?;
                self.main_stack.push(buf[0] as i64);
                self.pc += 1;
            }
            Instruction::Mark { .. } if is_main => {
                // 主程序中的 Mark 仅在解析时记录位置，运行时视为无操作
                self.pc += 1;
            }
            Instruction::Jump { name } if is_main => {
                let target = *self.main_marks.get(&name)
                    .ok_or_else(|| self.error(ErrorKind::UndefinedMark { name }))?;
                self.pc = target;
            }
            Instruction::Call { name, argc } if is_main => {
                self.handle_function_call(name, argc)?;
                self.pc += 1;
            }
            _ => {
                return Err(self.error(ErrorKind::InvalidInstructionContext));
            }
        }
        Ok(())
    }

    fn pop_main(&mut self) -> Result<i64, StardustError> {
        self.main_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))
    }

    /// 处理函数调用
    fn handle_function_call(&mut self, func_name: usize, argc: usize) -> Result<(), StardustError> {
        // 获取函数体
        let body = self.functions.get(&func_name)
            .ok_or_else(|| self.error(ErrorKind::UndefinedFunction { name: func_name }))?
            .clone();

        // 从主栈弹出 argc 个参数
        if self.main_stack.len() < argc {
            return Err(self.error(ErrorKind::NotEnoughArguments {
                func: func_name,
                expected: argc,
                actual: self.main_stack.len(),
            }));
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
            if let Instruction::Mark { name } = inst {
                if local_marks.insert(*name, idx).is_some() {
                    return Err(self.error(ErrorKind::DuplicateMark { name: *name }));
                }
            }
        }

        // 执行函数体
        let mut local_pc = 0;
        while local_pc < body.len() {
            let inst = &body[local_pc];
            match inst {
                Instruction::Push(val) => {
                    new_stack.push(*val);
                    local_pc += 1;
                }
                Instruction::Dup => {
                    let top = *new_stack.last().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    new_stack.push(top);
                    local_pc += 1;
                }
                Instruction::Swap => {
                    if new_stack.len() < 2 {
                        return Err(self.error(ErrorKind::StackUnderflow));
                    }
                    let a = new_stack.pop().unwrap();
                    let b = new_stack.pop().unwrap();
                    new_stack.push(a);
                    new_stack.push(b);
                    local_pc += 1;
                }
                Instruction::Rotate => {
                    if new_stack.len() < 3 {
                        return Err(self.error(ErrorKind::StackUnderflow));
                    }
                    let c = new_stack.pop().unwrap();
                    let b = new_stack.pop().unwrap();
                    let a = new_stack.pop().unwrap();
                    new_stack.push(c);
                    new_stack.push(a);
                    new_stack.push(b);
                    local_pc += 1;
                }
                Instruction::Pop => {
                    new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    local_pc += 1;
                }
                Instruction::Add => {
                    let b = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    let a = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    new_stack.push(a + b);
                    local_pc += 1;
                }
                Instruction::Sub => {
                    let b = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    let a = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    new_stack.push(a - b);
                    local_pc += 1;
                }
                Instruction::Mul => {
                    let b = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    let a = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    new_stack.push(a * b);
                    local_pc += 1;
                }
                Instruction::Div => {
                    let b = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    let a = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    if b == 0 {
                        return Err(self.error(ErrorKind::DivisionByZero));
                    }
                    new_stack.push(a / b);
                    local_pc += 1;
                }
                Instruction::Mod => {
                    let b = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    let a = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    if b == 0 {
                        return Err(self.error(ErrorKind::ModuloByZero));
                    }
                    new_stack.push(a % b);
                    local_pc += 1;
                }
                Instruction::Reverse => {
                    new_stack.reverse();
                    local_pc += 1;
                }
                Instruction::NumOut => {
                    let val = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    print!("{}", val);
                    io::stdout().flush().map_err(|e| self.error(ErrorKind::IoError { reason: e.to_string() }))?;
                    local_pc += 1;
                }
                Instruction::NumIn => {
                    let mut input = String::new();
                    io::stdin().read_line(&mut input).map_err(|e| self.error(ErrorKind::IoError { reason: e.to_string() }))?;
                    let val: i64 = input.trim().parse().map_err(|_| self.error(ErrorKind::InvalidIntegerInput))?;
                    new_stack.push(val);
                    local_pc += 1;
                }
                Instruction::CharOut => {
                    let val = new_stack.pop().ok_or_else(|| self.error(ErrorKind::StackUnderflow))?;
                    if val < 0 || val > 127 {
                        return Err(self.error(ErrorKind::InvalidAscii { value: val }));
                    }
                    print!("{}", val as u8 as char);
                    io::stdout().flush().map_err(|e| self.error(ErrorKind::IoError { reason: e.to_string() }))?;
                    local_pc += 1;
                }
                Instruction::CharIn => {
                    let mut buf = [0u8; 1];
                    io::stdin().read_exact(&mut buf).map_err(|e| self.error(ErrorKind::IoError { reason: e.to_string() }))?;
                    new_stack.push(buf[0] as i64);
                    local_pc += 1;
                }
                Instruction::Mark { .. } => {
                    local_pc += 1;
                }
                Instruction::Jump { name } => {
                    let target = *local_marks.get(name)
                        .ok_or_else(|| self.error(ErrorKind::UndefinedMark { name: *name }))?;
                    local_pc = target;
                }
                Instruction::Call { .. } => {
                    return Err(self.error(ErrorKind::CallInsideFunction));
                }
                _ => {
                    return Err(self.error(ErrorKind::InvalidInstructionContext));
                }
            }
        }

        // 将新栈内容合并回主栈
        self.main_stack.extend(new_stack);
        Ok(())
    }
}
