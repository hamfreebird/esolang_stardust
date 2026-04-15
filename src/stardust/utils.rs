use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
///
/// # 错误
/// 若文件读取或写入失败，返回 `Box<dyn std::error::Error>`
pub fn compile_file_to_stardust(
    input_path: &str,
    output_path: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 读取输入文件内容
    let content = fs::read_to_string(input_path)?;

    // 生成 Stardust 源码
    let stardust_code = generate_print_string(&content);

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

/// 便捷函数：自动生成输出文件名（扩展名改为 `.stardust`）
pub fn compile_file_auto(input_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    compile_file_to_stardust(input_path, None)
}
