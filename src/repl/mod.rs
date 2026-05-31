//! Stardust REPL — 交互式 Read-Eval-Print Loop
//!
//! 两种模式：
//! - 普通 REPL (`stardust -r`)：交互输入 Stardust 指令
//! - 调试 REPL (`stardust -r -d [file]`)：加载文件并逐指令调试
//!
//! 支持简写语法和原始 .sd 语法自动检测。

pub mod display;
pub mod executor;
pub mod parser;

use crate::stardust::debugger::Debugger;
use crate::stardust::lexer::tokenize;
use crate::stardust::parser::parse_program;
use executor::ReplContext;
use std::io::{self, Write};
use std::fs;

/// REPL 入口
pub fn repl_main(debug_mode: bool, file_to_load: Option<&str>) {
    let mut ctx = ReplContext::new();
    let mut debugger = if debug_mode {
        Some(Debugger::new())
    } else {
        None
    };
    let mut history: Vec<String> = Vec::new();
    let mut raw_mode = false;

    // 欢迎信息
    eprintln!(
        "Stardust REPL {} {}",
        env!("CARGO_PKG_VERSION"),
        if debug_mode { "[debug mode]" } else { "" }
    );
    eprintln!("Type Stardust instructions or :help for help.");
    eprintln!("─────────────────────────────────────────────────");

    // 加载文件
    if let Some(file) = file_to_load {
        eprintln!("Loading '{}'...", file);
        match load_and_execute_file(&mut ctx, file) {
            Ok(()) => eprintln!("Loaded successfully."),
            Err(e) => eprintln!("Load error: {}", e),
        }
    }

    // 主循环
    loop {
        // 提示符
        let depth = ctx.stack_depth();
        if depth > 0 {
            eprint!("sd[{}]> ", depth);
        } else {
            eprint!("sd> ");
        }
        if io::stderr().flush().is_err() {
            break;
        }

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(_) => break,
        }
        let input = input.trim().to_string();
        if input.is_empty() {
            continue;
        }

        // 元命令
        if input.starts_with(':') {
            match handle_meta(
                &input,
                &mut ctx,
                &mut debugger,
                &mut raw_mode,
                &history,
            ) {
                MetaAction::Quit => break,
                MetaAction::Error(msg) => eprintln!("Error: {}", msg),
                _ => {}
            }
            continue;
        }

        // 解析
        let instructions = match parse_input(&input, raw_mode) {
            Ok(insts) => insts,
            Err(e) => {
                eprintln!("Parse error: {}", e);
                continue;
            }
        };

        if instructions.is_empty() {
            continue;
        }

        history.push(input.clone());

        // 执行
        match executor::execute_repl(&mut ctx, instructions) {
            Ok(()) => {
                show_stack_status(&ctx);
            }
            Err(e) => {
                eprintln!("Runtime error: {}", e);
            }
        }
    }

    eprintln!("Goodbye!");
}

// ── 输入解析 ────────────────────────────────────────────────

enum InputMode {
    Shorthand,
    Raw,
}

fn detect_mode(input: &str) -> InputMode {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return InputMode::Shorthand;
    }
    match trimmed.chars().next().unwrap() {
        'a'..='z' | 'A'..='Z' | '"' => InputMode::Shorthand,
        _ => InputMode::Raw,
    }
}

fn parse_input(
    input: &str,
    force_raw: bool,
) -> Result<Vec<crate::stardust::Instruction>, String> {
    let mode = if force_raw {
        InputMode::Raw
    } else {
        detect_mode(input)
    };

    match mode {
        InputMode::Shorthand => {
            parser::parse_shorthand(input).map_err(|e| e.to_string())
        }
        InputMode::Raw => {
            let tokens = tokenize(input).map_err(|e| e.message)?;
            let parsed = parse_program(tokens).map_err(|e| e.message)?;
            Ok(parsed.main_instructions)
        }
    }
}

// ── 元命令处理 ──────────────────────────────────────────────

enum MetaAction {
    Ok,
    Quit,
    Error(String),
}

