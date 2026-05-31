use esolang_stardust::codegen::{self, CodeGenConfig};
use esolang_stardust::extension::unwind::simple_preprocess;
use esolang_stardust::stardust::lexer::tokenize;
use esolang_stardust::stardust::parser::parse_program;
use esolang_stardust::stardust::utils::{bump_source, compile_file_to_stardust, print_error, print_usage};
use esolang_stardust::stardust::{StardustError, VM};
use std::{env, fs, process};

// TODO:转译为Rust/C代码，实现编译为可执行文件  ← LLVM 编译已实现！
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
        print_cli_usage(&args[0]);
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
    } else if args[1] == "--compile" || args[1] == "-c" {
        cmd_compile(&args);
    } else if args[1] == "--build" || args[1] == "-b" {
        cmd_build(&args);
    } else {
        // 解释执行模式（默认）
        cmd_run(&args);
    }
}

/// 获取源文件的 unwinded 源码（预处理后）
fn read_and_unwind(filename: &str) -> Result<(String, String), StardustError> {
    let source = fs::read_to_string(filename)?;
    let unwind_source = simple_preprocess(&source)?.into_owned();
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

    let (_, unwind_source) = match read_and_unwind(filename) {
        Ok(v) => v,
        Err(e) => {
            print_json_diagnostics(&[&e]);
            process::exit(1);
        }
    };

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

    let (_, unwind_source) = match read_and_unwind(filename) {
        Ok(v) => v,
        Err(e) => {
            println!(r#"{{"tokens":[],"error":"{}"}}"#, e.message);
            process::exit(1);
        }
    };

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

/// --compile: 编译为 LLVM IR 文本文件
fn cmd_compile(args: &[String]) {
    if args.len() < 3 || args.len() > 4 {
        eprintln!("Usage: {} --compile <file.sd> [output.ll]", args[0]);
        process::exit(1);
    }
    let input_file = &args[2];
    let output_file = args.get(3).map(|s| s.as_str());

    let parse_result = parse_input_file(input_file);

    let config = CodeGenConfig::default();
    let ir = codegen::compile_to_ir(&parse_result, &config);

    let out_path = match output_file {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let path = std::path::Path::new(input_file);
            let mut out = path.to_path_buf();
            out.set_extension("ll");
            out
        }
    };

    fs::write(&out_path, &ir).unwrap_or_else(|e| {
        eprintln!("Error writing IR file '{}': {}", out_path.display(), e);
        process::exit(1);
    });

    println!("LLVM IR generated: {}", out_path.display());
    println!("Compile with: clang {} -o program", out_path.display());
}

/// --build: 编译为可执行二进制文件
fn cmd_build(args: &[String]) {
    if args.len() < 3 || args.len() > 4 {
        eprintln!("Usage: {} --build <file.sd> [output]", args[0]);
        process::exit(1);
    }
    let input_file = &args[2];
    let output_file = args.get(3).map(|s| s.as_str());

    // Check toolchain
    let tools = codegen::check_toolchain();
    if !tools.can_compile() {
        eprintln!("Error: LLVM toolchain not found.");
        eprintln!("  llc:  {}", if tools.llc { "✓" } else { "✗ not found" });
        eprintln!("  clang: {}", if tools.clang { "✓" } else { "✗ not found" });
        eprintln!("Install LLVM/clang and try again.");
        process::exit(1);
    }

    let parse_result = parse_input_file(input_file);

    let out_path = match output_file {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let path = std::path::Path::new(input_file);
            let mut out = path.to_path_buf();
            out.set_extension(""); // remove .sd extension
            out
        }
    };

    let config = CodeGenConfig::default();
    match codegen::compile_to_exe(&parse_result, &out_path, &config) {
        Ok(()) => {
            println!("Binary compiled: {}", out_path.display());
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            process::exit(1);
        }
    }
}

/// 完整的解析流水线（预处理 → 词法 → 语法），出错时打印并退出
fn parse_input_file(filename: &str) -> esolang_stardust::stardust::ParseResult {
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
            print_error(&e, &source, filename);
            process::exit(1);
        }
    };

    let tokens = match tokenize(&unwind_source) {
        Ok(toks) => toks,
        Err(e) => {
            print_error(&e, &source, filename);
            process::exit(1);
        }
    };

    match parse_program(tokens) {
        Ok(prog) => prog,
        Err(e) => {
            print_error(&e, &source, filename);
            process::exit(1);
        }
    }
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

fn print_cli_usage(program: &str) {
    eprintln!("Usage:");
    eprintln!("  {} <file.sd>                      Run a Stardust program", program);
    eprintln!("  {} --check <file.sd>             Check syntax, output JSON diagnostics", program);
    eprintln!("  {} --tokens <file.sd>            Output token stream as JSON", program);
    eprintln!("  {} --compile <file.sd> [out.ll]  Compile to LLVM IR", program);
    eprintln!("  {} -c <file.sd>                  Same as --compile", program);
    eprintln!("  {} --build <file.sd> [out]       Compile to binary executable", program);
    eprintln!("  {} -b <file.sd>                  Same as --build", program);
    eprintln!("  {} --stardust <input.txt> [out]  Compile text to Stardust code", program);
    eprintln!("  {} --dump <file.sd> [out]        Analyze and dump pipeline stages", program);
    eprintln!("  {} --help                        Show this help", program);
}
