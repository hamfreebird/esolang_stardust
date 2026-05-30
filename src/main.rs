pub mod extension;
pub mod ide_py;
pub mod stardust;

use crate::extension::unwind::simple_preprocess;
use crate::stardust::lexer::tokenize;
use crate::stardust::parser::parse_program;
use crate::stardust::utils::{bump_source, compile_file_to_stardust, print_error, print_usage};
use crate::stardust::VM;
use std::{env, fs, process};

// TODO:转译为Rust/C代码，实现编译为可执行文件
// TODO:分离IDE部分的代码，暂定使用py实现
// TODO:增加调试处理，单点执行

fn main() {
    let args: Vec<String> = env::args().collect();
    stardust(args);

    println!(
        "You now open an Stardust IDE where you can write code and run it.\n\
    The code written in the IDE will be performed at this terminal window.\n\
    If you want to run the code separately, use the command line to run the\n\
    file that contains the stardust code directly\n\
    Usage: stardust <file.stardust|file.sd>\n\
    When you use the Stardust IDE, a good way to determine the correct \n\
    syntax of the code is to see if the code is highlight, \n\
    the highlight of the IDE is based on the interpreter's tokenize.\n"
    );
}

fn stardust(args: Vec<String>) {
    if args.len() < 2 {
        return;
    }

    if args[1] == "--stardust" || args[1] == "-s" {
        // 字符转换模式
        if args.len() < 3 || args.len() > 4 {
            print_usage(&args[0]);
            process::exit(1);
        }
        let input_file = &args[2];
        let output_file = if args.len() == 4 {
            Some(args[3].as_str())
        } else {
            None
        };

        if let Err(e) = compile_file_to_stardust(input_file, output_file) {
            eprintln!("Transform error: {}", e);
            process::exit(1);
        }
    } else if args[1] == "--help" {
        print_usage(&args[0]);
        process::exit(0);
    } else if args[1] == "--dump" {
        // 分析输出功能
        if args.len() < 3 || args.len() > 4 {
            print_usage(&args[0]);
            process::exit(1);
        }
        let input_file = &args[2];
        let output_file = if args.len() == 4 {
            Some(args[3].as_str())
        } else {
            None
        };

        if !(input_file.ends_with(".stardust") || input_file.ends_with(".sd")) {
            eprintln!("Error: File must have .stardust or .sd extension");
            process::exit(1);
        }

        if let Err(e) = bump_source(input_file, output_file) {
            eprintln!("Create error: {}", e);
            process::exit(1);
        }
    }

    // 解释执行模式
    if args.len() != 2 {
        print_usage(&args[0]);
        process::exit(0);
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

    let unwind_source = match simple_preprocess(&source) {
        Ok(uw) => uw,
        Err(e) => {
            print_error(&e, &source, "");
            return;
        }
    };

    let tokens = match tokenize(&unwind_source) {
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

    process::exit(0);
}
