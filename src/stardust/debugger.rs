//! 交互式调试器
//!
//! 支持单步执行、断点管理、状态检查。
//! 所有调试器输出写入 stderr，保持 stdout 整洁。

use crate::stardust::{InstrMeta, Instruction, VM};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

/// 调试器执行动作
pub enum DebugAction {
    Continue,
    Step,
    Quit,
}

/// 交互式调试器
pub struct Debugger {
    breakpoints: HashSet<usize>,
    single_step: bool,
}

impl Debugger {
    pub fn new() -> Self {
        Debugger {
            breakpoints: HashSet::new(),
            single_step: true, // 进入调试模式立即暂停
        }
    }

    pub fn is_single_step(&self) -> bool {
        self.single_step
    }

    /// 判断当前是否应该暂停
    pub fn should_break(
        &self,
        pc: usize,
        marks: &HashMap<usize, usize>,
        inst: &Instruction,
    ) -> bool {
        if self.single_step {
            return true;
        }
        if matches!(inst, Instruction::Breakpoint(_)) {
            return true;
        }
        for bp_name in &self.breakpoints {
            if let Some(&target_pc) = marks.get(bp_name) {
                if target_pc == pc {
                    return true;
                }
            }
        }
        false
    }

    /// 进入交互式 REPL
    pub fn interact(&mut self, vm: &VM) -> Result<DebugAction, io::Error> {
        self.print_state(vm);
        loop {
            eprint!("(sd-dbg) ");
            io::stderr().flush()?;

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                return Ok(DebugAction::Quit);
            }
            let cmd = input.trim();

            match cmd {
                // 执行控制
                "s" | "step" => {
                    self.single_step = true;
                    return Ok(DebugAction::Continue);
                }
                "c" | "continue" => {
                    self.single_step = false;
                    return Ok(DebugAction::Continue);
                }
                "q" | "quit" => return Ok(DebugAction::Quit),

                // 断点管理
                cmd if cmd.starts_with("b ") || cmd.starts_with("break ") => {
                    if let Some(ns) = cmd.split_whitespace().nth(1) {
                        if let Ok(n) = ns.parse::<usize>() {
                            self.breakpoints.insert(n);
                            eprintln!("Breakpoint set at Mark({}).", n);
                        }
                    }
                }
                "lb" => {
                    if self.breakpoints.is_empty() {
                        eprintln!("No breakpoints.");
                    } else {
                        let mut v: Vec<_> = self.breakpoints.iter().collect();
                        v.sort();
                        eprintln!("Breakpoints: {:?}", v);
                    }
                }
                cmd if cmd.starts_with("db ") => {
                    if let Some(ns) = cmd.split_whitespace().nth(1) {
                        if let Ok(n) = ns.parse::<usize>() {
                            self.breakpoints.remove(&n);
                            eprintln!("Removed breakpoint at Mark({}).", n);
                        }
                    }
                }

                // 状态查看
                "p" | "stack" => self.print_stack(vm),
                "hp" | "heap" => self.print_heap(vm),
                "i" | "info" => self.print_info(vm),
                "l" | "list" => self.print_nearby(vm, 8),
                "pc" => self.print_pc(vm),
                "bt" | "frames" => self.print_frames(vm),

                // 帮助
                "h" | "help" => self.print_help(),

                "" => {}
                _ => eprintln!("Unknown '{}'. Type 'h' for help.", cmd),
            }
        }
    }

    // ── 展示方法 ───────────────────────────────────────

    fn print_state(&self, vm: &VM) {
        let frame = vm.current_frame();
        let fname = match vm.current_frame_index() {
            0 => "main".into(),
            n => format!("func #{}", n),
        };
        let next = if frame.pc < frame.instructions.len() {
            format_inst(&frame.instructions[frame.pc])
        } else {
            "(end)".into()
        };
        let loc = if frame.pc < frame.instructions.len() {
            let m = inst_meta(&frame.instructions[frame.pc]);
            format!("line {}, col {}", m.span.line, m.span.column)
        } else {
            "-".into()
        };

        eprintln!();
        eprintln!("── Stardust Debugger ──────────────────────────────");
        eprintln!(
            "  PC: {}  Frame: {}  Stack: {}  Heap: {}",
            frame.pc,
            fname,
            frame.stack.len(),
            vm.heap_len(),
        );
        eprintln!("  Next: {}  [{}]", next, loc);
        eprintln!("───────────────────────────────────────────────────");
    }

    fn print_stack(&self, vm: &VM) {
        let stack = vm.current_stack();
        if stack.is_empty() {
            eprintln!("  Stack: (empty)");
        } else {
            let items: Vec<String> = stack.iter().rev().map(|v| v.to_string()).collect();
            eprintln!("  Stack (top→bottom): [{}]", items.join(", "));
        }
    }

    fn print_heap(&self, vm: &VM) {
        let entries = vm.heap_entries();
        if entries.is_empty() {
            eprintln!("  Heap: (empty)");
            return;
        }
        eprintln!("  Heap ({} entries):", entries.len());
        let mut sorted: Vec<_> = entries.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted.iter().take(50) {
            eprintln!("    [{}] = {}", k, v);
        }
        if sorted.len() > 50 {
            eprintln!("    ... ({} more)", sorted.len() - 50);
        }
    }

    fn print_info(&self, vm: &VM) {
        let frame = vm.current_frame();
        eprintln!("  PC: {}", frame.pc);
        eprintln!("  Stack depth: {}", frame.stack.len());
        eprintln!("  Heap entries: {}", vm.heap_len());
        eprintln!("  Call frames: {}", vm.all_frames().len());
        eprintln!("  Functions defined: {}", vm.function_count());
    }

    fn print_nearby(&self, vm: &VM, radius: usize) {
        let frame = vm.current_frame();
        let pc = frame.pc;
        let start = pc.saturating_sub(radius);
        let end = (pc + radius + 1).min(frame.instructions.len());

        eprintln!("  Instructions around PC {}:", pc);
        for i in start..end {
            let mark = if i == pc { "→" } else { " " };
            let marks_at: Vec<String> = frame
                .marks
                .iter()
                .filter(|(_, pos)| **pos == i)
                .map(|(n, _)| format!("Mark({})", n))
                .collect();
            let mark_str = if marks_at.is_empty() {
                String::new()
            } else {
                format!("  ; {}", marks_at.join(", "))
            };
            eprintln!(
                "  {} {:4}: {}{}",
                mark,
                i,
                format_inst(&frame.instructions[i]),
                mark_str
            );
        }
    }

    fn print_pc(&self, vm: &VM) {
        let frame = vm.current_frame();
        if frame.pc < frame.instructions.len() {
            eprintln!("  PC: {}  {}", frame.pc, format_inst(&frame.instructions[frame.pc]));
        } else {
            eprintln!("  PC: {}  (end)", frame.pc);
        }
    }

    fn print_frames(&self, vm: &VM) {
        let frames = vm.all_frames();
        eprintln!("  Call stack ({} frames):", frames.len());
        for (i, f) in frames.iter().enumerate() {
            let label = if i == 0 { "main" } else { "func" };
            eprintln!(
                "    #{} {}: PC={}/{}, depth={}",
                i,
                label,
                f.pc,
                f.instructions.len(),
                f.stack.len(),
            );
        }
    }

    fn print_help(&self) {
        eprintln!("Debugger commands:");
        eprintln!("  s, step        Single-step one instruction");
        eprintln!("  c, continue    Run to next breakpoint or end");
        eprintln!("  b <n>          Set breakpoint at Mark(n)");
        eprintln!("  lb             List breakpoints");
        eprintln!("  db <n>         Delete breakpoint at Mark(n)");
        eprintln!("  p, stack       Print current stack");
        eprintln!("  hp, heap       Print heap contents");
        eprintln!("  i, info        Print VM state");
        eprintln!("  l, list        Show instructions around PC");
        eprintln!("  pc             Show PC and next instruction");
        eprintln!("  bt, frames     Show call stack");
        eprintln!("  q, quit        Terminate program");
        eprintln!("  h, help        Show this help");
    }
}

