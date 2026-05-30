use esolang_stardust::extension::unwind::simple_preprocess;
use esolang_stardust::stardust::lexer::tokenize;
use esolang_stardust::stardust::parser::parse_program;
use esolang_stardust::stardust::utils::{bump_source, compile_file_to_stardust, print_error, print_usage};
use esolang_stardust::stardust::VM;
use std::{env, fs, process};

// TODO:转译为Rust/C代码，实现编译为可执行文件
// TODO:增加调试处理，单点执行

fn main() {
    let args: Vec<String> = env::args().collect();
    stardust(args);
}

fn stardust(args: Vec<String>) {
    if args.len() < 2 {
        print_usage(&args[0]);
        process::exit(0);
    }

    if args[1] == "--help" || args[1] == "-h" {
        print_usage(&args[0]);
        process::exit(0);
    }

    if args[1] == "--stardust" || args[1] == "-s" {
        cmd_stardust(&args);
    } else if args[1] == "--dump" {
        cmd_dump(&args);
    } else if args[1] == "--check" {
        cmd_check(&args);
    } else if args[1] == "--tokens" {
        cmd_tokens(&args);
    } else {
        // 解释执行模式（默认）
        cmd_run(&args);
    }
}

/// 获取源文件的 unwinded 源码（预处理后）
fn read_and_unwind(filename: &str) -> Result<(String, String), String> {
    let source = fs::read_to_string(filename)
        .map_err(|e| format!("Error reading file '{}': {}", filename, e))?;

    let unwind_source = simple_preprocess(&source)
        .map(|uw| uw.into_owned())
        .map_err(|e| format!("Preprocess error: {}", e))?;

    Ok((source, unwind_source))
}

/// --stardust: 字符转换模式
fn cmd_stardust(args: &[String]) {
    if args.len() < 3 || args.len() > 4 {
        print_usage(&args[0]);
        process::exit(1);
    }
    let input_file = &args[2];
    let output_file = args.get(3).map(|s| s.as_str());

    if let Err(e) = compile_file_to_stardust(input_file, output_file) {
        eprintln!("Transform error: {}", e);
        process::exit(1);
    }
}

/// --dump: 分析输出功能
fn cmd_dump(args: &[String]) {
    if args.len() < 3 || args.len() > 4 {
        print_usage(&args[0]);
        process::exit(1);
    }
    let input_file = &args[2];
    let output_file = args.get(3).map(|s| s.as_str());

    if !(input_file.ends_with(".stardust") || input_file.ends_with(".sd")) {
        eprintln!("Error: File must have .stardust or .sd extension");
        process::exit(1);
    }

    if let Err(e) = bump_source(input_file, output_file) {
        eprintln!("Create error: {}", e);
        process::exit(1);
    }
}

/// --check: 语法检查模式（JSON 诊断输出）
fn cmd_check(args: &[String]) {
    if args.len() != 3 {
        print_usage(&args[0]);
        process::exit(1);
    }
    let filename = &args[2];

    let (_, unwind_source) = read_and_unwind(filename).unwrap_or_else(|e| {
        // IO 错误输出为 JSON
        println!(r#"{{"status":"error","diagnostics":[{{"severity":"error","line":1,"column":1,"message":"{}","code":"IOError"}}]}}"#, e);
        process::exit(1);
    });

    let tokens = match tokenize(&unwind_source) {
        Ok(toks) => toks,
        Err(e) => {
            print_json_diagnostics(&[&e]);
            process::exit(0);
        }
    };

    match parse_program(tokens) {
        Ok(_) => {
            println!(r#"{{"status":"ok","diagnostics":[]}}"#);
        }
        Err(e) => {
            print_json_diagnostics(&[&e]);
        }
    }
    process::exit(0);
}

/// --tokens: Token 流输出模式（JSON）
fn cmd_tokens(args: &[String]) {
    if args.len() != 3 {
        print_usage(&args[0]);
        process::exit(1);
    }
    let filename = &args[2];

    let (_, unwind_source) = read_and_unwind(filename).unwrap_or_else(|e| {
        println!(r#"{{"tokens":[],"error":"{}"}}"#, e);
        process::exit(1);
    });

    let tokens = match tokenize(&unwind_source) {
        Ok(toks) => toks,
        Err(e) => {
            println!(r#"{{"tokens":[],"error":"{}"}}"#, e.message);
            process::exit(0);
        }
    };

    let json = serde_json::to_string(&serde_json::json!({
        "tokens": tokens,
    })).unwrap();

    println!("{}", json);
    process::exit(0);
}

/// 默认运行模式：完整流水线执行
fn cmd_run(args: &[String]) {
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

/// 将错误列表以 JSON 诊断格式输出
fn print_json_diagnostics(errors: &[&esolang_stardust::stardust::StardustError]) {
    let diagnostics: Vec<serde_json::Value> = errors
        .iter()
        .map(|e| {
            serde_json::json!({
                "severity": "error",
                "line": e.span.as_ref().map(|s| s.line).unwrap_or(1),
                "column": e.span.as_ref().map(|s| s.column).unwrap_or(1),
                "message": e.message,
                "code": format!("{:?}", e.kind),
            })
        })
        .collect();

    let output = serde_json::json!({
        "status": "error",
        "diagnostics": diagnostics,
    });

    println!("{}", serde_json::to_string(&output).unwrap());
}
