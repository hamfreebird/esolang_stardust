pub mod stardust;
pub mod extension;
pub mod ide_py;

use crate::stardust::lexer::tokenize;
use crate::stardust::parser::parse_program;
use crate::stardust::utils::compile_file_to_stardust;
use crate::stardust::{ErrorKind, StardustError, Token, TokenType, VM};
use std::{env, fs, process};
use std::collections::BTreeSet;
use eframe::egui;
use egui::text::LayoutJob;
use egui::Color32;
use egui::TextFormat;
use egui::panel::Panel;
use egui_code_editor::Syntax;
use crate::extension::unwind::{preprocess, simple_preprocess};
// TODO:转译为Rust/C代码，实现编译为可执行文件

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = env::args().collect();
    stardust(args);

    println!("You now open an Stardust IDE where you can write code and run it.\n\
    The code written in the IDE will be performed at this terminal window.\n\
    If you want to run the code separately, use the command line to run the\n\
    file that contains the stardust code directly\n\
    Usage:\n\
    |   stardust <file.stardust|file.sd>           Run a Stardust program\n\
    |   stardust --stardust <input.txt> [output]   Compile text file to Stardust code\n\
    When you use the Stardust IDE, a good way to determine the correct \n\
    syntax of the code is to see if the code is highlight, \n\
    the highlight of the IDE is based on the interpreter's tokenize.\n");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Stardust IDE",
        options,
        Box::new(|cc| {
            Ok(Box::<MyApp>::default())
        }),
    )
}

struct MyApp {
    code: String,
    tokens: Vec<Token>,
    current_file: Option<std::path::PathBuf>,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            code: String::new(),
            tokens: Vec::new(),
            current_file: None,
        }
    }
}

impl MyApp {
    fn new() -> Self {
        Self {
            code: String::new(),
            tokens: Vec::new(),
            current_file: None,
        }
    }

    fn update_tokens(&mut self) {
        match tokenize(&self.code) {
            Ok(tokens) => self.tokens = tokens,
            Err(_) => self.tokens.clear(), // 词法错误时不显示高亮
        }
    }

    fn new_file(&mut self) {
        self.code.clear();
        self.current_file = None;
    }

    fn open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                self.code = content;
                self.current_file = Some(path);
            }
        }
    }

    fn save_file(&mut self) {
        if let Some(path) = &self.current_file {
            // 已有路径，直接保存
            let _ = std::fs::write(path, &self.code);
        } else {
            // 另存为
            if let Some(path) = rfd::FileDialog::new().save_file() {
                let _ = std::fs::write(&path, &self.code);
                self.current_file = Some(path);
            }
        }
    }

    fn run_code(&mut self) {
        let unwind_source = match simple_preprocess(&self.code) {
            Ok(uw) => uw,
            Err(e) => {
                print_error(&e, &self.code, "");
                return;
            }
        };
        println!("{}", unwind_source);

        let tokens = match tokenize(&unwind_source) {
            Ok(toks) => toks,
            Err(e) => {
                print_error(&e, &self.code, "");
                return;
            }
        };
        println!("{:?}", tokens);

        let parsed = match parse_program(tokens) {
            Ok(prog) => prog,
            Err(e) => {
                print_error(&e, &self.code, "");
                return;
            }
        };
        println!("{:?}", parsed);

        let mut vm = VM::new(parsed);
        if let Err(e) = vm.run() {
            print_error(&e, &self.code, "");
            return;
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ctx: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 顶部工具栏
        Panel::top("top_panel").show_inside(ctx, |ui| {
            ui.horizontal(|ui| {
                // 左侧三个文件操作按钮
                if ui.button("New").clicked() {
                    self.new_file();
                }
                if ui.button("Open").clicked() {
                    self.open_file();
                }
                if ui.button("Save").clicked() {
                    self.save_file();
                }
                // 右侧运行按钮（用空白空间把按钮推到右边）
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Run").clicked() {
                        self.run_code();
                    }
                });
            });
            ui.add_space(1.0);
        });

        // 底部状态栏
        Panel::bottom("bottom_panel").show_inside(ctx, |ui| {
            let byte_count = self.code.len();
            let char_count = self.code.chars().count();
            let line_count = self.code.lines().count();

            ui.horizontal(|ui| {
                // 如果有当前文件路径，显示文件名
                if let Some(path) = &self.current_file {
                    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                        ui.label(format!("Film: {}", file_name));
                    }
                } else {
                    ui.label("Unnamed");
                }
                ui.label(format!("Size: {} byte", byte_count));
                ui.label(format!("Chars: {}", char_count));
                ui.label(format!("Lines: {}", line_count));
            });
        });

        // --- 中央编辑区 ---
        egui::CentralPanel::default().show_inside(ctx, |ui| {
            self.update_tokens(); // 更新语法高亮
            // 创建用于高亮的 layouter
            let mut layouter = |ui: &egui::Ui, text_buffer: &dyn egui::TextBuffer, _wrap_width: f32| {
                // 从 TextBuffer 中获取 &str
                let string = text_buffer.as_str();
                let job = highlight(string, &self.tokens);
                ui.fonts_mut(|fonts| fonts.layout_job(job))
            };
            egui::TextEdit::multiline(&mut self.code)
                .font(egui::TextStyle::Monospace)
                .layouter(&mut layouter)
                .desired_width(ui.available_width())   // 宽度填满
                .desired_rows(10000)         // 高度尽可能大，但实际被面板限制
                .show(ui);
        });
    }
}

