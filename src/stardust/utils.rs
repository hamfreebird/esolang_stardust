use crate::extension::unwind::simple_preprocess;
use crate::stardust::lexer::tokenize;
use crate::stardust::parser::parse_program;
use crate::stardust::{ErrorKind, StageResult, StardustError};
use std::char::ParseCharError;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, process};

impl From<ParseCharError> for StardustError {
    fn from(err: ParseCharError) -> Self {
        StardustError {
            kind: ErrorKind::ParseChar, // 字符解析错误
            span: None,
            message: err.to_string(),
        }
    }
}

impl From<std::io::Error> for StardustError {
    fn from(err: std::io::Error) -> Self {
        StardustError { kind: ErrorKind::StdIoError, span: None, message: err.to_string() }
    }
}

// 用于包装错误并传递
/// 根据字符串生成 Stardust 打印源码
pub fn generate_print_string(s: &str) -> String {
    let mut code = String::new();
    for ch in s.chars() {
        let ascii = ch as usize;
        let spaces = ascii + 5;
        code.push_str(&" ".repeat(spaces));
        code.push('+');
        code.push(','); // char_out
    }
    code
}

/// 读取文本文件并生成 Stardust 源码文件
pub fn compile_file_to_stardust(
    input_path: &str,
    output_path: Option<&str>,
) -> Result<(), StardustError> {
    // 读取输入文件内容
    let content = fs::read_to_string(input_path)?;
    let copy_content = content.clone();

    let stardust_code = match natural_source_code(content, input_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Warn transform code '{}': {}", input_path, e);
            eprintln!("Simple ASCII codes will be used for replacement");
            // 如果转换失败，复用unwind中的简单转换器
            let unwind_source = match simple_preprocess(&copy_content) {
                Ok(uw) => uw,
                Err(e) => {
                    print_error(&e, &copy_content, input_path);
                    process::exit(1);
                }
            };
            unwind_source.parse().unwrap()
        }
    };

    // 确定输出文件路径
    let out_path = match output_path {
        Some(p) => PathBuf::from(p),
        None => {
            let path = Path::new(input_path);
            let mut out = path.to_path_buf();
            out.set_extension("stardust");
            out
        }
    };

    // 将源码写入输出文件
    let mut file = fs::File::create(&out_path)?;
    file.write_all(stardust_code.as_bytes())?;

    println!("Stardust code generated successfully: {}", out_path.display());
    Ok(())
}

/// 将任意文本转换为「打印该文本」的 Stardust 源码。
///
/// 除换行等空白外，每个可见字符都被替换为 `Push(code) CharOut` 序列。
/// 特别注意：Stardust 的 15 个符号字符（`+ * ` ' : ; . , - = < > & ~ #`）
/// 在此也必须被转换——它们在这里是要**输出的文本**，而不是指令。
/// 旧实现把这些符号原样保留，结果是它们被当作指令执行
/// （例如文本里的 `,` 变成一次真实的 CharOut，随后引发栈下溢）。
fn natural_source_code(content: String, _input_path: &str) -> Result<String, StardustError> {
    let mut out = String::with_capacity(content.len() * 8);

    for ch in content.chars() {
        // 换行保留为换行（源码结构），其余可打印 ASCII（含空格）
        // 一律转成 Push + CharOut，这样空格和标点都能原样输出。
        if ch == '\n' {
            out.push('\n');
            continue;
        }
        if ch == ' ' || ch == '\t' {
            out.push_str(&generate_print_string(" "));
            continue;
        }
        // 可见 ASCII（字母、数字、标点、Stardust 符号）→ Push + CharOut
        if ch.is_ascii_graphic() {
            out.push_str(&generate_print_string(&ch.to_string()));
        } else {
            // 非 ASCII 字符没有可打印表示，替换为空格占位
            out.push(' ');
        }
    }

    Ok(out)
}

/// 便捷函数：自动生成输出文件名（扩展名改为 `.stardust`）
pub fn compile_file_auto(input_path: &str) -> Result<(), StardustError> {
    compile_file_to_stardust(input_path, None)
}

