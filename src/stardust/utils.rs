use crate::extension::unwind::{preprocess, simple_preprocess};
use crate::print_error;
use crate::stardust::{ErrorKind, StardustError};
use std::char::ParseCharError;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, process};

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
///
/// # 参数
/// - `input_path`: 输入文本文件的路径
/// - `output_path`: 可选的输出文件路径。若为 `None`，则自动将输入文件的扩展名替换为 `.stardust`
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