pub fn token_format(token: &Token) -> TextFormat {
    let color = match token.token_type {
        TokenType::Plus => Color32::from_rgb(100, 200, 255),   // 浅蓝
        TokenType::Star => Color32::from_rgb(255, 160, 100),   // 橙色
        TokenType::Backtick => Color32::from_rgb(200, 150, 255), // 淡紫
        TokenType::Quote => Color32::from_rgb(255, 100, 100),   // 红色
        TokenType::Colon => Color32::from_rgb(150, 255, 150),   // 浅绿
        TokenType::Semicolon => Color32::GRAY,
        TokenType::Dot => Color32::from_rgb(255, 255, 100),     // 黄色
        TokenType::Comma => Color32::LIGHT_GRAY,
        _ => {Color32::WHITE}
    };

    TextFormat {
        color,
        ..Default::default()
    }
}

fn stardust_syntax() -> Syntax {
    Syntax {
        language: "stardust",
        case_sensitive: true,
        comment: ";",  // Stardust 中以分号开头的注释
        comment_multiline: ["", ""],
        quotes: Default::default(),
        hyperlinks: BTreeSet::new(),
        keywords: BTreeSet::from_iter([
            "+", "*", "`", "'", ":", ";", ".", ",",
        ]),
        types: BTreeSet::new(),
        special: BTreeSet::new(),
    }
}

fn highlight(code: &str, tokens: &[Token]) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut last_byte = 0;

    for token in tokens {
        // 符号的起始字节必须有效
        if token.byte_pos > code.len() {
            // 无效 token，跳过
            continue;
        }

        // 处理前导空格（如果有）
        if token.spaces > 0 {
            // 空格起始位置 = 符号位置 - 空格数（因为空格都是 ASCII 空格，每个空格占 1 字节）
            let space_start = token.byte_pos.saturating_sub(token.spaces);
            // 确保 space_start 不小于 last_byte
            if space_start > last_byte {
                // 添加中间可能遗漏的普通文本（例如注释或无效字符）
                let text = &code[last_byte..space_start];
                job.append(text, 0.0, TextFormat::default());
            }
            if space_start < token.byte_pos {
                let space_text = &code[space_start..token.byte_pos];
                job.append(space_text, 0.0, TextFormat::default());
            }
        }

        // 符号本身长度（所有操作符均为单字节字符）
        let symbol_len = 1;
        let symbol_end = token.byte_pos + symbol_len;
        if symbol_end <= code.len() {
            let symbol_text = &code[token.byte_pos..symbol_end];
            let format = token_format(token);
            job.append(symbol_text, 0.0, format);
            last_byte = symbol_end;
        } else {
            // 如果符号结束位置超出字符串，说明 token 无效，忽略并记录 last_byte 为当前位置
            last_byte = token.byte_pos;
        }
    }

    // 添加末尾剩余文本
    if last_byte < code.len() {
        let rest = &code[last_byte..];
        job.append(rest, 0.0, TextFormat::default());
    }

    job
}

fn stardust(args: Vec<String>) {
    if args.len() < 2 {
        return
    }

    // 检查是否为字符转换模式
    if args[1] == "--stardust" || args[1] == "-s" {
        // 字符转换模式
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
    }

    // help模式
    if args[1] == "--help" {
        print_usage(&args[0]);
        process::exit(0);
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

    let unwind_source = match preprocess(&source) {
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