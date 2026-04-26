use crate::extension::unwind::{preprocess, simple_preprocess};
use crate::stardust::{ErrorKind, Instruction, StageResult, StardustError};
use std::char::ParseCharError;
use std::path::{Path, PathBuf};
use std::{fs, process};
use std::io::Write;
use crate::stardust::lexer::tokenize;
use crate::stardust::parser::parse_program;

impl From<ParseCharError> for StardustError {
    fn from(err: ParseCharError) -> Self {
        StardustError {
            kind: ErrorKind::ParseChar,            // 字符解析错误
            span: None,
            message: err.to_string(),
        }
    }
}

impl From<std::io::Error> for StardustError {
    fn from(err: std::io::Error) -> Self {
        StardustError {
            kind: ErrorKind::StdIoError,
            span: None,
            message: err.to_string(),
        }
    }
}

// 用于包装错误并传递
impl StardustError {
    fn char_error(err: StardustError, input_path: &str) -> Self {
        StardustError {
            kind: ErrorKind::ParseChar,
            span: None,
            message: format!("{} in {}", err, input_path),
        }
    }
}

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

fn natural_source_code(
    content: String,
    input_path: &str
) -> Result<String, StardustError> {
    let mut chars_vec: Vec<String> = content.chars()
        .map(|c| c.to_string())
        .collect();
    let copy_chars_vec = chars_vec.clone();

    let mut in_anno: bool = false;
    for (index, char) in chars_vec.iter_mut().enumerate() {
        if in_anno == true {
            continue
        };
        let _char = char.chars().next().unwrap();
        if _char.is_whitespace() | is_symbol(_char) {
            if _char == '\n'{
                in_anno = false;
            }
            continue
        } else if is_anno(_char) && is_anno(copy_chars_vec[index].parse()?) {
            in_anno = true;
        } else if _char.is_ascii() {
            let _char = char.clone();
            let sd_char = match preprocess(_char.as_str()) {
                Ok(uw) => uw,
                Err(e) => {
                    return Err(StardustError::char_error(e, input_path));
                }
            };
            char.replace_range(0..1, sd_char.as_ref());
        } else {
            char.replace_range(0..1, " ");
        }
    }

    Ok(chars_vec.concat())
}

fn is_symbol(ch: char) -> bool {
    matches!(ch, '+' | '*' | '`' | '\'' | ':' | ';' | '.' | ','
    | '-' | '=' | '<' | '>' | '&' | '~' | '#')
}

fn is_anno(ch: char) -> bool {
    matches!(ch, '/')
}

/// 便捷函数：自动生成输出文件名（扩展名改为 `.stardust`）
pub fn compile_file_auto(input_path: &str) -> Result<(), StardustError> {
    compile_file_to_stardust(input_path, None)
}