fn handle_meta(
    input: &str,
    ctx: &mut ReplContext,
    debugger: &mut Option<Debugger>,
    raw_mode: &mut bool,
    history: &[String],
) -> MetaAction {
    let cmd = input.trim_start_matches(':');
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    let name = parts[0];
    let arg = parts.get(1).copied().unwrap_or("");

    match name {
        "h" | "help" => {
            eprintln!("{}", display::repl_help(debugger.is_some()));
            MetaAction::Ok
        }
        "q" | "quit" => MetaAction::Quit,

        // 状态查看
        "s" | "stack" => {
            eprintln!("{}", display::format_stack(&ctx.stack, 20));
            MetaAction::Ok
        }
        "hp" | "heap" => {
            eprintln!("{}", display::format_heap(&ctx.heap, 30));
            MetaAction::Ok
        }
        "i" | "info" => {
            eprintln!(
                "{}",
                display::format_vm_info(
                    0, // REPL doesn't track PC across lines
                    ctx.stack.len(),
                    ctx.heap.len(),
                    1,
                    ctx.functions.len(),
                )
            );
            MetaAction::Ok
        }

        // 状态管理
        "c" | "clear" => {
            ctx.clear();
            eprintln!("Stack and heap cleared.");
            MetaAction::Ok
        }
        "l" | "load" => {
            if arg.is_empty() {
                return MetaAction::Error("Usage: :load <file.sd>".into());
            }
            match load_and_execute_file(ctx, arg) {
                Ok(()) => {
                    eprintln!("File '{}' executed.", arg);
                    MetaAction::Ok
                }
                Err(e) => MetaAction::Error(e),
            }
        }
        "func" | "f" => {
            let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
            if parts.len() < 3 {
                return MetaAction::Error("Usage: :func <n> <body>".into());
            }
            let n: usize = match parts[1].parse() {
                Ok(v) => v,
                Err(_) => return MetaAction::Error("Invalid function name".into()),
            };
            let body_str = parts[2];
            match parser::parse_shorthand(body_str) {
                Ok(body) => {
                    let count = body.len();
                    ctx.define_function(n, body);
                    eprintln!("Function {} defined ({} instructions).", n, count);
                    MetaAction::Ok
                }
                Err(e) => MetaAction::Error(e.to_string()),
            }
        }
        "funcs" => {
            if ctx.functions.is_empty() {
                eprintln!("No functions defined.");
            } else {
                eprintln!("Defined functions:");
                let mut names: Vec<_> = ctx.functions.keys().collect();
                names.sort();
                for n in names {
                    eprintln!("  func {}: {} instructions", n, ctx.functions[n].len());
                }
            }
            MetaAction::Ok
        }
        "raw" => {
            *raw_mode = !*raw_mode;
            eprintln!(
                "Input mode: {}",
                if *raw_mode {
                    "raw .sd syntax"
                } else {
                    "shorthand (auto-detect)"
                }
            );
            MetaAction::Ok
        }
        "history" => {
            if history.is_empty() {
                eprintln!("No history.");
            } else {
                eprintln!("Command history:");
                for (i, h) in history.iter().enumerate() {
                    eprintln!("  {:3}: {}", i + 1, h);
                }
            }
            MetaAction::Ok
        }

        // ── 调试命令（仅 debug 模式）──
        "st" | "step" => {
            if debugger.is_some() {
                eprintln!("Single-step mode enabled.");
                MetaAction::Ok
            } else {
                MetaAction::Error("Debug mode not active. Use stardust -r -d".into())
            }
        }
        "co" | "continue" => {
            if debugger.is_some() {
                eprintln!("Continuing execution.");
                MetaAction::Ok
            } else {
                MetaAction::Error("Debug mode not active.".into())
            }
        }
        "b" | "break" => {
            if debugger.is_some() {
                if arg.parse::<usize>().is_ok() {
                    eprintln!("Use debugger mode with a file for breakpoints: stardust -d file.sd");
                    MetaAction::Ok
                } else {
                    MetaAction::Error("Usage: :b <mark_name>".into())
                }
            } else {
                MetaAction::Error("Debug mode not active.".into())
            }
        }
        "lb" | "breaks" => {
            if debugger.is_some() {
                eprintln!("Breakpoints are managed in file debugging mode.");
                MetaAction::Ok
            } else {
                MetaAction::Error("Debug mode not active.".into())
            }
        }
        "db" => {
            if debugger.is_some() {
                eprintln!("Use debugger mode with a file: stardust -d file.sd");
                MetaAction::Ok
            } else {
                MetaAction::Error("Debug mode not active.".into())
            }
        }
        "list" => {
            if debugger.is_some() {
                eprintln!("Use :list in file debugging mode (stardust -d file.sd)");
                MetaAction::Ok
            } else {
                MetaAction::Error("Debug mode not active.".into())
            }
        }
        "pc" => {
            if debugger.is_some() {
                eprintln!("PC tracking is only available in file debugging mode.");
                MetaAction::Ok
            } else {
                MetaAction::Error("Debug mode not active.".into())
            }
        }
        "bt" => {
            if debugger.is_some() {
                eprintln!("Call stack is tracked in file debugging mode.");
                MetaAction::Ok
            } else {
                MetaAction::Error("Debug mode not active.".into())
            }
        }

        _ => MetaAction::Error(format!(
            "Unknown command ':{}'. Type :help for commands.",
            name
        )),
    }
}

// ── 辅助函数 ────────────────────────────────────────────────

/// 加载并执行 .sd 文件
fn load_and_execute_file(ctx: &mut ReplContext, filename: &str) -> Result<(), String> {
    let source = fs::read_to_string(filename).map_err(|e| format!("read error: {}", e))?;
    let tokens = tokenize(&source).map_err(|e| e.message)?;
    let parsed = parse_program(tokens).map_err(|e| e.message)?;

    // 合并文件中的函数定义
    for (name, body) in &parsed.functions {
        ctx.functions.entry(*name).or_insert_with(|| body.clone());
    }

    // 执行主指令
    executor::execute_repl(ctx, parsed.main_instructions)
}

/// 执行后显示栈状态
fn show_stack_status(ctx: &ReplContext) {
    if ctx.stack.is_empty() {
        return;
    }
    let depth = ctx.stack.len();
    let display_count = 5.min(depth);
    let items: Vec<String> = ctx
        .stack
        .iter()
        .rev()
        .take(display_count)
        .map(|v| v.to_string())
        .collect();
    let mut status = format!("→ [{}]", items.join(", "));
    if depth > display_count {
        status.push_str(&format!(" ... ({} more)", depth - display_count));
    }
    eprintln!("{}", status);
}