pub fn bump_source(input_file: &String, output_file: Option<&str>) -> Result<(), StardustError> {
    let bump_film = format_results(&bump_run_source(input_file));

    // 确定输出文件路径
    let out_path = match output_file {
        Some(p) => PathBuf::from(p),
        None => {
            let path = Path::new(input_file);
            let mut out = path.to_path_buf();
            out.set_extension("stardust_dump");
            out
        }
    };

    // 写入输出文件
    let mut file = fs::File::create(&out_path)?;
    file.write_all(bump_film.as_bytes())?;

    println!("Stardust bump film successfully create: {}", out_path.display());
    Ok(())
}

pub fn bump_run_source(filename: &String) -> Vec<StageResult> {
    let mut results =
        vec![StageResult::None, StageResult::None, StageResult::None, StageResult::None];

    let source = match fs::read_to_string(filename) {
        Ok(content) => {
            results[0] = StageResult::Source(content.clone());
            content
        }
        Err(e) => {
            let err_msg = format!("Error reading file '{}': {}", filename, e);
            eprintln!("{}", err_msg);
            results[0] = StageResult::Error(err_msg);
            return results;
        }
    };

    // 与 cmd_run 流水线一致：先剥离 #[key=value] 注解再预处理
    let clean_source = match crate::stardust::config::parse_config(&source) {
        Ok((_cfg, clean)) => clean,
        Err(e) => {
            let msg = format!("Config error: {}", e);
            eprintln!("{}", msg);
            results[1] = StageResult::Error(msg);
            return results;
        }
    };

    let unwind_source = match simple_preprocess(&clean_source) {
        Ok(uw) => {
            let owned = uw.into_owned();
            results[1] = StageResult::UnwindSource(owned.clone());
            owned
        }
        Err(e) => {
            print_error(&e, &source, "");
            results[1] = StageResult::Error(format!("{:?}", e));
            return results;
        }
    };

    let tokens = match tokenize(&unwind_source) {
        Ok(toks) => {
            results[2] = StageResult::Tokens(toks.clone());
            toks
        }
        Err(e) => {
            print_error(&e, &source, filename);
            results[2] = StageResult::Error(format!("{:?}", e));
            return results;
        }
    };

    match parse_program(tokens) {
        Ok(prog) => {
            results[3] = StageResult::Parsed(prog);
        }
        Err(e) => {
            print_error(&e, &source, filename);
            results[3] = StageResult::Error(format!("{:?}", e));
        }
    };

    results
}

/// 将一行长文本按规则分割
fn format_long_line(line: &str) -> String {
    let mut result = String::new();
    let mut remaining = line;
    let mut first_chunk = true;

    while !remaining.is_empty() {
        if !first_chunk {
            result.push_str(" -\n");
        }
        first_chunk = false;

        if remaining.len() > 100 {
            let (chunk, rest) = remaining.split_at(100);
            result.push_str(chunk);
            remaining = rest;
        } else {
            result.push_str(remaining);
            break;
        }
    }
    result
}