// ── 辅助函数 ───────────────────────────────────────────────

pub fn format_inst(inst: &Instruction) -> String {
    match inst {
        Instruction::Push(n, _) => format!("Push({})", n),
        Instruction::Dup(_) => "Dup".into(),
        Instruction::Swap(_) => "Swap".into(),
        Instruction::Rotate(_) => "Rotate".into(),
        Instruction::Pop(_) => "Pop".into(),
        Instruction::Add(_) => "Add".into(),
        Instruction::Sub(_) => "Sub".into(),
        Instruction::Mul(_) => "Mul".into(),
        Instruction::Div(_) => "Div".into(),
        Instruction::Mod(_) => "Mod".into(),
        Instruction::Reverse(_) => "Reverse".into(),
        Instruction::NumOut(_) => "NumOut".into(),
        Instruction::NumIn(_) => "NumIn".into(),
        Instruction::CharOut(_) => "CharOut".into(),
        Instruction::CharIn(_) => "CharIn".into(),
        Instruction::Mark { name, .. } => format!("Mark({})", name),
        Instruction::Jump { name, .. } => format!("Jump({})", name),
        Instruction::UnconditionalJump { name, .. } => format!("UncondJump({})", name),
        Instruction::Call { name, argc, .. } => format!("Call(func={}, argc={})", name, argc),
        Instruction::Eq(_) => "Eq".into(),
        Instruction::Ne(_) => "Ne".into(),
        Instruction::Lt(_) => "Lt".into(),
        Instruction::Gt(_) => "Gt".into(),
        Instruction::Le(_) => "Le".into(),
        Instruction::Ge(_) => "Ge".into(),
        Instruction::And(_) => "And".into(),
        Instruction::Or(_) => "Or".into(),
        Instruction::Not(_) => "Not".into(),
        Instruction::Xor(_) => "Xor".into(),
        Instruction::Store(_) => "Store".into(),
        Instruction::Load(_) => "Load".into(),
        Instruction::ShiftL(_) => "ShiftL".into(),
        Instruction::Depth(_) => "Depth".into(),
        Instruction::Pick(_) => "Pick".into(),
        Instruction::ShiftR(_) => "ShiftR".into(),
        Instruction::DropN(_) => "DropN".into(),
        Instruction::DumpStack(_) => "DumpStack".into(),
        Instruction::DumpState(_) => "DumpState".into(),
        Instruction::Breakpoint(_) => "Breakpoint".into(),
    }
}

