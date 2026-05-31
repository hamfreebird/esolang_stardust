//! REPL 执行上下文
//!
//! 维护跨 REPL 行的持久 VM 状态（栈、堆、函数表）。
//! 每次用户输入创建微型 VM，注入持久状态，执行后归还。

use crate::stardust::{Instruction, ParseResult, VM};
use std::collections::HashMap;

/// REPL 持久状态
pub struct ReplContext {
    /// 跨行持久栈
    pub stack: Vec<i64>,
    /// 跨行持久堆
    pub heap: HashMap<i64, i64>,
    /// 用户通过 :func 定义的函数
    pub functions: HashMap<usize, Vec<Instruction>>,
}

impl ReplContext {
    pub fn new() -> Self {
        ReplContext {
            stack: Vec::new(),
            heap: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.stack.clear();
        self.heap.clear();
    }

    pub fn define_function(&mut self, name: usize, body: Vec<Instruction>) {
        self.functions.insert(name, body);
    }

    pub fn stack_depth(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty() && self.heap.is_empty()
    }
}

/// 在 REPL 上下文中执行指令序列
///
/// 策略：
/// 1. 构建临时 ParseResult（含当前指令 + 持久函数表）
/// 2. 创建微型 VM，注入持久栈和堆
/// 3. 执行
/// 4. 归还栈和堆到上下文
pub fn execute_repl(
    ctx: &mut ReplContext,
    instructions: Vec<Instruction>,
) -> Result<(), String> {
    if instructions.is_empty() {
        return Ok(());
    }

    // 扫描 Marks
    let mut marks: HashMap<usize, usize> = HashMap::new();
    for (idx, inst) in instructions.iter().enumerate() {
        if let Instruction::Mark { name, .. } = inst {
            marks.insert(*name, idx);
        }
    }

    let parse_result = ParseResult {
        main_instructions: instructions,
        main_marks: marks,
        functions: ctx.functions.clone(),
    };

    let mut vm = VM::new(parse_result);

    // 注入持久状态
    vm.set_main_stack(ctx.stack.clone());
    vm.set_heap(ctx.heap.clone());

    // 执行
    vm.run().map_err(|e| e.message)?;

    // 归还状态
    ctx.stack = vm.take_main_stack();
    ctx.heap = vm.take_heap();

    Ok(())
}

// ── 测试 ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stardust::{InstrMeta, Instruction};

    fn meta() -> InstrMeta {
        InstrMeta::default()
    }

    #[test]
    fn execute_push_persists_stack() {
        let mut ctx = ReplContext::new();
        let insts = vec![Instruction::Push(42, meta())];
        execute_repl(&mut ctx, insts).unwrap();
        assert_eq!(ctx.stack, vec![42]);
    }

    #[test]
    fn execute_multiple_lines_stack_persists() {
        let mut ctx = ReplContext::new();

        // Line 1: Push(10) Push(20)
        execute_repl(
            &mut ctx,
            vec![Instruction::Push(10, meta()), Instruction::Push(20, meta())],
        )
        .unwrap();
        assert_eq!(ctx.stack, vec![10, 20]);

        // Line 2: Add → stack becomes [30]
        execute_repl(&mut ctx, vec![Instruction::Add(meta())]).unwrap();
        assert_eq!(ctx.stack, vec![30]);
    }

    #[test]
    fn execute_charout_does_not_crash() {
        let mut ctx = ReplContext::new();
        let insts = vec![Instruction::Push(65, meta()), Instruction::CharOut(meta())];
        execute_repl(&mut ctx, insts).unwrap();
        assert!(ctx.stack.is_empty()); // CharOut pops
    }

    #[test]
    fn execute_heap_persists() {
        let mut ctx = ReplContext::new();
        // Push(99) Push(0) Store → heap[0] = 99
        execute_repl(
            &mut ctx,
            vec![
                Instruction::Push(99, meta()),
                Instruction::Push(0, meta()),
                Instruction::Store(meta()),
            ],
        )
        .unwrap();
        assert_eq!(ctx.heap.get(&0), Some(&99));
    }

    #[test]
    fn execute_with_function() {
        let mut ctx = ReplContext::new();
        ctx.define_function(
            1,
            vec![Instruction::Push(65, meta()), Instruction::CharOut(meta())],
        );
        // Call(1, 0)
        let insts = vec![Instruction::Call {
            name: 1,
            argc: 0,
            meta: meta(),
        }];
        execute_repl(&mut ctx, insts).unwrap();
        // Function 1 pushes 65, CharOut pops it → empty stack
        assert!(ctx.stack.is_empty());
    }

    #[test]
    fn execute_clear_resets_state() {
        let mut ctx = ReplContext::new();
        execute_repl(&mut ctx, vec![Instruction::Push(99, meta())]).unwrap();
        assert!(!ctx.stack.is_empty());
        ctx.clear();
        assert!(ctx.stack.is_empty());
        assert!(ctx.heap.is_empty());
    }

    #[test]
    fn execute_empty_instructions_is_noop() {
        let mut ctx = ReplContext::new();
        execute_repl(&mut ctx, vec![]).unwrap();
        assert!(ctx.stack.is_empty());
    }

    #[test]
    fn execute_error_does_not_crash() {
        let mut ctx = ReplContext::new();
        // Pop on empty stack → error
        let r = execute_repl(&mut ctx, vec![Instruction::Pop(meta())]);
        assert!(r.is_err());
    }
}