/// 主格式化函数
/// 主格式化函数
pub fn format_results(results: &[StageResult]) -> String {
    // 统一用 `push_str(&format!(..))` 而非 `writeln!(..).unwrap()`：
    // 对 `String` 的写入永远不会失败，不需要处理 Result。
    let mut output = String::new();
    let stage_names = ["1. Source", "2. Unwind Source", "3. Tokens", "4. Parsed"];

    for (i, stage_result) in results.iter().enumerate() {
        let stage_title = if i < stage_names.len() { stage_names[i] } else { "Extra Stage" };
        output.push_str(&format!("=== {} ===\n", stage_title));

        match stage_result {
            StageResult::Source(src) => {
                output.push_str(&render_text_stage(src));
            }
            StageResult::UnwindSource(uw_src) => {
                output.push_str(&render_text_stage(uw_src));
            }
            StageResult::Tokens(tokens) => {
                output.push_str("Status: Success\n");
                output.push_str(&format!("Total tokens: {}\n", tokens.len()));
                if tokens.is_empty() {
                    output.push_str("No tokens.\n");
                } else {
                    for (idx, tok) in tokens.iter().enumerate() {
                        output.push_str(&format!(
                            "[{:3}] line {:3}:{:3} | spaces={} | {:?}\n",
                            idx, tok.line, tok.column, tok.spaces, tok.token_type
                        ));
                    }
                }
            }
            StageResult::Parsed(parsed) => {
                output.push_str("Status: Success\n");
                output.push_str(&format!(
                    "Main instructions: {}\nMain marks: {}\nFunctions defined: {}\n",
                    parsed.main_instructions.len(),
                    parsed.main_marks.len(),
                    parsed.functions.len()
                ));

                if parsed.main_instructions.is_empty() {
                    output.push_str("Main instructions: (none)\n");
                } else {
                    output.push_str("Main instructions:\n");
                    for (idx, instr) in parsed.main_instructions.iter().enumerate() {
                        output.push_str(&format!("  {:3}: {}\n", idx, instr.format()));
                    }
                }

                if parsed.main_marks.is_empty() {
                    output.push_str("Main marks: (none)\n");
                } else {
                    output.push_str("Main marks:\n");
                    let mut marks: Vec<_> = parsed.main_marks.iter().collect();
                    marks.sort_by_key(|(name, _)| **name);
                    for (mark_name, instr_idx) in marks {
                        output.push_str(&format!(
                            "  mark {} -> instruction index {}\n",
                            mark_name, instr_idx
                        ));
                    }
                }

                if parsed.functions.is_empty() {
                    output.push_str("Functions: (none)\n");
                } else {
                    output.push_str("Functions:\n");
                    let mut funcs: Vec<_> = parsed.functions.iter().collect();
                    funcs.sort_by_key(|(name, _)| **name);
                    for (func_name, func_def) in funcs {
                        output.push_str(&format!(
                            "  function {} ({} instructions):\n",
                            func_name,
                            func_def.body.len()
                        ));
                        for (idx, instr) in func_def.body.iter().enumerate() {
                            output.push_str(&format!("    {:3}: {}\n", idx, instr.format()));
                        }
                    }
                }
            }
            StageResult::Error(err_msg) => {
                output.push_str(&format!("Status: ERROR\nMessage: {}\n", err_msg));
            }
            StageResult::None => {
                output.push_str("Status: Not executed (previous stage failed)\n");
            }
        }
        output.push('\n'); // 阶段间空行
    }

    output
}

/// 渲染一个「原始文本 / 预处理后文本」阶段的内容。
fn render_text_stage(src: &str) -> String {
    let mut out = String::new();
    out.push_str("Status: Success\n");
    out.push_str(&format!("Length: {} bytes\n", src.len()));
    out.push_str("Content:\n");
    for line in src.lines() {
        if line.len() > 100 {
            out.push_str(&format_long_line(line));
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

pub fn print_usage(program: &str) {
    eprintln!("Usage:");
    eprintln!("  {} <file.stardust|file.sd>           Run a Stardust program", program);
    eprintln!(
        "  {} --check <file.sd>                 Check syntax, output JSON diagnostics",
        program
    );
    eprintln!("  {} --tokens <file.sd>                Output token stream as JSON", program);
    eprintln!("  {} --stardust <input.txt> [output]   Compile text file to Stardust code", program);
    eprintln!(
        "  {} --dump <file.sd> [output]         Analyze and dump all pipeline stages",
        program
    );
    eprintln!("  {} --help                            Show this help", program);
}

pub fn print_error(error: &StardustError, source: &str, filename: &str) {
    eprintln!("Error: {}", error.message);
    if let Some(span) = &error.span {
        eprintln!("  --> {}:{}:{}", filename, span.line, span.column);
        // 打印源代码行（防御性：line/column 为 0 时不尝试打印）
        if span.line > 0
            && let Some(line) = source.lines().nth(span.line.saturating_sub(1))
        {
            eprintln!("   |");
            eprintln!("{:3} | {}", span.line, line);
            let col = span.column.saturating_sub(1);
            eprintln!("   | {}^", " ".repeat(col));
        }
    }
    if let ErrorKind::IoError { reason } = &error.kind {
        eprintln!("  I/O details: {}", reason);
    }
}