pub fn inst_meta(inst: &Instruction) -> &InstrMeta {
    match inst {
        Instruction::Push(_, m) => m,
        Instruction::Dup(m) => m,
        Instruction::Swap(m) => m,
        Instruction::Rotate(m) => m,
        Instruction::Pop(m) => m,
        Instruction::Add(m) => m,
        Instruction::Sub(m) => m,
        Instruction::Mul(m) => m,
        Instruction::Div(m) => m,
        Instruction::Mod(m) => m,
        Instruction::Reverse(m) => m,
        Instruction::NumOut(m) => m,
        Instruction::NumIn(m) => m,
        Instruction::CharOut(m) => m,
        Instruction::CharIn(m) => m,
        Instruction::Mark { meta: m, .. } => m,
        Instruction::Jump { meta: m, .. } => m,
        Instruction::UnconditionalJump { meta: m, .. } => m,
        Instruction::Call { meta: m, .. } => m,
        Instruction::Eq(m) => m,
        Instruction::Ne(m) => m,
        Instruction::Lt(m) => m,
        Instruction::Gt(m) => m,
        Instruction::Le(m) => m,
        Instruction::Ge(m) => m,
        Instruction::And(m) => m,
        Instruction::Or(m) => m,
        Instruction::Not(m) => m,
        Instruction::Xor(m) => m,
        Instruction::Store(m) => m,
        Instruction::Load(m) => m,
        Instruction::ShiftL(m) => m,
        Instruction::Depth(m) => m,
        Instruction::Pick(m) => m,
        Instruction::ShiftR(m) => m,
        Instruction::DropN(m) => m,
        Instruction::DumpStack(m) => m,
        Instruction::DumpState(m) => m,
        Instruction::Breakpoint(m) => m,
    }
}
