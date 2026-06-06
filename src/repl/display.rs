//! REPL 状态展示工具
//!
//! 格式化栈、堆、VM 状态以便向用户展示。

use crate::stardust::CallFrame;
use std::collections::HashMap;

/// 格式化栈内容
pub fn format_stack(stack: &[i64], max_items: usize) -> String {
    if stack.is_empty() {
        return "  Stack: (empty)".into();
    }
    let limit = max_items.min(stack.len());
    let items: Vec<String> = stack.iter().rev().take(limit).map(|v| v.to_string()).collect();
    let mut s = format!("  Stack (top→bottom): [{}]", items.join(", "));
    if stack.len() > max_items {
        s.push_str(&format!(" ... ({} more)", stack.len() - max_items));
    }
    s
}

/// 格式化堆内容
pub fn format_heap(heap: &HashMap<i64, i64>, max_items: usize) -> String {
    if heap.is_empty() {
        return "  Heap: (empty)".into();
    }
    let mut entries: Vec<_> = heap.iter().collect();
    entries.sort_by_key(|(k, _)| *k);
    let mut lines: Vec<String> = vec![format!("  Heap ({} entries):", entries.len())];
    for (k, v) in entries.iter().take(max_items) {
        lines.push(format!("    [{}] = {}", k, v));
    }
    if entries.len() > max_items {
        lines.push(format!("    ... ({} more)", entries.len() - max_items));
    }
    lines.join("\n")
}

/// 格式化 VM 简要信息
pub fn format_vm_info(
    pc: usize,
    stack_depth: usize,
    heap_size: usize,
    frame_count: usize,
    func_count: usize,
) -> String {
    format!(
        "  PC: {}  Stack: {}  Heap: {}  Frames: {}  Functions: {}",
        pc, stack_depth, heap_size, frame_count, func_count
    )
}

/// 格式化指令列表（PC 附近）
pub fn format_nearby(frame: &CallFrame, radius: usize) -> String {
    let pc = frame.pc;
    let start = pc.saturating_sub(radius);
    let end = (pc + radius + 1).min(frame.instructions.len());

    let mut lines: Vec<String> = vec![format!("  Instructions around PC {}:", pc)];
    for i in start..end {
        let mark = if i == pc { "→" } else { " " };
        let marks_here: Vec<String> = frame
            .marks
            .iter()
            .filter(|(_, pos)| **pos == i)
            .map(|(n, _)| format!("Mark({})", n))
            .collect();
        let suffix = if marks_here.is_empty() {
            String::new()
        } else {
            format!("  ; {}", marks_here.join(", "))
        };
        lines.push(format!("  {} {:4}: {}{}", mark, i, frame.instructions[i].format(), suffix));
    }
    lines.join("\n")
}