pub fn bump_source(
     input_file: &String,
     output_file: Option<&str>
) -> Result<(), StardustError> {

    let bump_film = format_results(&*bump_run_source(input_file));

    // 确定输出文件路径
    let out_path = match output_file{
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
    let mut results = vec![
        StageResult::None,
        StageResult::None,
        StageResult::None,
        StageResult::None,
    ];

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

    let unwind_source = match simple_preprocess(&source) {
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

/// 格式化单条指令
fn format_instruction(instr: &Instruction) -> String {
    match instr {
        Instruction::Push(n) => format!("Push({})", n),
        Instruction::Dup => "Dup".to_string(),
        Instruction::Swap => "Swap".to_string(),
        Instruction::Rotate => "Rotate".to_string(),
        Instruction::Pop => "Pop".to_string(),
        Instruction::Add => "Add".to_string(),
        Instruction::Sub => "Sub".to_string(),
        Instruction::Mul => "Mul".to_string(),
        Instruction::Div => "Div".to_string(),
        Instruction::Mod => "Mod".to_string(),
        Instruction::Reverse => "Reverse".to_string(),
        Instruction::NumOut => "NumOut".to_string(),
        Instruction::NumIn => "NumIn".to_string(),
        Instruction::CharOut => "CharOut".to_string(),
        Instruction::CharIn => "CharIn".to_string(),
        Instruction::Mark { name } => format!("Mark({})", name),
        Instruction::Jump { name } => format!("Jump({})", name),
        Instruction::Call { name, argc } => format!("Call(name={}, argc={})", name, argc),
        Instruction::UnconditionalJump { name } => format!("UncondJump({})", name),
    }
}

/// 主格式化函数
pub fn format_results(results: &[StageResult]) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    let stage_names = ["1. Source", "2. Unwind Source", "3. Tokens", "4. Parsed"];

    for (i, stage_result) in results.iter().enumerate() {
        let stage_title = if i < stage_names.len() {
            stage_names[i]
        } else {
            "Extra Stage"
        };
        writeln!(output, "=== {} ===", stage_title).unwrap();

        match stage_result {
            StageResult::Source(src) => {
                writeln!(output, "Status: Success").unwrap();
                writeln!(output, "Length: {} bytes", src.len()).unwrap();
                writeln!(output, "Content:").unwrap();
                for line in src.lines() {
                    if line.len() > 100 {
                        let formatted = format_long_line(line);
                        for sub_line in formatted.lines() {
                            writeln!(output, "{}", sub_line).unwrap();
                        }
                    } else {
                        writeln!(output, "{}", line).unwrap();
                    }
                }
            }
            StageResult::UnwindSource(uw_src) => {
                writeln!(output, "Status: Success").unwrap();
                writeln!(output, "Length: {} bytes", uw_src.len()).unwrap();
                writeln!(output, "Content:").unwrap();
                for line in uw_src.lines() {
                    if line.len() > 100 {
                        let formatted = format_long_line(line);
                        for sub_line in formatted.lines() {
                            writeln!(output, "{}", sub_line).unwrap();
                        }
                    } else {
                        writeln!(output, "{}", line).unwrap();
                    }
                }
            }
            StageResult::Tokens(tokens) => {
                writeln!(output, "Status: Success").unwrap();
                writeln!(output, "Total tokens: {}", tokens.len()).unwrap();
                if tokens.is_empty() {
                    writeln!(output, "No tokens.").unwrap();
                } else {
                    for (idx, tok) in tokens.iter().enumerate() {
                        writeln!(
                            output,
                            "[{:3}] line {:3}:{:3} | spaces={} | {:?}",
                            idx, tok.line, tok.column, tok.spaces, tok.token_type
                        )
                            .unwrap();
                    }
                }
            }
            StageResult::Parsed(parsed) => {
                writeln!(output, "Status: Success").unwrap();
                writeln!(output, "Main instructions: {}", parsed.main_instructions.len()).unwrap();
                writeln!(output, "Main marks: {}", parsed.main_marks.len()).unwrap();
                writeln!(output, "Functions defined: {}", parsed.functions.len()).unwrap();

                if !parsed.main_instructions.is_empty() {
                    writeln!(output, "Main instructions:").unwrap();
                    for (idx, instr) in parsed.main_instructions.iter().enumerate() {
                        writeln!(output, "  {:3}: {}", idx, format_instruction(instr)).unwrap();
                    }
                } else {
                    writeln!(output, "Main instructions: (none)").unwrap();
                }

                if !parsed.main_marks.is_empty() {
                    writeln!(output, "Main marks:").unwrap();
                    for (mark_name, &instr_idx) in parsed.main_marks.iter() {
                        writeln!(output, "  mark {} -> instruction index {}", mark_name, instr_idx).unwrap();
                    }
                } else {
                    writeln!(output, "Main marks: (none)").unwrap();
                }

                if !parsed.functions.is_empty() {
                    writeln!(output, "Functions:").unwrap();
                    for (func_name, body) in parsed.functions.iter() {
                        writeln!(output, "  function {} ({} instructions):", func_name, body.len()).unwrap();
                        for (idx, instr) in body.iter().enumerate() {
                            writeln!(output, "    {:3}: {}", idx, format_instruction(instr)).unwrap();
                        }
                    }
                } else {
                    writeln!(output, "Functions: (none)").unwrap();
                }
            }
            StageResult::Error(err_msg) => {
                writeln!(output, "Status: ERROR").unwrap();
                writeln!(output, "Message: {}", err_msg).unwrap();
            }
            StageResult::None => {
                writeln!(output, "Status: Not executed (previous stage failed)").unwrap();
            }
        }
        writeln!(output).unwrap(); // 阶段间空行
    }

    output
}

pub fn print_usage(program: &str) {
    eprintln!("Usage:");
    eprintln!("  {} <file.stardust|file.sd>           Run a Stardust program", program);
    eprintln!("  {} --stardust <input.txt> [output]   Compile text file to Stardust code", program);
    eprintln!("  {} --dump <input.stardust> [output]  Analyze a Stardust program and output analysis results", program);
    eprintln!("  {}                                   Open Stardust IDE", program);
}

pub fn print_error(error: &StardustError, source: &str, filename: &str) {
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
