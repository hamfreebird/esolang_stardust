//! REPL 状态展示工具
//!
//! 格式化栈、堆、VM 状态以便向用户展示。

use crate::stardust::CallFrame;
use crate::stardust::debugger::format_inst;
use std::collections::HashMap;

/// 格式化栈内容
pub fn format_stack(stack: &[i64], max_items: usize) -> String {
    if stack.is_empty() {
        return "  Stack: (empty)".into();
    }
    let limit = max_items.min(stack.len());
    let items: Vec<String> = stack
        .iter()
        .rev()
        .take(limit)
        .map(|v| v.to_string())
        .collect();
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
        lines.push(format!(
            "  {} {:4}: {}{}",
            mark,
            i,
            format_inst(&frame.instructions[i]),
            suffix
        ));
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
