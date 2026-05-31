//! Stardust → LLVM IR → 二进制 编译模块
//!
//! # 架构
//!
//! ```text
//! ParseResult → [optimizer] → [compiler] → .ll 文本 → [llc+clang] → 可执行文件
//! ```
//!
//! # 使用
//!
//! ```rust,ignore
//! use esolang_stardust::codegen::{compile_to_ir, compile_to_exe, CodeGenConfig};
//!
//! let config = CodeGenConfig::default();
//! let ir_text = compile_to_ir(&parse_result, &config);
//! compile_to_exe(&parse_result, "output_binary", &config)?;
//! ```

pub mod compiler;
pub mod intrinsics;
pub mod optimizer;

use crate::stardust::ParseResult;
use std::path::Path;
use std::process::Command;
use std::{fs, io};

// ── 配置 ──────────────────────────────────────────────────────

/// 栈大小上限（i64 元素数）
const DEFAULT_STACK_SIZE: usize = 1_048_576; // 1M elements = 8 MB
/// 堆大小上限（i64 元素数）
const DEFAULT_HEAP_SIZE: usize = 1_048_576; // 1M elements = 8 MB
/// 最大调用深度
const DEFAULT_MAX_CALL_DEPTH: usize = 256;

#[derive(Debug, Clone)]
pub struct CodeGenConfig {
    pub stack_size: usize,
    pub heap_size: usize,
    pub max_call_depth: usize,
    /// 优化级别：传递给 clang 的 -O 参数
    pub optimization: u8, // 0-3
    /// 是否保留中间文件
    pub keep_temp: bool,
}

impl Default for CodeGenConfig {
    fn default() -> Self {
        Self {
            stack_size: DEFAULT_STACK_SIZE,
            heap_size: DEFAULT_HEAP_SIZE,
            max_call_depth: DEFAULT_MAX_CALL_DEPTH,
            optimization: 0,
            keep_temp: false,
        }
    }
}

// ── 错误类型 ──────────────────────────────────────────────────

#[derive(Debug)]
pub enum CodeGenError {
    /// 生成 LLVM IR 时的内部错误
    Internal(String),
    /// 系统工具链缺失（llc, clang）
    ToolchainMissing { tool: String },
    /// 工具链执行失败
    ToolchainError { tool: String, message: String },
    /// I/O 错误
    Io(io::Error),
}

impl std::fmt::Display for CodeGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            CodeGenError::Internal(msg) => write!(f, "codegen internal error: {}", msg),
            CodeGenError::ToolchainMissing { tool } => {
                write!(f, "tool '{}' not found — install LLVM/clang", tool)
            }
            CodeGenError::ToolchainError { tool, message } => {
                write!(f, "{} error: {}", tool, message)
            }
            CodeGenError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for CodeGenError {}

impl From<io::Error> for CodeGenError {
    fn from(e: io::Error) -> Self {
        CodeGenError::Io(e)
    }
}

// ── 公开 API ──────────────────────────────────────────────────

/// 将 ParseResult 编译为 LLVM IR 文本。
///
/// 这是编译流水线的第一步，生成的 .ll 文本可以：
/// - 写入文件，用 `llc` 编译为 .o
/// - 直接用 `clang` 编译为可执行文件
/// - 用于调试和分析
pub fn compile_to_ir(parse_result: &ParseResult, config: &CodeGenConfig) -> String {
    compiler::compile_to_ir(parse_result, config)
}

/// 将 ParseResult 编译为 LLVM IR 文本并写入文件。
pub fn compile_to_ir_file(
    parse_result: &ParseResult,
    output_path: &Path,
    config: &CodeGenConfig,
) -> Result<(), CodeGenError> {
    let ir = compile_to_ir(parse_result, config);
    fs::write(output_path, &ir)?;
    Ok(())
}

/// 将 ParseResult 编译为目标文件 (.o)。
///
/// 需要系统安装 `llc`（LLVM 静态编译器）。
pub fn compile_to_object(
    parse_result: &ParseResult,
    output_path: &Path,
    config: &CodeGenConfig,
) -> Result<(), CodeGenError> {
    // 1. 生成 .ll 文件
    let ll_path = output_path.with_extension("ll");
    compile_to_ir_file(parse_result, &ll_path, config)?;

    // 2. 调用 llc 编译为 .o
    let status = Command::new("llc")
        .args([
            &format!("-O{}", config.optimization),
            "-filetype=obj",
            ll_path.to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|_e| CodeGenError::ToolchainMissing {
            tool: "llc".into(),
        })?;

    if !status.success() {
        return Err(CodeGenError::ToolchainError {
            tool: "llc".into(),
            message: format!("exit code {}", status.code().unwrap_or(-1)),
        });
    }

    // 3. 清理（可选）
    if !config.keep_temp {
        let _ = fs::remove_file(&ll_path);
    }

    Ok(())
}

/// 将 ParseResult 编译为可执行二进制文件。
///
/// 需要系统安装 `clang`（C 编译器 / LLVM 前端）。
/// 编译产物会链接 libc。
pub fn compile_to_exe(
    parse_result: &ParseResult,
    output_path: &Path,
    config: &CodeGenConfig,
) -> Result<(), CodeGenError> {
    // 1. 生成 .ll 文件
    let ll_path = output_path.with_extension("ll");
    compile_to_ir_file(parse_result, &ll_path, config)?;

    // 2. 用 clang 直接编译 .ll → 可执行文件
    let opt_flag = format!("-O{}", config.optimization);
    let status = Command::new("clang")
        .args([
            &opt_flag,
            ll_path.to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|_e| CodeGenError::ToolchainMissing {
            tool: "clang".into(),
        })?;

    if !status.success() {
        return Err(CodeGenError::ToolchainError {
            tool: "clang".into(),
            message: format!("exit code {}", status.code().unwrap_or(-1)),
        });
    }

    // 3. 清理
    if !config.keep_temp {
        let _ = fs::remove_file(&ll_path);
    }

    Ok(())
}

/// 查询系统工具链是否可用。
pub fn check_toolchain() -> ToolchainStatus {
    let has_llc = Command::new("llc").arg("--version").output().is_ok();
    let has_clang = Command::new("clang").arg("--version").output().is_ok();
    let has_opt = Command::new("opt").arg("--version").output().is_ok();

    ToolchainStatus {
        llc: has_llc,
        clang: has_clang,
        opt: has_opt,
    }
}

#[derive(Debug)]
pub struct ToolchainStatus {
    pub llc: bool,
    pub clang: bool,
    pub opt: bool,
}

impl ToolchainStatus {
    pub fn can_compile(&self) -> bool {
        self.llc && self.clang
    }
}