/// 格式化调用栈
pub fn format_frames(frames: &[CallFrame]) -> String {
    let mut lines = vec![format!("  Call stack ({} frames):", frames.len())];
    for (i, f) in frames.iter().enumerate() {
        let label = if i == 0 { "main" } else { "func" };
        lines.push(format!(
            "    #{} {}: PC={}/{}, depth={}",
            i,
            label,
            f.pc,
            f.instructions.len(),
            f.stack.len(),
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stardust::{InstrMeta, Instruction};
    use std::collections::HashMap;
    use std::rc::Rc;

    fn meta() -> InstrMeta {
        InstrMeta::default()
    }

    // ── format_stack ─────────────────────────────────────────────

    #[test]
    fn format_stack_empty() {
        assert_eq!(format_stack(&[], 10), "  Stack: (empty)");
    }

    #[test]
    fn format_stack_nonempty() {
        let stack = vec![1, 2, 3];
        let result = format_stack(&stack, 10);
        assert!(result.contains("3, 2, 1")); // reversed order
    }

    #[test]
    fn format_stack_truncation() {
        let stack: Vec<i64> = (0..20).collect();
        let result = format_stack(&stack, 5);
        assert!(result.contains("... (15 more)"));
    }

    // ── format_heap ──────────────────────────────────────────────

    #[test]
    fn format_heap_empty() {
        assert_eq!(format_heap(&HashMap::new(), 10), "  Heap: (empty)");
    }

    #[test]
    fn format_heap_sorted_by_key() {
        let mut heap = HashMap::new();
        heap.insert(10, 100);
        heap.insert(5, 200);
        let result = format_heap(&heap, 10);
        let pos5 = result.find("[5]").unwrap();
        let pos10 = result.find("[10]").unwrap();
        assert!(pos5 < pos10); // keys are sorted
    }

    #[test]
    fn format_heap_truncation() {
        let mut heap = HashMap::new();
        for i in 0..10 {
            heap.insert(i, i * 10);
        }
        let result = format_heap(&heap, 3);
        assert!(result.contains("... (7 more)"));
    }

    // ── format_vm_info ───────────────────────────────────────────

    #[test]
    fn format_vm_info_basic() {
        let result = format_vm_info(42, 5, 3, 1, 2);
        assert!(result.contains("PC: 42"));
        assert!(result.contains("Stack: 5"));
        assert!(result.contains("Heap: 3"));
        assert!(result.contains("Frames: 1"));
        assert!(result.contains("Functions: 2"));
    }

    // ── format_nearby ────────────────────────────────────────────

    #[test]
    fn format_nearby_shows_current_pc_marker() {
        let frame = CallFrame::new(
            Rc::new(vec![Instruction::Push(1, meta()), Instruction::Add(meta())]),
            HashMap::new(),
        );
        let result = format_nearby(&frame, 3);
        assert!(result.contains("→")); // PC marker
    }

    #[test]
    fn format_nearby_shows_marks() {
        let mut marks = HashMap::new();
        marks.insert(7, 1usize);
        let mut frame = CallFrame::new(
            Rc::new(vec![
                Instruction::Push(1, meta()),
                Instruction::Mark { name: 7, meta: meta() },
            ]),
            marks,
        );
        frame.pc = 1;
        let result = format_nearby(&frame, 3);
        assert!(result.contains("Mark(7)"));
    }

    // ── format_frames ────────────────────────────────────────────

    #[test]
    fn format_frames_single() {
        let frame = CallFrame::new(Rc::new(vec![Instruction::Push(1, meta())]), HashMap::new());
        let result = format_frames(&[frame]);
        assert!(result.contains("1 frames"));
        assert!(result.contains("#0 main"));
    }

    #[test]
    fn format_frames_multiple() {
        let f1 = CallFrame::new(Rc::new(vec![Instruction::Push(1, meta())]), HashMap::new());
        let f2 = CallFrame::new(Rc::new(vec![Instruction::Add(meta())]), HashMap::new());
        let result = format_frames(&[f1, f2]);
        assert!(result.contains("2 frames"));
        assert!(result.contains("#0 main"));
        assert!(result.contains("#1 func"));
    }

    // ── repl_help ────────────────────────────────────────────────

    #[test]
    fn repl_help_nonempty() {
        assert!(!repl_help(false).is_empty());
        assert!(!repl_help(true).is_empty());
    }

    #[test]
    fn debug_help_has_more_commands() {
        assert!(repl_help(true).len() > repl_help(false).len());
    }

    #[test]
    fn debug_help_contains_step_command() {
        assert!(repl_help(true).contains("step"));
        assert!(!repl_help(false).contains("step"));
    }
}

/// REPL 帮助文本
pub fn repl_help(debug_mode: bool) -> &'static str {
    if debug_mode {
        "\
Commands (prefix with ':'):
  :h, :help         Show this help
  :q, :quit         Exit REPL
  :s, :stack        Print stack
  :hp, :heap        Print heap
  :i, :info         Print VM state
  :c, :clear        Clear stack and heap
  :l, :load <file>  Load and run a .sd file
  :f, :func <n> <body>  Define function n
  :funcs            List defined functions
  :raw              Toggle raw .sd input mode
  :history          Show command history
  ── Debugger ──
  :st, :step        Single-step one instruction
  :co, :continue    Run to next breakpoint or end
  :b, :break <n>    Set breakpoint at Mark(n)
  :lb, :breaks      List breakpoints
  :db <n>           Delete breakpoint at Mark(n)
  :list [n]         Show instructions around PC
  :pc               Show PC and next instruction
  :bt               Show call stack"
    } else {
        "\
Commands (prefix with ':'):
  :h, :help         Show this help
  :q, :quit         Exit REPL
  :s, :stack        Print stack
  :hp, :heap        Print heap
  :i, :info         Print VM state
  :c, :clear        Clear stack and heap
  :l, :load <file>  Load and run a .sd file
  :f, :func <n> <body>  Define function n
  :funcs            List defined functions
  :raw              Toggle raw .sd input mode
  :history          Show command history"
    }
}
