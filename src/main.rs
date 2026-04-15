pub mod stardust;

use std::{env, fs, process};
use crate::stardust::lexer::tokenize;
use crate::stardust::parser::parse_program;
use crate::stardust::{ErrorKind, StardustError, VM};
use crate::stardust::utils::compile_file_to_stardust;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage(&args[0]);
        process::exit(1);
    }

    // 检查是否为编译模式
    if args[1] == "--stardust" || args[1] == "-s" {
        // 编译模式
        if args.len() < 3 || args.len() > 4 {
            print_usage(&args[0]);
            process::exit(1);
        }
        let input_file = &args[2];
        let output_file = if args.len() == 4 { Some(args[3].as_str()) } else { None };

        if let Err(e) = compile_file_to_stardust(input_file, output_file) {
            eprintln!("Compilation error: {}", e);
            process::exit(1);
        }
        return;
    }

    // help模式
    if args[1] == "--help" {
        print_usage(&args[0]);
        process::exit(1);
        return;
    }

    // 解释执行模式
    if args.len() != 2 {
        print_usage(&args[0]);
        process::exit(1);
    }

    let filename = &args[1];
    if !(filename.ends_with(".stardust") || filename.ends_with(".sd")) {
        eprintln!("Error: File must have .stardust or .sd extension");
        process::exit(1);
    }

    let source = match fs::read_to_string(filename) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filename, e);
            process::exit(1);
        }
    };

    let tokens = match tokenize(&source) {
        Ok(toks) => toks,
        Err(e) => {
            print_error(&e, &source, filename);
            process::exit(1);
        }
    };

    let parsed = match parse_program(tokens) {
        Ok(prog) => prog,
        Err(e) => {
            print_error(&e, &source, filename);
            process::exit(1);
        }
    };

    let mut vm = VM::new(parsed);
    if let Err(e) = vm.run() {
        print_error(&e, &source, filename);
        process::exit(1);
    }
}

fn print_usage(program: &str) {
    eprintln!("Usage:");
    eprintln!("  {} <file.stardust|file.sd>           Run a Stardust program", program);
    eprintln!("  {} --stardust <input.txt> [output]    Compile text file to Stardust code", program);
}

fn print_error(error: &StardustError, source: &str, filename: &str) {
    eprintln!("Error: {}", error.message);
    if let Some(span) = &error.span {
        eprintln!("  --> {}:{}:{}", filename, span.line, span.column);
        // 打印源代码行
        if let Some(line) = source.lines().nth(span.line - 1) {
            eprintln!("   |");
            eprintln!("{:3} | {}", span.line, line);
            eprintln!("   | {}{}", " ".repeat(span.column - 1), "^");
        }
    }
    if let ErrorKind::IoError { reason } = &error.kind {
        eprintln!("  I/O details: {}", reason);
    }
}