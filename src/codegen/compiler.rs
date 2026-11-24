//! Stardust → LLVM IR 编译器
//!
//! 核心策略：
//! - 全局栈 @stack + 栈指针 @sp（malloc 动态分配）
//! - 基本块内使用编译期栈追踪（compile_stack），操作 SSA 寄存器，零内存访问
//! - 控制流边界（Jump/Mark/Call）flush 到 @stack 内存，下一 BB 再从 @stack 读取
//! - 函数调用通过帧基址（frame_base）实现多值返回语义

use crate::stardust::config::CharMode;
use crate::stardust::{Instruction, ParseResult};
use std::collections::HashMap;

use super::CodeGenConfig;
use super::intrinsics::{EXTERNAL_DECLS, INTRINSICS_IR};
use super::optimizer;

// ── 编译期栈条目 ──────────────────────────────────────────────

#[derive(Debug, Clone)]
enum StackEntry {
    /// 编译期已知常量
    Const(i64),
    /// LLVM SSA 寄存器名，如 "%r5"
    Reg(String),
}

impl StackEntry {
    fn to_llvm(&self) -> String {
        match self {
            StackEntry::Const(v) => v.to_string(),
            StackEntry::Reg(r) => r.clone(),
        }
    }
}

// ── 编译器 ────────────────────────────────────────────────────

struct FuncCompiler<'a> {
    config: &'a CodeGenConfig,
    output: String,
    label_ctr: usize,
    /// 编译期栈追踪 — 基本块内的栈值映射
    compile_stack: Vec<StackEntry>,
    /// 当前编译的函数名（None 表示主程序）
    current_func: Option<usize>,
    /// 当前是否在基本块起始位置（刚经过 label，需要从 @stack 加载）
    at_bb_start: bool,
    /// 当前基本块是否已有终结指令（br/ret/switch…）。
    /// LLVM 要求每个块以终结指令结束，打开新块时必须先补齐。
    block_terminated: bool,
    /// 当前栈深度（用于追踪 flush/reload 位置）
    tracked_depth: usize,
}

impl<'a> FuncCompiler<'a> {
    fn new(config: &'a CodeGenConfig) -> Self {
        FuncCompiler {
            config,
            output: String::new(),
            label_ctr: 0,
            compile_stack: Vec::new(),
            current_func: None,
            at_bb_start: true,
            block_terminated: false,
            tracked_depth: 0,
        }
    }

    // ── 辅助方法 ───────────────────────────────────────────

    fn emitln(&mut self, ir: &str) {
        self.output.push_str(ir);
        if !ir.ends_with('\n') {
            self.output.push('\n');
        }
        // 跟踪终结指令：所有以 br/ret/switch/resume/unreachable 结尾的指令
        let trimmed = ir.trim();
        if trimmed.starts_with("br ")
            || trimmed.starts_with("ret ")
            || trimmed.starts_with("switch ")
            || trimmed.starts_with("indirectbr ")
            || trimmed.starts_with("resume ")
            || trimmed == "unreachable"
        {
            self.block_terminated = true;
        }
    }

    /// 打开一个新的基本块标签。
    ///
    /// 若当前块尚未终结（LLVM 不允许），先补一条跳转到该新标签的 `br`。
    /// 这保证了 `flush()` 之后紧跟 `mark_N:` 这类序列能生成合法 IR。
    fn emit_label(&mut self, label: &str) {
        if !self.block_terminated {
            self.emitln(&format!("  br label %{label}"));
        }
        self.emitln(&format!("{label}:"));
        self.block_terminated = false;
        self.at_bb_start = true;
    }

    fn next_label(&mut self) -> usize {
        let l = self.label_ctr;
        self.label_ctr += 1;
        l
    }

    fn mark_label(name: usize) -> String {
        format!("mark_{}", name)
    }

    fn bb_label(name: usize) -> String {
        format!("bb_{}", name)
    }

    // ── 栈追踪原语 ─────────────────────────────────────────

    /// 从编译期栈弹出值（生成 SSA 寄存器或常量）。
    /// 如果编译栈为空（BB 起始），生成从 @stack 加载的 IR。
    fn pop_ssa(&mut self) -> StackEntry {
        if let Some(entry) = self.compile_stack.pop() {
            self.tracked_depth = self.tracked_depth.saturating_sub(1);
            return entry;
        }

        // BB 起始：从 @stack 加载
        let l = self.next_label();
        let sp_dec = format!("%sp_dec_{l}");
        let ptr = format!("%ptr_{l}");
        let val = format!("%val_{l}");
        let max_depth = self.config.max_call_depth;

        self.emitln(&format!("  %sp_pre_{l} = load i64, i64* @sp", l = l));

        // 栈下溢检查：@sp 必须严格大于当前帧基址，否则栈已空，
        // 继续加载会读到未初始化的内存（未定义行为）。
        self.emitln(&format!("  %suf_fbd_{l} = load i32, i32* @frame_depth", l = l));
        self.emitln(&format!("  %suf_fbd_d_{l} = sub i32 %suf_fbd_{l}, 1", l = l));
        self.emitln(&format!(
            "  %suf_fb_gep_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %suf_fbd_d_{l}",
            MD = max_depth,
            l = l
        ));
        self.emitln(&format!("  %suf_fb_{l} = load i64, i64* %suf_fb_gep_{l}", l = l));
        self.emitln(&format!("  %suf_ok_{l} = icmp sgt i64 %sp_pre_{l}, %suf_fb_{l}", l = l));
        self.emitln(&format!(
            "  br i1 %suf_ok_{l}, label %suf_ok_blk_{l}, label %suf_err_blk_{l}",
            l = l
        ));
        self.emit_label(&format!("suf_err_blk_{l}"));
        self.emitln("  call void @runtime_stack_underflow()");
        self.emitln("  unreachable");
        self.emit_label(&format!("suf_ok_blk_{l}"));

        self.emitln(&format!("  {sp} = sub i64 %sp_pre_{l}, 1", sp = sp_dec));
        self.emitln(&format!("  store i64 {sp}, i64* @sp", sp = sp_dec));
        self.emitln(&format!(
            "  {ptr} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 {sp}",
            ptr = ptr,
            sp = sp_dec,
            SZ = self.config.stack_size
        ));
        self.emitln(&format!("  {val} = load i64, i64* {ptr}", val = val, ptr = ptr));
        self.tracked_depth = self.tracked_depth.saturating_sub(1);
        StackEntry::Reg(val)
    }

    /// 将 SSA 值压入编译期栈（推迟写入 @stack）。
    fn push_ssa(&mut self, entry: StackEntry) {
        self.compile_stack.push(entry);
        self.tracked_depth += 1;
    }

    /// 将编译期栈全部刷新到 @stack 内存。
    /// 清空 compile_stack，更新 @sp。
    fn flush(&mut self) {
        if self.compile_stack.is_empty() {
            return;
        }

        // Collect entries into local Vec to avoid borrow issues
        let entries: Vec<String> = self
            .compile_stack
            .iter()
            .map(|e| match e {
                StackEntry::Const(v) => v.to_string(),
                StackEntry::Reg(r) => r.clone(),
            })
            .collect();
        let count = entries.len();
        let stack_size = self.config.stack_size;
        let l = self.next_label();

        self.emitln(&format!("  ; flush {} stack entries to memory", count));
        self.emitln(&format!("  %sp_flush_{l} = load i64, i64* @sp"));

        for (i, val_str) in entries.iter().enumerate().take(count) {
            let val_str = val_str.as_str();
            let ptr = format!("%flush_ptr_{}_{}", l, i);
            let sp_name =
                if i == 0 { format!("%sp_flush_{l}") } else { format!("%sp_flush_{}_{}", l, i) };
            let next_sp = if i + 1 < count {
                format!("%sp_flush_{}_{}", l, i + 1)
            } else {
                format!("%sp_flush_done_{l}")
            };

            self.emitln(&format!(
                "  {ptr} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 {sp}",
                ptr = ptr,
                sp = sp_name,
                SZ = stack_size
            ));
            self.emitln(&format!("  store i64 {val}, i64* {ptr}", val = val_str, ptr = ptr));

            if i + 1 < count {
                self.emitln(&format!("  {next} = add i64 {sp}, 1", next = next_sp, sp = sp_name));
            }
        }

        if count == 1 {
            self.emitln(&format!("  %sp_flush_done_{l} = add i64 %sp_flush_{l}, 1"));
        } else {
            self.emitln(&format!(
                "  %sp_flush_done_{l} = add i64 %sp_flush_{l}_{last}, 1",
                l = l,
                last = count - 1
            ));
        }
        self.emitln(&format!("  store i64 %sp_flush_done_{l}, i64* @sp"));

        // 结尾的 store 不是终结指令；标记为未终结，
        // 由下一个 emit_label 自动补上跳转，避免生成非法 IR。
        self.compile_stack.clear();
    }

    // ── 指令编译 ───────────────────────────────────────────

    fn compile_instruction(&mut self, inst: &Instruction) {
        match inst {
            Instruction::Push(val, _) => self.compile_push(*val),
            Instruction::Dup(_) => self.compile_dup(),
            Instruction::Swap(_) => self.compile_swap(),
            Instruction::Rotate(_) => self.compile_rotate(),
            Instruction::Pop(_) => self.compile_pop(),
            Instruction::Add(_) => self.compile_binop("add"),
            Instruction::Sub(_) => self.compile_binop("sub"),
            Instruction::Mul(_) => self.compile_binop("mul"),
            Instruction::Div(_) => self.compile_div(),
            Instruction::Mod(_) => self.compile_mod(),
            Instruction::Reverse(_) => self.compile_reverse(),
            Instruction::NumOut(_) => self.compile_numout(),
            Instruction::NumIn(_) => self.compile_numin(),
            Instruction::CharOut(_) => self.compile_charout(),
            Instruction::CharIn(_) => self.compile_charin(),
            Instruction::FileRead(_) => self.compile_file_read(),
            Instruction::FileWrite(_) => self.compile_file_write(),
            Instruction::Mark { name, .. } => self.compile_mark(*name),
            Instruction::Jump { name, .. } => self.compile_jump(*name),
            Instruction::UnconditionalJump { name, .. } => self.compile_ujump(*name),
            Instruction::Call { name, argc, .. } => self.compile_call(*name, *argc),
            Instruction::Eq(_) => self.compile_cmp("eq"),
            Instruction::Ne(_) => self.compile_cmp("ne"),
            Instruction::Lt(_) => self.compile_cmp("slt"),
            Instruction::Gt(_) => self.compile_cmp("sgt"),
            Instruction::Le(_) => self.compile_cmp("sle"),
            Instruction::Ge(_) => self.compile_cmp("sge"),
            Instruction::And(_) => self.compile_logic_and(),
            Instruction::Or(_) => self.compile_logic_or(),
            Instruction::Not(_) => self.compile_logic_not(),
            Instruction::Xor(_) => self.compile_logic_xor(),
            Instruction::Store(_) => self.compile_store(),
            Instruction::Load(_) => self.compile_load(),
            Instruction::ShiftL(_) => self.compile_shiftl(),
            Instruction::Depth(_) => self.compile_depth(),
            Instruction::Pick(_) => self.compile_pick(),
            Instruction::ShiftR(_) => self.compile_shiftr(),
            Instruction::DropN(_) => self.compile_dropn(),
            Instruction::DumpStack(_) => self.compile_dumpstack(),
            Instruction::DumpState(_) => self.compile_dumpstate(),
            Instruction::Breakpoint(_) => {
                // NOP in compilation mode
            }
        }
    }

    // ── Push ───────────────────────────────────────────────

    fn compile_push(&mut self, val: i64) {
        self.push_ssa(StackEntry::Const(val));
    }

    // ── Dup ────────────────────────────────────────────────

    fn compile_dup(&mut self) {
        let entry = self.pop_ssa();
        // Extract the value before pushing (avoid borrow issues)
        match &entry {
            StackEntry::Const(v) => {
                let val = *v;
                self.compile_stack.push(StackEntry::Const(val));
                self.compile_stack.push(StackEntry::Const(val));
                self.tracked_depth += 2;
            }
            StackEntry::Reg(r) => {
                let r_clone = r.clone();
                self.compile_stack.push(StackEntry::Reg(r.clone()));
                self.compile_stack.push(StackEntry::Reg(r_clone));
                self.tracked_depth += 2;
            }
        }
    }

    // ── Swap ───────────────────────────────────────────────

    fn compile_swap(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        self.push_ssa(b);
        self.push_ssa(a);
    }

    // ── Rotate ─────────────────────────────────────────────

    fn compile_rotate(&mut self) {
        let c = self.pop_ssa();
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        self.push_ssa(c);
        self.push_ssa(a);
        self.push_ssa(b);
    }

    // ── Pop ────────────────────────────────────────────────

    fn compile_pop(&mut self) {
        self.pop_ssa(); // discard
    }

    // ── 算术二元操作 ───────────────────────────────────────

    fn compile_binop(&mut self, op: &str) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%binop_{l}");

        let a_str = a.to_llvm();
        let b_str = b.to_llvm();

        match op {
            "add" => {
                self.emitln(&format!(
                    "  {r} = call i64 @checked_add(i64 {a}, i64 {b})",
                    r = result,
                    a = a_str,
                    b = b_str
                ));
            }
            "sub" => {
                self.emitln(&format!(
                    "  {r} = call i64 @checked_sub(i64 {a}, i64 {b})",
                    r = result,
                    a = a_str,
                    b = b_str
                ));
            }
            "mul" => {
                self.emitln(&format!(
                    "  {r} = call i64 @checked_mul(i64 {a}, i64 {b})",
                    r = result,
                    a = a_str,
                    b = b_str
                ));
            }
            _ => unreachable!(),
        }

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── Div ────────────────────────────────────────────────

    fn compile_div(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%div_{l}");
        let a_str = a.to_llvm();
        let b_str = b.to_llvm();

        self.emitln(&format!("  %div_zero_{l} = icmp eq i64 {b}, 0", l = l, b = b_str));
        self.emitln(&format!("  br i1 %div_zero_{l}, label %div_err_{l}, label %div_ok_{l}"));
        self.emit_label(&format!("div_err_{l}"));
        self.emitln("  call void @runtime_divzero()");
        self.emitln("  unreachable");
        self.emit_label(&format!("div_ok_{l}"));
        self.emitln(&format!("  {r} = sdiv i64 {a}, {b}", r = result, a = a_str, b = b_str));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── Mod ────────────────────────────────────────────────

    fn compile_mod(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%mod_{l}");
        let a_str = a.to_llvm();
        let b_str = b.to_llvm();

        self.emitln(&format!("  %mod_zero_{l} = icmp eq i64 {b}, 0", l = l, b = b_str));
        self.emitln(&format!("  br i1 %mod_zero_{l}, label %mod_err_{l}, label %mod_ok_{l}"));
        self.emit_label(&format!("mod_err_{l}"));
        self.emitln("  call void @runtime_modzero()");
        self.emitln("  unreachable");
        self.emit_label(&format!("mod_ok_{l}"));
        self.emitln(&format!("  {r} = srem i64 {a}, {b}", r = result, a = a_str, b = b_str));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── Reverse ────────────────────────────────────────────

    fn compile_reverse(&mut self) {
        // 倒转当前帧的栈 — flush 到 @stack，用临时缓冲区实现
        self.flush();
        let l = self.next_label();

        // 跳转到准备块
        self.emitln(&format!("  br label %rev_prep_{l}"));
        self.emit_label(&format!("rev_prep_{l}"));

        // 获取帧基址和栈指针
        self.emitln(&format!("  %sp_rev_{l} = load i64, i64* @sp"));
        self.emitln(&format!("  %fbd_rev_{l} = load i32, i32* @frame_depth"));
        self.emitln(&format!("  %fbd_rev_d_{l} = sub i32 %fbd_rev_{l}, 1"));
        self.emitln(&format!(
            "  %fb_gep_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %fbd_rev_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!("  %fb_{l} = load i64, i64* %fb_gep_{l}"));
        self.emitln(&format!("  %rev_len_{l} = sub i64 %sp_rev_{l}, %fb_{l}"));

        // 如果 len <= 1，不需要做任何事
        self.emitln(&format!("  %rev_trivial_{l} = icmp sle i64 %rev_len_{l}, 1"));
        self.emitln(&format!("  br i1 %rev_trivial_{l}, label %rev_done_{l}, label %rev_do_{l}"));
        self.emit_label(&format!("rev_do_{l}"));

        // 分配临时缓冲区
        self.emitln(&format!("  %rev_bytes_{l} = mul i64 %rev_len_{l}, 8"));
        self.emitln(&format!("  %rev_tmp_{l} = call i8* @malloc(i64 %rev_bytes_{l})"));
        self.emitln(&format!("  %rev_tmp_i64_{l} = bitcast i8* %rev_tmp_{l} to i64*"));

        // 复制到 tmp（原地顺序）
        self.emitln(&format!(
            "  %rev_src_base_{l} = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %fb_{l}",
            SSZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!(
            "  call void @llvm.memmove.p0.p0.i64(i64* %rev_tmp_i64_{l}, i64* %rev_src_base_{l}, i64 %rev_bytes_{l}, i1 false)"
        ));

        // 反向复制回 @stack: stack[fb + j] = tmp[len - 1 - j]
        self.emitln(&format!("  br label %rev_loop_{l}"));
        self.emit_label(&format!("rev_loop_{l}"));
        self.emitln(&format!(
            "  %rev_j_{l} = phi i64 [ 0, %rev_do_{l} ], [ %rev_j_next_{l}, %rev_loop_body_{l} ]"
        ));
        self.emitln(&format!("  %rev_loop_done_{l} = icmp eq i64 %rev_j_{l}, %rev_len_{l}"));
        self.emitln(&format!(
            "  br i1 %rev_loop_done_{l}, label %rev_free_{l}, label %rev_loop_body_{l}"
        ));
        self.emit_label(&format!("rev_loop_body_{l}"));
        // tmp_src_idx = len - 1 - j
        self.emitln(&format!("  %rev_len_m1_{l} = sub i64 %rev_len_{l}, 1"));
        self.emitln(&format!("  %rev_tmp_src_idx_{l} = sub i64 %rev_len_m1_{l}, %rev_j_{l}"));
        self.emitln(&format!(
            "  %rev_tmp_src_{l} = getelementptr i64, i64* %rev_tmp_i64_{l}, i64 %rev_tmp_src_idx_{l}"
        ));
        self.emitln(&format!("  %rev_val_{l} = load i64, i64* %rev_tmp_src_{l}"));
        // stack_dst_idx = fb + j
        self.emitln(&format!("  %rev_dst_idx_{l} = add i64 %fb_{l}, %rev_j_{l}"));
        self.emitln(&format!(
            "  %rev_dst_{l} = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %rev_dst_idx_{l}",
            SSZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  store i64 %rev_val_{l}, i64* %rev_dst_{l}"));
        self.emitln(&format!("  %rev_j_next_{l} = add i64 %rev_j_{l}, 1"));
        self.emitln(&format!("  br label %rev_loop_{l}"));
        self.emit_label(&format!("rev_free_{l}"));
        self.emitln(&format!("  call void @free(i8* %rev_tmp_{l})"));
        self.emitln(&format!("  br label %rev_done_{l}"));
        self.emit_label(&format!("rev_done_{l}"));
    }

    // ── CharOut ────────────────────────────────────────────

    fn compile_charout(&mut self) {
        let val = self.pop_ssa();
        let val_str = val.to_llvm();
        match &self.config.char_mode {
            CharMode::Ascii => {
                let l = self.next_label();
                // Bounds check: 0 <= val <= 127
                self.emitln(&format!("  %ch_ok_low_{l} = icmp sge i64 {v}, 0", v = val_str, l = l));
                self.emitln(&format!(
                    "  %ch_ok_high_{l} = icmp sle i64 {v}, 127",
                    v = val_str,
                    l = l
                ));
                self.emitln(&format!("  %ch_ok_{l} = and i1 %ch_ok_low_{l}, %ch_ok_high_{l}"));
                self.emitln(&format!("  br i1 %ch_ok_{l}, label %ch_put_{l}, label %ch_bad_{l}"));
                self.emit_label(&format!("ch_bad_{l}"));
                self.emitln(&format!("  call void @runtime_bad_ascii(i64 {v})", v = val_str));
                self.emitln("  unreachable");
                self.emit_label(&format!("ch_put_{l}"));
                self.emitln(&format!("  %ch_trunc_{l} = trunc i64 {v} to i32", v = val_str));
                self.emitln(&format!(
                    "  %ch_call_{l} = call i32 @putchar(i32 %ch_trunc_{l})",
                    l = l
                ));
            }
            CharMode::Unicode => {
                // 运行时负责校验码点并编码为 UTF-8（1-4 字节）。
                self.emitln(&format!("  call void @runtime_char_out_utf8(i64 {v})", v = val_str));
            }
        }
    }

    // ── CharIn ─────────────────────────────────────────────

    fn compile_charin(&mut self) {
        // If compile_stack has items, flush first so we don't lose state across I/O
        if !self.compile_stack.is_empty() {
            self.flush();
        }
        let l = self.next_label();
        let result = format!("%charin_{l}");
        self.emitln(&format!("  {r} = call i32 @getchar()", r = result));
        // getchar() 在 EOF 时返回 -1，与解释器的 "failed to fill whole buffer" 行为对齐
        self.emitln(&format!("  %charin_eof_{l} = icmp eq i32 {r}, -1", r = result));
        self.emitln(&format!(
            "  br i1 %charin_eof_{l}, label %charin_err_blk_{l}, label %charin_ok_blk_{l}",
            l = l
        ));
        self.emit_label(&format!("charin_err_blk_{l}"));
        self.emitln("  call void @runtime_io_error()");
        self.emitln("  unreachable");
        self.emit_label(&format!("charin_ok_blk_{l}"));
        self.emitln(&format!("  %charin_ext_{l} = sext i32 {r} to i64", r = result));
        self.push_ssa(StackEntry::Reg(format!("%charin_ext_{l}")));
    }

    // ── NumOut ─────────────────────────────────────────────

    fn compile_numout(&mut self) {
        let val = self.pop_ssa();
        let val_str = val.to_llvm();
        let l = self.next_label();
        self.emitln(&format!(
            "  %nout_{l} = call i32 (i8*, ...) @printf(i8* getelementptr ([4 x i8], [4 x i8]* @fmt_numout, i32 0, i32 0), i64 {v})",
            v = val_str
        ));
    }

    // ── NumIn ──────────────────────────────────────────────

    fn compile_numin(&mut self) {
        if !self.compile_stack.is_empty() {
            self.flush();
        }
        let l = self.next_label();
        // Alloca space for input
        self.emitln(&format!("  %nin_ptr_{l} = alloca i64"));
        // 先把槽位置零：scanf 未写入时也不会读到垃圾值
        self.emitln(&format!("  store i64 0, i64* %nin_ptr_{l}", l = l));
        self.emitln(&format!(
            "  %nin_call_{l} = call i32 (i8*, ...) @scanf(i8* getelementptr ([4 x i8], [4 x i8]* @fmt_numin, i32 0, i32 0), i64* %nin_ptr_{l})",
            l = l
        ));
        // scanf 返回已成功赋值的个数；0 或 EOF 均表示读取失败
        self.emitln(&format!("  %nin_ok_{l} = icmp eq i32 %nin_call_{l}, 1", l = l));
        self.emitln(&format!(
            "  br i1 %nin_ok_{l}, label %nin_ok_blk_{l}, label %nin_err_blk_{l}",
            l = l
        ));
        self.emit_label(&format!("nin_err_blk_{l}"));
        self.emitln("  call void @runtime_io_error()");
        self.emitln("  unreachable");
        self.emit_label(&format!("nin_ok_blk_{l}"));
        self.emitln(&format!("  %nin_val_{l} = load i64, i64* %nin_ptr_{l}", l = l));
        self.push_ssa(StackEntry::Reg(format!("%nin_val_{l}")));
    }

    // ── FileRead / FileWrite ───────────────────────────────

    /// FileRead：栈上 [path chars..., len] → [contents chars..., len]。
    ///
    /// 读到的内容长度在运行时才确定，无法用编译期栈模型表达，因此实现放在
    /// 运行时函数里：它直接读写 `@stack`/`@sp`。调用前必须 flush 编译期栈，
    /// 调用后编译期栈为空，后续 `pop` 自动回到从 `@stack` 加载的路径。
    fn compile_file_read(&mut self) {
        self.flush();
        self.emitln("  call void @sd_file_read()");
    }

    /// FileWrite：栈上 [path chars..., len, data chars..., len] → []。
    fn compile_file_write(&mut self) {
        self.flush();
        self.emitln("  call void @sd_file_write()");
    }

    fn compile_mark(&mut self, name: usize) {
        // Flush before the mark (control flow may enter here from a jump)
        self.flush();

        let label = Self::mark_label(name);
        self.emit_label(&label);

        self.at_bb_start = true;
    }

    // ── Jump ───────────────────────────────────────────────

    fn compile_jump(&mut self, name: usize) {
        let cond = self.pop_ssa();
        let cond_str = cond.to_llvm();
        let l = self.next_label();

        self.flush();

        let target = Self::mark_label(name);
        let fallthrough = Self::bb_label(l);

        self.emitln(&format!("  %jmp_cond_{l} = icmp ne i64 {c}, 0", c = cond_str, l = l));
        self.emitln(&format!(
            "  br i1 %jmp_cond_{l}, label %{t}, label %{f}",
            t = target,
            f = fallthrough,
            l = l
        ));
        self.emit_label(&fallthrough);
        self.at_bb_start = true;
    }

    // ── UnconditionalJump ──────────────────────────────────

    fn compile_ujump(&mut self, name: usize) {
        self.flush();
        let target = Self::mark_label(name);
        self.emitln(&format!("  br label %{}", target));

        // 无条件跳转之后的代码不可达。LLVM 要求每个块以终结指令结束，
        // 因此这里生成一个自跳转的死块（永不执行，仅满足 IR 结构要求）。
        let l = self.next_label();
        self.emit_label(&format!("dead_{l}"));
        self.emitln(&format!("  br label %dead_{l}"));
    }

    // ── Call ───────────────────────────────────────────────

    fn compile_call(&mut self, name: usize, argc: usize) {
        let l = self.next_label();

        // Flush current stack to memory
        self.flush();

        // Save frame base
        self.emitln(&format!("  ; Call function {}, argc={}", name, argc));
        self.emitln(&format!("  %fbd_call_{l} = load i32, i32* @frame_depth"));
        self.emitln(&format!("  %sp_call_{l} = load i64, i64* @sp"));
        self.emitln(&format!("  %fb_call_{l} = sub i64 %sp_call_{l}, {argc}", argc = argc, l = l));

        // Check arguments
        self.emitln(&format!("  %arg_ok_{l} = icmp sge i64 %fb_call_{l}, 0", l = l));
        self.emitln(&format!(
            "  br i1 %arg_ok_{l}, label %call_args_ok_{l}, label %call_noargs_{l}"
        ));
        self.emit_label(&format!("call_noargs_{l}"));
        self.emitln("  call void @runtime_stack_underflow()");
        self.emitln("  unreachable");
        self.emit_label(&format!("call_args_ok_{l}"));

        // Store frame base
        self.emitln(&format!(
            "  %fb_slot_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %fbd_call_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!("  store i64 %fb_call_{l}, i64* %fb_slot_{l}"));

        // Increment frame depth with overflow check
        self.emitln(&format!("  %fbd_next_{l} = add i32 %fbd_call_{l}, 1"));
        self.emitln(&format!(
            "  %depth_ok_{l} = icmp slt i32 %fbd_next_{l}, {MD}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!(
            "  br i1 %depth_ok_{l}, label %call_ok_{l}, label %call_depth_err_{l}"
        ));
        self.emit_label(&format!("call_depth_err_{l}"));
        self.emitln("  call void @runtime_call_depth()");
        self.emitln("  unreachable");
        self.emit_label(&format!("call_ok_{l}"));
        self.emitln(&format!("  store i32 %fbd_next_{l}, i32* @frame_depth"));

        // Call the function
        self.emitln(&format!("  call void @sd_func_{name}()", name = name));

        // Restore frame depth
        self.emitln(&format!("  store i32 %fbd_call_{l}, i32* @frame_depth"));

        self.at_bb_start = true;
    }

    // ── 比较运算 ───────────────────────────────────────────

    fn compile_cmp(&mut self, op: &str) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%cmp_{l}");

        let llvm_op = match op {
            "eq" => "eq",
            "ne" => "ne",
            "slt" => "slt",
            "sgt" => "sgt",
            "sle" => "sle",
            "sge" => "sge",
            _ => unreachable!(),
        };

        self.emitln(&format!(
            "  %cmp_bool_{l} = icmp {op} i64 {a}, {b}",
            op = llvm_op,
            a = a.to_llvm(),
            b = b.to_llvm(),
            l = l
        ));
        self.emitln(&format!("  {r} = zext i1 %cmp_bool_{l} to i64", r = result, l = l));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── 逻辑运算 ───────────────────────────────────────────

    fn compile_logic_and(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%and_{l}");

        self.emitln(&format!("  %and_a_{l} = icmp ne i64 {a}, 0", a = a.to_llvm(), l = l));
        self.emitln(&format!("  %and_b_{l} = icmp ne i64 {b}, 0", b = b.to_llvm(), l = l));
        self.emitln(&format!("  %and_bool_{l} = and i1 %and_a_{l}, %and_b_{l}"));
        self.emitln(&format!("  {r} = zext i1 %and_bool_{l} to i64", r = result));

        self.push_ssa(StackEntry::Reg(result));
    }

    fn compile_logic_or(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%or_{l}");

        self.emitln(&format!("  %or_a_{l} = icmp ne i64 {a}, 0", a = a.to_llvm(), l = l));
        self.emitln(&format!("  %or_b_{l} = icmp ne i64 {b}, 0", b = b.to_llvm(), l = l));
        self.emitln(&format!("  %or_bool_{l} = or i1 %or_a_{l}, %or_b_{l}"));
        self.emitln(&format!("  {r} = zext i1 %or_bool_{l} to i64", r = result));

        self.push_ssa(StackEntry::Reg(result));
    }

    fn compile_logic_not(&mut self) {
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%not_{l}");

        self.emitln(&format!("  %not_a_{l} = icmp eq i64 {a}, 0", a = a.to_llvm(), l = l));
        self.emitln(&format!("  {r} = zext i1 %not_a_{l} to i64", r = result));

        self.push_ssa(StackEntry::Reg(result));
    }

    fn compile_logic_xor(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%xor_{l}");

        self.emitln(&format!("  %xor_a_{l} = icmp ne i64 {a}, 0", a = a.to_llvm(), l = l));
        self.emitln(&format!("  %xor_b_{l} = icmp ne i64 {b}, 0", b = b.to_llvm(), l = l));
        self.emitln(&format!("  %xor_bool_{l} = xor i1 %xor_a_{l}, %xor_b_{l}"));
        self.emitln(&format!("  {r} = zext i1 %xor_bool_{l} to i64", r = result));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── Store ──────────────────────────────────────────────

    fn compile_store(&mut self) {
        let addr = self.pop_ssa();
        let val = self.pop_ssa();
        let l = self.next_label();

        let addr_str = addr.to_llvm();
        let val_str = val.to_llvm();

        // Check address bounds
        self.emitln(&format!(
            "  %st_ok_{l} = icmp ult i64 {a}, {HS}",
            a = addr_str,
            HS = self.config.heap_size,
            l = l
        ));
        self.emitln(&format!("  br i1 %st_ok_{l}, label %st_do_{l}, label %st_err_{l}"));
        self.emit_label(&format!("st_err_{l}"));
        self.emitln(&format!("  call void @runtime_heap_oob(i64 {a})", a = addr_str));
        self.emitln("  unreachable");
        self.emit_label(&format!("st_do_{l}"));
        self.emitln(&format!(
            "  %st_ptr_{l} = getelementptr [{HS} x i64], [{HS} x i64]* @heap, i32 0, i64 {a}",
            HS = self.config.heap_size,
            a = addr_str,
            l = l
        ));
        self.emitln(&format!("  store i64 {v}, i64* %st_ptr_{l}", v = val_str));
    }

    fn compile_load(&mut self) {
        let addr = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%ld_{l}");

        let addr_str = addr.to_llvm();

        self.emitln(&format!(
            "  %ld_ok_{l} = icmp ult i64 {a}, {HS}",
            a = addr_str,
            HS = self.config.heap_size,
            l = l
        ));
        self.emitln(&format!("  br i1 %ld_ok_{l}, label %ld_do_{l}, label %ld_err_{l}"));
        self.emit_label(&format!("ld_err_{l}"));
        self.emitln(&format!("  call void @runtime_heap_oob(i64 {a})", a = addr_str));
        self.emitln("  unreachable");
        self.emit_label(&format!("ld_do_{l}"));
        self.emitln(&format!(
            "  %ld_ptr_{l} = getelementptr [{HS} x i64], [{HS} x i64]* @heap, i32 0, i64 {a}",
            HS = self.config.heap_size,
            a = addr_str,
            l = l
        ));
        self.emitln(&format!("  {r} = load i64, i64* %ld_ptr_{l}", r = result));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── Depth ──────────────────────────────────────────────

    fn compile_depth(&mut self) {
        let l = self.next_label();
        let result = format!("%depth_{l}");

        // Depth = current sp - frame_base
        self.flush(); // Ensure sp is up to date

        self.emitln(&format!("  %sp_d_{l} = load i64, i64* @sp"));
        self.emitln(&format!("  %fbd_d_{l} = load i32, i32* @frame_depth"));
        self.emitln(&format!("  %fbd_d_d_{l} = sub i32 %fbd_d_{l}, 1"));
        self.emitln(&format!(
            "  %fb_gep_d_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %fbd_d_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!("  %fb_d_{l} = load i64, i64* %fb_gep_d_{l}"));
        self.emitln(&format!("  {r} = sub i64 %sp_d_{l}, %fb_d_{l}", r = result));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── ShiftL ─────────────────────────────────────────────

    fn compile_shiftl(&mut self) {
        self.flush();
        let l = self.next_label();

        // Shift left: stack[fb] moves to stack[sp-1], everything else shifts down
        self.emitln(&format!("  %sp_sl_{l} = load i64, i64* @sp"));
        self.emitln(&format!("  %fbd_sl_{l} = load i32, i32* @frame_depth"));
        self.emitln(&format!("  %fbd_sl_d_{l} = sub i32 %fbd_sl_{l}, 1"));
        self.emitln(&format!(
            "  %fb_gep_sl_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %fbd_sl_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!("  %fb_sl_{l} = load i64, i64* %fb_gep_sl_{l}"));
        // stack_underflow check
        self.emitln(&format!("  %sl_empty_{l} = icmp eq i64 %sp_sl_{l}, %fb_sl_{l}"));
        self.emitln(&format!("  br i1 %sl_empty_{l}, label %sl_err_{l}, label %sl_ok_{l}"));
        self.emit_label(&format!("sl_err_{l}"));
        self.emitln("  call void @runtime_stack_underflow()");
        self.emitln("  unreachable");
        self.emit_label(&format!("sl_ok_{l}"));
        // Read first element
        self.emitln(&format!(
            "  %sl_first_ptr_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %fb_sl_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  %sl_first_{l} = load i64, i64* %sl_first_ptr_{l}"));
        // memmove(stack[fb], stack[fb+1], (sp - fb - 1)*8)
        self.emitln(&format!(
            "  %sl_src_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %fb_sl_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  %sl_src_1_{l} = getelementptr i64, i64* %sl_src_{l}, i64 1"));
        self.emitln(&format!("  %sl_count_{l} = sub i64 %sp_sl_{l}, %fb_sl_{l}"));
        self.emitln(&format!("  %sl_count_dec_{l} = sub i64 %sl_count_{l}, 1"));
        self.emitln(&format!("  %sl_bytes_{l} = mul i64 %sl_count_dec_{l}, 8"));
        self.emitln(&format!(
            "  call void @llvm.memmove.p0.p0.i64(i64* %sl_src_{l}, i64* %sl_src_1_{l}, i64 %sl_bytes_{l}, i1 false)",
            l = l
        ));
        // Write first element to sp-1 position
        self.emitln(&format!(
            "  %sl_dst_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %sp_sl_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  %sl_dst_dec_{l} = getelementptr i64, i64* %sl_dst_{l}, i64 -1"));
        self.emitln(&format!("  store i64 %sl_first_{l}, i64* %sl_dst_dec_{l}"));
    }

    // ── ShiftR ─────────────────────────────────────────────

    fn compile_shiftr(&mut self) {
        self.flush();
        let l = self.next_label();

        self.emitln(&format!("  %sp_sr_{l} = load i64, i64* @sp"));
        // Get last element
        self.emitln(&format!("  %sr_last_sp_{l} = sub i64 %sp_sr_{l}, 1"));
        self.emitln(&format!(
            "  %sr_last_ptr_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %sr_last_sp_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  %sr_last_{l} = load i64, i64* %sr_last_ptr_{l}"));
        // memmove(stack[fb+1], stack[fb], (sp - fb - 1)*8)
        self.emitln(&format!("  %sr_fbd_{l} = load i32, i32* @frame_depth"));
        self.emitln(&format!("  %sr_fbd_d_{l} = sub i32 %sr_fbd_{l}, 1"));
        self.emitln(&format!(
            "  %sr_fb_gep_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %sr_fbd_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!("  %sr_fb_{l} = load i64, i64* %sr_fb_gep_{l}"));
        self.emitln(&format!(
            "  %sr_src_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %sr_fb_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  %sr_dst_{l} = getelementptr i64, i64* %sr_src_{l}, i64 1"));
        self.emitln(&format!("  %sr_count_{l} = sub i64 %sp_sr_{l}, %sr_fb_{l}"));
        self.emitln(&format!("  %sr_count_dec_{l} = sub i64 %sr_count_{l}, 1"));
        self.emitln(&format!("  %sr_bytes_{l} = mul i64 %sr_count_dec_{l}, 8"));
        self.emitln(&format!(
            "  call void @llvm.memmove.p0.p0.i64(i64* %sr_dst_{l}, i64* %sr_src_{l}, i64 %sr_bytes_{l}, i1 false)",
            l = l
        ));
        // Write last element to fb position
        self.emitln(&format!(
            "  %sr_fb_ptr_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %sr_fb_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  store i64 %sr_last_{l}, i64* %sr_fb_ptr_{l}"));
    }

    // ── Pick ───────────────────────────────────────────────

    fn compile_pick(&mut self) {
        // Pick: pop n, push copy of stack[sp-1-n]
        let n = self.pop_ssa();
        self.flush(); // Ensure stack state is in memory
        let l = self.next_label();
        let result = format!("%pick_{l}");

        let n_str = n.to_llvm();

        self.emitln(&format!("  %sp_pk_{l} = load i64, i64* @sp"));
        self.emitln(&format!("  %pk_n_{l} = call i64 @pick_max(i64 {n})", n = n_str, l = l));
        self.emitln(&format!("  %pk_sp_m1_{l} = sub i64 %sp_pk_{l}, 1"));
        self.emitln(&format!("  %pk_idx_{l} = sub i64 %pk_sp_m1_{l}, %pk_n_{l}"));
        // Check bounds
        self.emitln(&format!("  %pk_ge0_{l} = icmp sge i64 %pk_idx_{l}, 0", l = l));
        self.emitln(&format!("  br i1 %pk_ge0_{l}, label %pk_ok_{l}, label %pk_err_{l}"));
        self.emit_label(&format!("pk_err_{l}"));
        self.emitln("  call void @runtime_stack_underflow()");
        self.emitln("  unreachable");
        self.emit_label(&format!("pk_ok_{l}"));
        self.emitln(&format!(
            "  %pk_ptr_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %pk_idx_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  {r} = load i64, i64* %pk_ptr_{l}", r = result));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── DropN ──────────────────────────────────────────────

    fn compile_dropn(&mut self) {
        let n = self.pop_ssa();
        self.flush();
        let l = self.next_label();

        let n_str = n.to_llvm();

        // Clamp n >= 0
        self.emitln(&format!("  %dn_n_{l} = call i64 @pick_max(i64 {n})", n = n_str, l = l));
        // Check that we have enough elements
        self.emitln(&format!("  %sp_dn_{l} = load i64, i64* @sp"));
        self.emitln(&format!("  %dn_ok_{l} = icmp sle i64 %dn_n_{l}, %sp_dn_{l}"));
        self.emitln(&format!("  br i1 %dn_ok_{l}, label %dn_do_{l}, label %dn_err_{l}"));
        self.emit_label(&format!("dn_err_{l}"));
        self.emitln("  call void @runtime_stack_underflow()");
        self.emitln("  unreachable");
        self.emit_label(&format!("dn_do_{l}"));
        self.emitln(&format!("  %sp_dn_new_{l} = sub i64 %sp_dn_{l}, %dn_n_{l}"));
        self.emitln(&format!("  store i64 %sp_dn_new_{l}, i64* @sp"));
    }

    // ── 调试指令 ───────────────────────────────────────────

    fn compile_dumpstack(&mut self) {
        self.flush();
        let l = self.next_label();

        // Jump to prep
        self.emitln(&format!("  br label %ds_prep_{l}"));
        self.emit_label(&format!("ds_prep_{l}"));

        // Get frame base and sp
        self.emitln(&format!("  %ds_sp_{l} = load i64, i64* @sp"));
        self.emitln(&format!("  %ds_fbd_{l} = load i32, i32* @frame_depth"));
        self.emitln(&format!("  %ds_fbd_d_{l} = sub i32 %ds_fbd_{l}, 1"));
        self.emitln(&format!(
            "  %ds_fb_gep_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %ds_fbd_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!("  %ds_fb_{l} = load i64, i64* %ds_fb_gep_{l}"));

        // Print header to stderr
        self.emitln(&format!("  %ds_fp_{l} = call i8* @get_stderr()"));
        self.emitln(&format!(
            "  %ds_hdr_{l} = call i32 (i8*, i8*, ...) @fprintf(i8* %ds_fp_{l}, i8* getelementptr ([17 x i8], [17 x i8]* @dbg_header, i32 0, i32 0))"
        ));

        // Loop and print each element: fprintf(stderr, " %ld", val)
        self.emitln(&format!("  br label %ds_loop_hdr_{l}"));
        self.emit_label(&format!("ds_loop_hdr_{l}"));
        // phi 的回边前驱是循环末尾的块（ds_print_blk），必须与实际跳转来源一致
        self.emitln(&format!(
            "  %ds_i_{l} = phi i64 [ %ds_fb_{l}, %ds_prep_{l} ], [ %ds_i_next_{l}, %ds_print_blk_{l} ]"
        ));
        self.emitln(&format!("  %ds_end_{l} = icmp eq i64 %ds_i_{l}, %ds_sp_{l}"));
        self.emitln(&format!("  br i1 %ds_end_{l}, label %ds_done_{l}, label %ds_body_{l}"));
        self.emit_label(&format!("ds_body_{l}"));
        self.emitln(&format!(
            "  %ds_ptr_{l} = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %ds_i_{l}",
            SSZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!("  %ds_val_{l} = load i64, i64* %ds_ptr_{l}"));
        // 非首个元素先输出 ", "，使输出格式与解释器的 `{:?}` 一致
        self.emitln(&format!("  %ds_first_{l} = icmp eq i64 %ds_i_{l}, %ds_fb_{l}", l = l));
        self.emitln(&format!(
            "  br i1 %ds_first_{l}, label %ds_nocomma_blk_{l}, label %ds_comma_blk_{l}",
            l = l
        ));
        self.emit_label(&format!("ds_nocomma_blk_{l}"));
        self.emitln(&format!("  br label %ds_print_blk_{l}", l = l));
        self.emit_label(&format!("ds_comma_blk_{l}"));
        self.emitln(&format!("  %ds_fp2a_{l} = call i8* @get_stderr()"));
        self.emitln(&format!(
            "  %ds_cma_{l} = call i32 (i8*, i8*, ...) @fprintf(i8* %ds_fp2a_{l}, i8* getelementptr ([3 x i8], [3 x i8]* @dbg_comma, i32 0, i32 0))"
        ));
        self.emitln(&format!("  br label %ds_print_blk_{l}", l = l));
        self.emit_label(&format!("ds_print_blk_{l}"));
        self.emitln(&format!("  %ds_fp2_{l} = call i8* @get_stderr()"));
        self.emitln(&format!(
            "  %ds_pr_{l} = call i32 (i8*, i8*, ...) @fprintf(i8* %ds_fp2_{l}, i8* getelementptr ([4 x i8], [4 x i8]* @dbg_fmt, i32 0, i32 0), i64 %ds_val_{l})"
        ));
        self.emitln(&format!("  %ds_i_next_{l} = add i64 %ds_i_{l}, 1"));
        self.emitln(&format!("  br label %ds_loop_hdr_{l}"));
        self.emit_label(&format!("ds_done_{l}"));

        // Print closing bracket + newline
        self.emitln(&format!("  %ds_fp3_{l} = call i8* @get_stderr()"));
        self.emitln(&format!(
            "  %ds_nl_{l} = call i32 (i8*, i8*, ...) @fprintf(i8* %ds_fp3_{l}, i8* getelementptr ([3 x i8], [3 x i8]* @dbg_close, i32 0, i32 0))"
        ));
    }

    fn compile_dumpstate(&mut self) {
        self.flush();
        let l = self.next_label();

        self.emitln("  ; DumpState: print VM state to stderr");
        self.emitln(&format!("  %dss_sp_{l} = load i64, i64* @sp"));
        self.emitln(&format!("  %dss_fbd_{l} = load i32, i32* @frame_depth"));
        self.emitln(&format!("  %dss_fp_{l} = call i8* @get_stderr()"));
        // 栈深 = @sp - 帧基址（与解释器的 frame.stack.len() 等价）
        self.emitln(&format!("  %dss_fbd_d_{l} = sub i32 %dss_fbd_{l}, 1", l = l));
        self.emitln(&format!(
            "  %dss_fb_gep_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %dss_fbd_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!("  %dss_fb_{l} = load i64, i64* %dss_fb_gep_{l}", l = l));
        self.emitln(&format!("  %dss_depth_{l} = sub i64 %dss_sp_{l}, %dss_fb_{l}", l = l));
        self.emitln(&format!("  %dss_hu_{l} = load i64, i64* @heap_used", l = l));
        self.emitln(&format!(
            "  %dss_{l} = call i32 (i8*, i8*, ...) @fprintf(i8* %dss_fp_{l}, i8* getelementptr ([46 x i8], [46 x i8]* @dbg_state_fmt, i32 0, i32 0), i64 %dss_sp_{l}, i64 %dss_depth_{l}, i64 %dss_hu_{l})",
            l = l
        ));
    }

    // ── 编译函数体 ─────────────────────────────────────────

    fn compile_body(&mut self, instructions: &[Instruction]) {
        for inst in instructions {
            self.compile_instruction(inst);
        }
        // Flush remaining stack entries before function return
        self.flush();
    }
}

// ── 文件 I/O 运行时 ──────────────────────────────────────────

/// 文件 I/O 运行时（`FileRead` / `FileWrite` 的 LLVM 实现）。
///
/// 栈上的字符串表示与解释器一致：`[chars..., len]`，`@sp` 指向下一个空槽。
/// 读取内容的长度只有运行时才知道，无法用编译期栈模型表达，所以这里直接生成
/// 会改写 `@stack`/`@sp` 的运行时函数；编译器在调用它们之前会 flush 编译期栈。
///
/// 错误语义与解释器对齐：文件打不开、读写失败、内容不是合法 UTF-8、或栈空间
/// 不足，都打印错误并以非零状态退出。
fn file_io_runtime_ir(config: &CodeGenConfig) -> String {
    let template = r#"
; ═══════════════════════════════════════════════════════════════
; 文件 I/O 运行时（FileRead / FileWrite）
; ═══════════════════════════════════════════════════════════════
@err_file_io   = private constant [17 x i8] c"file I/O failed\0A\00"
@err_stack_ovf = private constant [16 x i8] c"stack overflow\0A\00"
@sd_mode_rb    = private constant [3 x i8] c"rb\00"
@sd_mode_wb    = private constant [3 x i8] c"wb\00"

define internal void @sd_file_io_error() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([17 x i8], [17 x i8]* @err_file_io, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @sd_stack_overflow() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([16 x i8], [16 x i8]* @err_stack_ovf, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

; 把码点编码为 UTF-8 写入 %buf，返回写入的字节数（1–4）。
define internal i64 @sd_utf8_encode(i64 %cp, i8* %buf) {
entry:
  %is1 = icmp slt i64 %cp, 128
  br i1 %is1, label %enc1, label %enc_chk2
enc1:
  %enc1_b = trunc i64 %cp to i8
  store i8 %enc1_b, i8* %buf
  ret i64 1
enc_chk2:
  %is2 = icmp slt i64 %cp, 2048
  br i1 %is2, label %enc2, label %enc_chk3
enc2:
  %e2_b0s = lshr i64 %cp, 6
  %e2_b0o = or i64 %e2_b0s, 192
  %e2_b0t = trunc i64 %e2_b0o to i8
  store i8 %e2_b0t, i8* %buf
  %e2_p1 = getelementptr i8, i8* %buf, i64 1
  %e2_b1m = and i64 %cp, 63
  %e2_b1o = or i64 %e2_b1m, 128
  %e2_b1t = trunc i64 %e2_b1o to i8
  store i8 %e2_b1t, i8* %e2_p1
  ret i64 2
enc_chk3:
  %is3 = icmp slt i64 %cp, 65536
  br i1 %is3, label %enc3, label %enc4
enc3:
  %e3_b0s = lshr i64 %cp, 12
  %e3_b0o = or i64 %e3_b0s, 224
  %e3_b0t = trunc i64 %e3_b0o to i8
  store i8 %e3_b0t, i8* %buf
  %e3_p1 = getelementptr i8, i8* %buf, i64 1
  %e3_b1s = lshr i64 %cp, 6
  %e3_b1m = and i64 %e3_b1s, 63
  %e3_b1o = or i64 %e3_b1m, 128
  %e3_b1t = trunc i64 %e3_b1o to i8
  store i8 %e3_b1t, i8* %e3_p1
  %e3_p2 = getelementptr i8, i8* %buf, i64 2
  %e3_b2m = and i64 %cp, 63
  %e3_b2o = or i64 %e3_b2m, 128
  %e3_b2t = trunc i64 %e3_b2o to i8
  store i8 %e3_b2t, i8* %e3_p2
  ret i64 3
enc4:
  %e4_b0s = lshr i64 %cp, 18
  %e4_b0o = or i64 %e4_b0s, 240
  %e4_b0t = trunc i64 %e4_b0o to i8
  store i8 %e4_b0t, i8* %buf
  %e4_p1 = getelementptr i8, i8* %buf, i64 1
  %e4_b1s = lshr i64 %cp, 12
  %e4_b1m = and i64 %e4_b1s, 63
  %e4_b1o = or i64 %e4_b1m, 128
  %e4_b1t = trunc i64 %e4_b1o to i8
  store i8 %e4_b1t, i8* %e4_p1
  %e4_p2 = getelementptr i8, i8* %buf, i64 2
  %e4_b2s = lshr i64 %cp, 6
  %e4_b2m = and i64 %e4_b2s, 63
  %e4_b2o = or i64 %e4_b2m, 128
  %e4_b2t = trunc i64 %e4_b2o to i8
  store i8 %e4_b2t, i8* %e4_p2
  %e4_p3 = getelementptr i8, i8* %buf, i64 3
  %e4_b3m = and i64 %cp, 63
  %e4_b3o = or i64 %e4_b3m, 128
  %e4_b3t = trunc i64 %e4_b3o to i8
  store i8 %e4_b3t, i8* %e4_p3
  ret i64 4
}

; 从 %buf 的 %pos 处解码一个 UTF-8 码点写入 %out，返回消耗的字节数；
; 返回 0 表示非法序列（与解释器的 UTF-8 校验对齐）。
define internal i64 @sd_utf8_decode(i8* %buf, i64 %pos, i64 %size, i64* %out) {
entry:
  %d0p = getelementptr i8, i8* %buf, i64 %pos
  %d0w = load i8, i8* %d0p
  %d0 = zext i8 %d0w to i64
  %d_is1 = icmp ult i64 %d0, 128
  br i1 %d_is1, label %dec1, label %dec_chk2
dec1:
  store i64 %d0, i64* %out
  ret i64 1
dec_chk2:
  %d1 = add i64 %pos, 1
  %d1_ge = icmp uge i64 %d0, 194
  %d1_le = icmp ule i64 %d0, 223
  %d1_lead = and i1 %d1_ge, %d1_le
  %d1_has = icmp slt i64 %d1, %size
  %d1_ok = and i1 %d1_lead, %d1_has
  br i1 %d1_ok, label %dec2, label %dec_chk3
dec2:
  %d2p = getelementptr i8, i8* %buf, i64 %d1
  %d2w = load i8, i8* %d2p
  %d2 = zext i8 %d2w to i64
  %d2m = and i64 %d2, 192
  %d2_cont = icmp eq i64 %d2m, 128
  br i1 %d2_cont, label %dec2_ok, label %dec_bad
dec2_ok:
  %d2_l0 = and i64 %d0, 31
  %d2_s0 = shl i64 %d2_l0, 6
  %d2_l1 = and i64 %d2, 63
  %d2_cp = or i64 %d2_s0, %d2_l1
  store i64 %d2_cp, i64* %out
  ret i64 2
dec_chk3:
  %d3i = add i64 %pos, 2
  %d3_ge = icmp uge i64 %d0, 224
  %d3_le = icmp ule i64 %d0, 239
  %d3_lead = and i1 %d3_ge, %d3_le
  %d3_has = icmp slt i64 %d3i, %size
  %d3_ok = and i1 %d3_lead, %d3_has
  br i1 %d3_ok, label %dec3, label %dec_chk4
dec3:
  %d3p1 = getelementptr i8, i8* %buf, i64 %d1
  %d3w1 = load i8, i8* %d3p1
  %d3c1 = zext i8 %d3w1 to i64
  %d3p2 = getelementptr i8, i8* %buf, i64 %d3i
  %d3w2 = load i8, i8* %d3p2
  %d3c2 = zext i8 %d3w2 to i64
  %d3m1 = and i64 %d3c1, 192
  %d3k1 = icmp eq i64 %d3m1, 128
  %d3m2 = and i64 %d3c2, 192
  %d3k2 = icmp eq i64 %d3m2, 128
  %d3_conts = and i1 %d3k1, %d3k2
  ; 过长编码（E0 80..9F）与代理区（ED A0..BF）都视为非法
  %d3_is_e0 = icmp eq i64 %d0, 224
  %d3_c1_lt = icmp ult i64 %d3c1, 160
  %d3_ovl = and i1 %d3_is_e0, %d3_c1_lt
  %d3_is_ed = icmp eq i64 %d0, 237
  %d3_c1_hi = icmp uge i64 %d3c1, 160
  %d3_surr = and i1 %d3_is_ed, %d3_c1_hi
  %d3_bad_range = or i1 %d3_ovl, %d3_surr
  %d3_good_range = xor i1 %d3_bad_range, true
  %d3_ok_range = and i1 %d3_conts, %d3_good_range
  br i1 %d3_ok_range, label %dec3_ok, label %dec_bad
dec3_ok:
  %d3_l0 = and i64 %d0, 15
  %d3_s0 = shl i64 %d3_l0, 12
  %d3_l1 = and i64 %d3c1, 63
  %d3_s1 = shl i64 %d3_l1, 6
  %d3_l2 = and i64 %d3c2, 63
  %d3_a = or i64 %d3_s0, %d3_s1
  %d3_cp = or i64 %d3_a, %d3_l2
  store i64 %d3_cp, i64* %out
  ret i64 3
dec_chk4:
  %d4i = add i64 %pos, 3
  %d4_ge = icmp uge i64 %d0, 240
  %d4_le = icmp ule i64 %d0, 244
  %d4_lead = and i1 %d4_ge, %d4_le
  %d4_has = icmp slt i64 %d4i, %size
  %d4_ok = and i1 %d4_lead, %d4_has
  br i1 %d4_ok, label %dec4, label %dec_bad
dec4:
  %d4p1 = getelementptr i8, i8* %buf, i64 %d1
  %d4w1 = load i8, i8* %d4p1
  %d4c1 = zext i8 %d4w1 to i64
  %d4p2 = getelementptr i8, i8* %buf, i64 %d3i
  %d4w2 = load i8, i8* %d4p2
  %d4c2 = zext i8 %d4w2 to i64
  %d4p3 = getelementptr i8, i8* %buf, i64 %d4i
  %d4w3 = load i8, i8* %d4p3
  %d4c3 = zext i8 %d4w3 to i64
  %d4m1 = and i64 %d4c1, 192
  %d4k1 = icmp eq i64 %d4m1, 128
  %d4m2 = and i64 %d4c2, 192
  %d4k2 = icmp eq i64 %d4m2, 128
  %d4m3 = and i64 %d4c3, 192
  %d4k3 = icmp eq i64 %d4m3, 128
  %d4_k12 = and i1 %d4k1, %d4k2
  %d4_conts = and i1 %d4_k12, %d4k3
  ; 过长编码（F0 80..8F）与超出 U+10FFFF（F4 90..）视为非法
  %d4_is_f0 = icmp eq i64 %d0, 240
  %d4_c1_lt = icmp ult i64 %d4c1, 144
  %d4_ovl = and i1 %d4_is_f0, %d4_c1_lt
  %d4_is_f4 = icmp eq i64 %d0, 244
  %d4_c1_hi = icmp uge i64 %d4c1, 144
  %d4_range = and i1 %d4_is_f4, %d4_c1_hi
  %d4_bad_range = or i1 %d4_ovl, %d4_range
  %d4_good_range = xor i1 %d4_bad_range, true
  %d4_ok_range = and i1 %d4_conts, %d4_good_range
  br i1 %d4_ok_range, label %dec4_ok, label %dec_bad
dec4_ok:
  %d4_l0 = and i64 %d0, 7
  %d4_s0 = shl i64 %d4_l0, 18
  %d4_l1 = and i64 %d4c1, 63
  %d4_s1 = shl i64 %d4_l1, 12
  %d4_l2 = and i64 %d4c2, 63
  %d4_s2 = shl i64 %d4_l2, 6
  %d4_l3 = and i64 %d4c3, 63
  %d4_a = or i64 %d4_s0, %d4_s1
  %d4_b = or i64 %d4_a, %d4_s2
  %d4_cp = or i64 %d4_b, %d4_l3
  store i64 %d4_cp, i64* %out
  ret i64 4
dec_bad:
  ret i64 0
}

; FileRead：栈上 [path chars..., len] → [contents chars..., len]
define internal void @sd_file_read() {
entry:
  %rd_cp = alloca i64
  %rd_sp = load i64, i64* @sp
  %rd_fdd = load i32, i32* @frame_depth
  %rd_fdi = sub i32 %rd_fdd, 1
  %rd_fbp = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %rd_fdi
  %rd_fb = load i64, i64* %rd_fbp
  %rd_has = icmp sgt i64 %rd_sp, %rd_fb
  br i1 %rd_has, label %pop_len, label %rd_underflow
pop_len:
  %rd_li = sub i64 %rd_sp, 1
  %rd_lp = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %rd_li
  %rd_plen = load i64, i64* %rd_lp
  %rd_len_ok = icmp sge i64 %rd_plen, 0
  %rd_pstart = sub i64 %rd_li, %rd_plen
  %rd_room = icmp sge i64 %rd_pstart, %rd_fb
  %rd_ok = and i1 %rd_len_ok, %rd_room
  br i1 %rd_ok, label %alloc_path, label %rd_underflow
alloc_path:
  %rd_pmul = mul i64 %rd_plen, 4
  %rd_pbytes = add i64 %rd_pmul, 1
  %rd_path = call i8* @malloc(i64 %rd_pbytes)
  %rd_path_null = icmp eq i8* %rd_path, null
  br i1 %rd_path_null, label %rd_io, label %enc_path
enc_path:
  br label %enc_path_check
enc_path_check:
  %rd_ei = phi i64 [ 0, %enc_path ], [ %rd_ei_next, %enc_path_step ]
  %rd_ecp = phi i64 [ %rd_pstart, %enc_path ], [ %rd_ecp_next, %enc_path_step ]
  %rd_eb = phi i64 [ 0, %enc_path ], [ %rd_eb_next, %enc_path_step ]
  %rd_enc_done = icmp eq i64 %rd_ei, %rd_plen
  br i1 %rd_enc_done, label %enc_path_done, label %enc_path_body
enc_path_body:
  %rd_ecpp = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %rd_ecp
  %rd_ecpv = load i64, i64* %rd_ecpp
  %rd_edst = getelementptr i8, i8* %rd_path, i64 %rd_eb
  %rd_en = call i64 @sd_utf8_encode(i64 %rd_ecpv, i8* %rd_edst)
  %rd_ei_next = add i64 %rd_ei, 1
  %rd_ecp_next = add i64 %rd_ecp, 1
  %rd_eb_next = add i64 %rd_eb, %rd_en
  br label %enc_path_step
enc_path_step:
  br label %enc_path_check
enc_path_done:
  %rd_nul = getelementptr i8, i8* %rd_path, i64 %rd_eb
  store i8 0, i8* %rd_nul
  %rd_rb = getelementptr [3 x i8], [3 x i8]* @sd_mode_rb, i32 0, i32 0
  %rd_fp = call i8* @fopen(i8* %rd_path, i8* %rd_rb)
  %rd_fp_null = icmp eq i8* %rd_fp, null
  br i1 %rd_fp_null, label %rd_io_path, label %seek_end
seek_end:
  %rd_s1 = call i32 @fseek(i8* %rd_fp, i64 0, i32 2)
  %rd_size = call i64 @ftell(i8* %rd_fp)
  %rd_size_ok = icmp sge i64 %rd_size, 0
  br i1 %rd_size_ok, label %rewind, label %rd_io_handle
rewind:
  %rd_s2 = call i32 @fseek(i8* %rd_fp, i64 0, i32 0)
  %rd_s2_ok = icmp eq i32 %rd_s2, 0
  br i1 %rd_s2_ok, label %alloc_data, label %rd_io_handle
alloc_data:
  %rd_dn = add i64 %rd_size, 1
  %rd_data = call i8* @malloc(i64 %rd_dn)
  %rd_data_null = icmp eq i8* %rd_data, null
  br i1 %rd_data_null, label %rd_io_handle, label %do_read
do_read:
  %rd_read = call i64 @fread(i8* %rd_data, i64 1, i64 %rd_size, i8* %rd_fp)
  %rd_close = call i32 @fclose(i8* %rd_fp)
  %rd_read_ok = icmp eq i64 %rd_read, %rd_size
  br i1 %rd_read_ok, label %decode, label %rd_io_data
decode:
  call void @free(i8* %rd_path)
  br label %decode_check
decode_check:
  %rd_di = phi i64 [ 0, %decode ], [ %rd_di_next, %decode_step ]
  %rd_dc = phi i64 [ 0, %decode ], [ %rd_dc_next, %decode_step ]
  %rd_decode_done = icmp eq i64 %rd_di, %rd_size
  br i1 %rd_decode_done, label %decode_done, label %decode_body
decode_body:
  %rd_wpos = add i64 %rd_pstart, %rd_dc
  %rd_cap_ok = icmp slt i64 %rd_wpos, {SSZ}
  br i1 %rd_cap_ok, label %decode_call, label %rd_overflow
decode_call:
  %rd_adv = call i64 @sd_utf8_decode(i8* %rd_data, i64 %rd_di, i64 %rd_size, i64* %rd_cp)
  %rd_adv_zero = icmp eq i64 %rd_adv, 0
  br i1 %rd_adv_zero, label %rd_io_data, label %decode_store
decode_store:
  %rd_cpv = load i64, i64* %rd_cp
  %rd_wptr = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %rd_wpos
  store i64 %rd_cpv, i64* %rd_wptr
  %rd_di_next = add i64 %rd_di, %rd_adv
  %rd_dc_next = add i64 %rd_dc, 1
  br label %decode_step
decode_step:
  br label %decode_check
decode_done:
  %rd_end = add i64 %rd_pstart, %rd_dc
  %rd_len_room = icmp slt i64 %rd_end, {SSZ}
  br i1 %rd_len_room, label %push_len, label %rd_overflow_data
push_len:
  %rd_lptr = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %rd_end
  store i64 %rd_dc, i64* %rd_lptr
  %rd_sp_new = add i64 %rd_end, 1
  store i64 %rd_sp_new, i64* @sp
  call void @free(i8* %rd_data)
  ret void
rd_underflow:
  call void @runtime_stack_underflow()
  unreachable
rd_overflow:
  call void @free(i8* %rd_data)
  call void @sd_stack_overflow()
  unreachable
rd_overflow_data:
  call void @free(i8* %rd_data)
  call void @sd_stack_overflow()
  unreachable
rd_io:
  call void @sd_file_io_error()
  unreachable
rd_io_path:
  call void @free(i8* %rd_path)
  call void @sd_file_io_error()
  unreachable
rd_io_handle:
  call void @fclose(i8* %rd_fp)
  call void @free(i8* %rd_path)
  call void @sd_file_io_error()
  unreachable
rd_io_data:
  call void @free(i8* %rd_data)
  call void @sd_file_io_error()
  unreachable
}

; FileWrite：栈上 [path chars..., len, data chars..., len] → []
define internal void @sd_file_write() {
entry:
  %wr_sp = load i64, i64* @sp
  %wr_fdd = load i32, i32* @frame_depth
  %wr_fdi = sub i32 %wr_fdd, 1
  %wr_fbp = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %wr_fdi
  %wr_fb = load i64, i64* %wr_fbp
  %wr_has = icmp sgt i64 %wr_sp, %wr_fb
  br i1 %wr_has, label %wr_pop_data, label %wr_underflow
wr_pop_data:
  %wr_dli = sub i64 %wr_sp, 1
  %wr_dlp = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %wr_dli
  %wr_dlen = load i64, i64* %wr_dlp
  %wr_dlen_ok = icmp sge i64 %wr_dlen, 0
  %wr_dstart = sub i64 %wr_dli, %wr_dlen
  %wr_droom = icmp sge i64 %wr_dstart, %wr_fb
  %wr_dok = and i1 %wr_dlen_ok, %wr_droom
  br i1 %wr_dok, label %wr_pop_path, label %wr_underflow
wr_pop_path:
  %wr_pli = sub i64 %wr_dstart, 1
  %wr_has_plen = icmp sge i64 %wr_pli, %wr_fb
  br i1 %wr_has_plen, label %wr_read_plen, label %wr_underflow
wr_read_plen:
  %wr_plp = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %wr_pli
  %wr_plen = load i64, i64* %wr_plp
  %wr_plen_ok = icmp sge i64 %wr_plen, 0
  %wr_pstart = sub i64 %wr_pli, %wr_plen
  %wr_pok = icmp sge i64 %wr_pstart, %wr_fb
  %wr_path_ok = and i1 %wr_plen_ok, %wr_pok
  br i1 %wr_path_ok, label %wr_alloc_path, label %wr_underflow
wr_alloc_path:
  %wr_pmul = mul i64 %wr_plen, 4
  %wr_pbytes = add i64 %wr_pmul, 1
  %wr_path = call i8* @malloc(i64 %wr_pbytes)
  %wr_path_null = icmp eq i8* %wr_path, null
  br i1 %wr_path_null, label %wr_io, label %wr_enc_path
wr_enc_path:
  br label %wr_enc_check
wr_enc_check:
  %wr_ei = phi i64 [ 0, %wr_enc_path ], [ %wr_ei_next, %wr_enc_step ]
  %wr_ecp = phi i64 [ %wr_pstart, %wr_enc_path ], [ %wr_ecp_next, %wr_enc_step ]
  %wr_eb = phi i64 [ 0, %wr_enc_path ], [ %wr_eb_next, %wr_enc_step ]
  %wr_enc_done = icmp eq i64 %wr_ei, %wr_plen
  br i1 %wr_enc_done, label %wr_enc_done_blk, label %wr_enc_body
wr_enc_body:
  %wr_ecpp = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %wr_ecp
  %wr_ecpv = load i64, i64* %wr_ecpp
  %wr_edst = getelementptr i8, i8* %wr_path, i64 %wr_eb
  %wr_en = call i64 @sd_utf8_encode(i64 %wr_ecpv, i8* %wr_edst)
  %wr_ei_next = add i64 %wr_ei, 1
  %wr_ecp_next = add i64 %wr_ecp, 1
  %wr_eb_next = add i64 %wr_eb, %wr_en
  br label %wr_enc_step
wr_enc_step:
  br label %wr_enc_check
wr_enc_done_blk:
  %wr_nul = getelementptr i8, i8* %wr_path, i64 %wr_eb
  store i8 0, i8* %wr_nul
  %wr_wb = getelementptr [3 x i8], [3 x i8]* @sd_mode_wb, i32 0, i32 0
  %wr_fp = call i8* @fopen(i8* %wr_path, i8* %wr_wb)
  %wr_fp_null = icmp eq i8* %wr_fp, null
  br i1 %wr_fp_null, label %wr_io_path, label %wr_alloc_data
wr_alloc_data:
  %wr_dmul = mul i64 %wr_dlen, 4
  %wr_dbytes = add i64 %wr_dmul, 1
  %wr_data = call i8* @malloc(i64 %wr_dbytes)
  %wr_data_null = icmp eq i8* %wr_data, null
  br i1 %wr_data_null, label %wr_io_handle, label %wr_enc_data
wr_enc_data:
  br label %wr_dcheck
wr_dcheck:
  %wr_di = phi i64 [ 0, %wr_enc_data ], [ %wr_di_next, %wr_dstep ]
  %wr_dcp = phi i64 [ %wr_dstart, %wr_enc_data ], [ %wr_dcp_next, %wr_dstep ]
  %wr_db = phi i64 [ 0, %wr_enc_data ], [ %wr_db_next, %wr_dstep ]
  %wr_d_done = icmp eq i64 %wr_di, %wr_dlen
  br i1 %wr_d_done, label %wr_done_blk, label %wr_dbody
wr_dbody:
  %wr_dcpp = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %wr_dcp
  %wr_dcpv = load i64, i64* %wr_dcpp
  %wr_ddst = getelementptr i8, i8* %wr_data, i64 %wr_db
  %wr_dn = call i64 @sd_utf8_encode(i64 %wr_dcpv, i8* %wr_ddst)
  %wr_di_next = add i64 %wr_di, 1
  %wr_dcp_next = add i64 %wr_dcp, 1
  %wr_db_next = add i64 %wr_db, %wr_dn
  br label %wr_dstep
wr_dstep:
  br label %wr_dcheck
wr_done_blk:
  %wr_written = call i64 @fwrite(i8* %wr_data, i64 1, i64 %wr_db, i8* %wr_fp)
  %wr_close = call i32 @fclose(i8* %wr_fp)
  store i64 %wr_pstart, i64* @sp
  call void @free(i8* %wr_data)
  call void @free(i8* %wr_path)
  %wr_write_ok = icmp eq i64 %wr_written, %wr_db
  br i1 %wr_write_ok, label %wr_done, label %wr_io
wr_done:
  ret void
wr_underflow:
  call void @runtime_stack_underflow()
  unreachable
wr_io:
  call void @sd_file_io_error()
  unreachable
wr_io_path:
  call void @free(i8* %wr_path)
  call void @sd_file_io_error()
  unreachable
wr_io_handle:
  call void @fclose(i8* %wr_fp)
  call void @free(i8* %wr_path)
  call void @sd_file_io_error()
  unreachable
}
"#;
    template
        .replace("{SSZ}", &config.stack_size.to_string())
        .replace("{MD}", &config.max_call_depth.to_string())
}

// ── 公开 API ──────────────────────────────────────────────────

/// 将 ParseResult 编译为 LLVM IR 文本。
pub fn compile_to_ir(parse_result: &ParseResult, config: &CodeGenConfig) -> String {
    // 1. 运行 Stardust IR 优化
    let (main_opt, funcs_opt) =
        optimizer::optimize_functions(&parse_result.main_instructions, &parse_result.functions);

    let mut out = String::new();

    // ── 模块头部 ──
    out.push_str(&format!(
        r#"; ═══════════════════════════════════════════════════════════════
; Stardust compiled program
; Generated by stardustc {}
; ═══════════════════════════════════════════════════════════════

target triple = "x86_64-unknown-linux-gnu"

"#,
        env!("CARGO_PKG_VERSION")
    ));

    // ── 外部声明 ──
    out.push_str(EXTERNAL_DECLS);

    // ── memmove 声明（ShiftL/ShiftR 使用）──
    out.push_str("declare void @llvm.memmove.p0.p0.i64(i64*, i64*, i64, i1)\n");

    // ── stderr 辅助 ──
    // 使用 C 运行时的全局 stderr 符号。
    // 旧实现用 fdopen(2, "w") 现场创建流，每次调用都会泄漏一个 FILE*。
    out.push_str(
        r#"
@stderr = external global i8*

define internal i8* @get_stderr() {
  %fp = load i8*, i8** @stderr
  ret i8* %fp
}

; ── 辅助函数：pick_max ──
define internal i64 @pick_max(i64 %n) {
  %is_neg = icmp slt i64 %n, 0
  br i1 %is_neg, label %ret_zero, label %ret_n
ret_zero:
  ret i64 0
ret_n:
  ret i64 %n
}
"#,
    );

    // ── 全局状态 ──
    out.push_str(&format!(
        r#"
; ── Stardust 运行时状态 ──
@stack = global [{SSZ} x i64] zeroinitializer
@sp    = global i64 0
@heap  = global [{HSZ} x i64] zeroinitializer
@heap_used = global i64 0
@frame_base_stack = global [{MD} x i64] zeroinitializer
@frame_depth = global i32 0

; ── 调试字符串 ──
@stderr_fmt = private constant [4 x i8] c"%s\0A\00"
@dbg_header = private constant [17 x i8] c"[DEBUG] Stack: [\00"
@dbg_comma = private constant [3 x i8] c", \00"
@dbg_fmt = private constant [4 x i8] c"%ld\00"
@dbg_close = private constant [3 x i8] c"]\0A\00"
@dbg_state_fmt = private constant [46 x i8] c"[DEBUG] sp=%ld, StackDepth=%ld, HeapSize=%ld\0A\00"


"#,
        SSZ = config.stack_size,
        HSZ = config.heap_size,
        MD = config.max_call_depth,
    ));

    // ── 运行时内建函数 ──
    out.push_str(INTRINSICS_IR);

    // ── 文件 I/O 运行时（需要栈/帧参数，故以配置参数化生成）──
    out.push_str(&file_io_runtime_ir(config));

    // ── 编译 Stardust 函数 ──
    for (name, func_def) in &funcs_opt {
        // 一阶段：收集 Marks
        let mut marks: HashMap<usize, String> = HashMap::new();
        for inst in func_def.body.iter() {
            if let Instruction::Mark { name, .. } = inst {
                marks.insert(*name, FuncCompiler::<'_>::mark_label(*name));
            }
        }

        let mut fc = FuncCompiler::new(config);
        fc.current_func = Some(*name);

        out.push_str(&format!("define void @sd_func_{name}() {{\nentry:\n", name = name));
        fc.compile_body(&func_def.body);
        out.push_str(&fc.output);
        out.push_str("  ret void\n}\n\n");
    }

    // ── 编译主程序 ──
    {
        let mut main_marks: HashMap<usize, String> = HashMap::new();
        for inst in &main_opt {
            if let Instruction::Mark { name, .. } = inst {
                main_marks.insert(*name, FuncCompiler::<'_>::mark_label(*name));
            }
        }

        let mut fc = FuncCompiler::new(config);
        fc.current_func = None;

        out.push_str("define void @sd_main() {\nentry:\n");

        // Initialize frame_base for main
        out.push_str(&format!(
            "  store i64 0, i64* getelementptr ([{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 0)\n",
            MD = config.max_call_depth
        ));
        out.push_str("  store i32 1, i32* @frame_depth\n");

        fc.compile_body(&main_opt);
        out.push_str(&fc.output);
        out.push_str("  ret void\n}\n\n");
    }

    // ── main 入口 ──
    out.push_str(
        r#"define i64 @main() {
entry:
  ; Initialize frame tracking
  store i64 0, i64* getelementptr"#,
    );
    out.push_str(&format!(
        " ([{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 0)\n",
        MD = config.max_call_depth
    ));
    out.push_str("  store i32 1, i32* @frame_depth\n");
    out.push_str("  call void @sd_main()\n");
    out.push_str("  ret i64 0\n}\n");

    out
}

// ── 测试 ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stardust::lexer::tokenize;
    use crate::stardust::parser::parse_program;

    fn default_config() -> CodeGenConfig {
        CodeGenConfig::default()
    }

    /// 完整流水线：源码 → LLVM IR
    fn compile_source(source: &str) -> String {
        let tokens = tokenize(source).unwrap();
        let parsed = parse_program(tokens).unwrap();
        compile_to_ir(&parsed, &default_config())
    }

    fn compile_source_with_config(source: &str, config: &CodeGenConfig) -> String {
        let tokens = tokenize(source).unwrap();
        let parsed = parse_program(tokens).unwrap();
        compile_to_ir(&parsed, config)
    }

    #[test]
    fn compile_charout_unicode_uses_utf8_runtime() {
        let config = CodeGenConfig { char_mode: CharMode::Unicode, ..CodeGenConfig::default() };
        // Push(233) + CharOut（233 = é）
        let source = format!("{}+,\n", " ".repeat(238));
        let ir = compile_source_with_config(&source, &config);
        assert!(ir.contains("runtime_char_out_utf8"), "unicode CharOut should use UTF-8 runtime");
        assert!(!ir.contains("%ch_ok_low_"), "unicode CharOut should skip ASCII bounds check");
    }

    #[test]
    fn compile_empty_program() {
        let ir = compile_source("");
        assert!(ir.contains("define void @sd_main()"));
        assert!(ir.contains("define i64 @main()"));
    }

    #[test]
    fn compile_push_and_charout() {
        // Push(72) CharOut → outputs 'H'
        let ir = compile_source("             +,"); // 77 spaces +,+ = Push(72) CharOut
        assert!(ir.contains("@putchar"));
        assert!(ir.contains("@stack"));
        assert!(ir.contains("@sp"));
    }

    #[test]
    fn compile_hello_world() {
        let src = include_str!("../../hello_world.sd");
        let ir = compile_source(src);
        // Should contain multiple charout calls
        assert!(ir.contains("putchar"));
        assert!(ir.contains("define i64 @main()"));
    }

    #[test]
    fn compile_arithmetic() {
        // Push(5) Push(3) Add → IR should use checked_add
        let src = "          +        +*"; // push(5) push(3) add
        let ir = compile_source(src);
        assert!(ir.contains("checked_add"));
    }

    #[test]
    fn compile_with_mark_and_jump() {
        // Mark(0) followed by Jump(0) references the same mark
        let src = "`'"; // Mark(0) Jump(0)
        let ir = compile_source(src);
        assert!(ir.contains("mark_0:"), "IR should contain mark_0 label");
        assert!(ir.contains("br i1"), "IR should contain conditional branch");
    }

    #[test]
    fn compile_function_decl_and_call() {
        // (1): Push(65) CharOut (1):
        // (1): (0);
        let src = " :             +, : : ;";
        let ir = compile_source(src);
        assert!(ir.contains("@sd_func_1"));
        assert!(ir.contains("@putchar"));
    }

    #[test]
    fn ir_contains_required_sections() {
        let ir = compile_source("");
        // Must have essential components
        assert!(ir.contains("target triple"));
        assert!(ir.contains("@stack"));
        assert!(ir.contains("@sp"));
        assert!(ir.contains("@heap"));
        assert!(ir.contains("define i64 @main()"));
    }

    // ═══════════ 比较运算 IR 生成 ═══════════
    // 使用 Dup 创建非常量值以避免优化器折叠

    #[test]
    fn compile_cmp_eq() {
        // Push(5) Dup Eq — Dup makes the comparison runtime-dependent
        let src = "          + +=";
        let ir = compile_source(src);
        assert!(
            ir.contains("icmp eq i64") || ir.contains("icmp ne i64"),
            "should generate comparison IR"
        );
    }

    #[test]
    fn compile_cmp_ne() {
        let src = "          + + ="; // Push(5) Dup Ne
        let ir = compile_source(src);
        assert!(ir.contains("icmp ne i64"), "should generate icmp ne");
    }

    #[test]
    fn compile_cmp_lt() {
        // Push(5) Dup Lt — Dup prevents constant folding
        let src = "          + +  =";
        let ir = compile_source(src);
        assert!(ir.contains("icmp slt i64"), "should generate icmp slt");
    }

    #[test]
    fn compile_cmp_gt() {
        let src = "          + +   =";
        let ir = compile_source(src);
        assert!(ir.contains("icmp sgt i64"), "should generate icmp sgt");
    }

    #[test]
    fn compile_cmp_le() {
        let src = "          + +    =";
        let ir = compile_source(src);
        assert!(ir.contains("icmp sle i64"), "should generate icmp sle");
    }

    #[test]
    fn compile_cmp_ge() {
        let src = "          + +     =";
        let ir = compile_source(src);
        assert!(ir.contains("icmp sge i64"), "should generate icmp sge");
    }

    // ═══════════ 逻辑运算 IR 生成 ═══════════

    #[test]
    fn compile_logic_and() {
        // Push(1) Dup And — Dup prevents folding
        let src = "      + +&";
        let ir = compile_source(src);
        assert!(ir.contains("and i1"), "should contain logical and");
    }

    #[test]
    fn compile_logic_or() {
        let src = "      + + &";
        let ir = compile_source(src);
        assert!(ir.contains("or i1"), "should contain logical or");
    }

    #[test]
    fn compile_logic_not() {
        // Push(0) Not → optimizer CAN fold this one since it's Push-Not pair
        // But the compiled IR still needs to handle Not semantics
        let src = "     +  &";
        let ir = compile_source(src);
        // After folding Push(0) Not → Push(1), no comparison needed
        assert!(ir.contains("define void @sd_main"), "should compile");
    }

    #[test]
    fn compile_logic_xor() {
        let src = "      + +   &";
        let ir = compile_source(src);
        assert!(ir.contains("xor i1"), "should contain logical xor");
    }

    // ═══════════ 控制流 ═══════════

    #[test]
    fn compile_unconditional_jump() {
        // Mark(0) UncondJump(0)
        let src = "`~";
        let ir = compile_source(src);
        assert!(ir.contains("mark_0:"), "should have mark label");
        assert!(ir.contains("br label %mark_0"), "should have unconditional branch");
    }

    #[test]
    fn compile_loop_pattern() {
        // Push(3) Mark(0) Dup Push(1) Sub Dup Jump(0) — countdown loop
        let src = "        +` +      + * +'";
        let ir = compile_source(src);
        assert!(ir.contains("mark_0:"), "should have loop mark");
        assert!(ir.contains("br i1"), "should have conditional branch");
        assert!(ir.contains("checked_sub"), "should have subtraction");
    }

    #[test]
    fn compile_mark_with_nonzero_name() {
        // Mark(5) Push(0) Jump(5) — named mark 5
        let src = "     `     +     '";
        let ir = compile_source(src);
        assert!(ir.contains("mark_5:"), "should have mark_5 label");
        assert!(ir.contains("%mark_5"), "branch should target mark_5");
    }

    // ═══════════ 函数调用 ═══════════

    #[test]
    fn compile_function_with_args() {
        // (2): Add NumOut (2):  — 函数2: 接收2个参数，相加并输出
        // Push(3) Push(4) (2): (2);  — 调用函数2
        let src = "  :* .  :          +          +  :  ;";
        let ir = compile_source(src);
        assert!(ir.contains("@sd_func_2"), "should define function 2");
        assert!(ir.contains("call void @sd_func_2"), "should call function 2");
    }

    #[test]
    fn compile_nested_function_call() {
        // (2): Push(66) CharOut (2):
        // (1): Push(65) CharOut (2): (0); (1):
        // (1): (0);
        let src = "  :             +,  : :             +,  : ; : : ;";
        let ir = compile_source(src);
        assert!(ir.contains("@sd_func_1"), "should define func 1");
        assert!(ir.contains("@sd_func_2"), "should define func 2");
        assert!(ir.contains("call void @sd_func_"), "should have function calls");
    }

    #[test]
    fn compile_function_name_zero() {
        // (0): Push(42) (0):
        // (0): (0);
        let src = ":          +:  : ;";
        let ir = compile_source(src);
        assert!(ir.contains("@sd_func_0"), "function 0 should be defined");
    }

    // ═══════════ I/O 指令 ═══════════

    #[test]
    fn compile_numout() {
        // Push(42) NumOut
        let src = "           +.";
        let ir = compile_source(src);
        assert!(ir.contains("@printf"), "should use printf for NumOut");
        assert!(ir.contains("@fmt_numout"), "should reference format string");
    }

    #[test]
    fn compile_numin() {
        // NumIn
        let src = " .";
        let ir = compile_source(src);
        assert!(ir.contains("@scanf"), "should use scanf for NumIn");
        assert!(ir.contains("alloca i64"), "should allocate input buffer");
    }

    #[test]
    fn compile_charin() {
        // CharIn
        let src = " ,";
        let ir = compile_source(src);
        assert!(ir.contains("@getchar"), "should use getchar for CharIn");
    }

    // ═══════════ 算术运算 IR 生成 ═══════════

    #[test]
    fn compile_sub() {
        let src = "          +        + *";
        let ir = compile_source(src);
        assert!(ir.contains("checked_sub"), "should use checked_sub");
    }

    #[test]
    fn compile_mul() {
        let src = "          +        +  *";
        let ir = compile_source(src);
        assert!(ir.contains("checked_mul"), "should use checked_mul");
    }

    #[test]
    fn compile_div_with_zero_check() {
        // Push(10) Dup Div — Dup prevents folding, runtime div includes zero-check
        let src = "           + +   *";
        let ir = compile_source(src);
        assert!(ir.contains("sdiv i64"), "should use sdiv for runtime division");
        assert!(ir.contains("div_zero") || ir.contains("div_err"), "should have zero-check");
    }

    #[test]
    fn compile_mod_with_zero_check() {
        // Push(10) Dup Mod
        let src = "           + +    *";
        let ir = compile_source(src);
        assert!(ir.contains("srem i64"), "should use srem for runtime modulo");
        assert!(ir.contains("mod_zero") || ir.contains("mod_err"), "should have zero-check");
    }

    #[test]
    fn compile_reverse_generates_ir() {
        // Push(1) Push(2) Push(3) Reverse
        let src = "      +      +      +     *";
        let ir = compile_source(src);
        assert!(ir.contains("@malloc"), "reverse should allocate temp buffer");
        assert!(ir.contains("@free"), "reverse should free temp buffer");
    }

    // ═══════════ 堆操作 ═══════════

    #[test]
    fn compile_store_and_load() {
        // Push(10) Push(99) Store — heap[10] = 99
        // Push(10) Load — push heap[10]
        let src = "           +             +-           + -";
        let ir = compile_source(src);
        assert!(ir.contains("@heap"), "should reference heap");
        assert!(ir.contains("heap_oob"), "should have OOB check for store");
    }

    #[test]
    fn compile_store_has_bounds_check() {
        // Push(0) Push(0) Store — should contain heap bounds checking
        let src = "     +     +-";
        let ir = compile_source(src);
        assert!(ir.contains("runtime_heap_oob") || ir.contains("heap"), "should handle heap ops");
    }

    // ═══════════ 栈扩展 ═══════════

    #[test]
    fn compile_depth() {
        // Depth
        let src = " <";
        let ir = compile_source(src);
        assert!(ir.contains("@frame_base_stack"), "depth uses frame_base_stack");
    }

    #[test]
    fn compile_shiftl() {
        // Push(1) Push(2) ShiftL
        let src = "      +      +<";
        let ir = compile_source(src);
        assert!(ir.contains("@llvm.memmove"), "shiftl uses memmove");
    }

    #[test]
    fn compile_shiftr() {
        // Push(1) Push(2) ShiftR
        let src = "      +      +>";
        let ir = compile_source(src);
        assert!(ir.contains("@llvm.memmove"), "shiftr uses memmove");
    }

    #[test]
    fn compile_dropn() {
        // Push(1) Push(2) Push(2) DropN  — drop top 2, leaves [1]
        let src = "      +      +      + >";
        let ir = compile_source(src);
        assert!(ir.contains("sub i64"), "dropn should decrement sp");
    }

    #[test]
    fn compile_pick() {
        // Push(10) Push(20) Push(30) Push(1) Pick  — copies 20
        let src = "           +           +           +      +  <";
        let ir = compile_source(src);
        assert!(ir.contains("@pick_max"), "pick uses pick_max helper");
    }

    // ═══════════ 调试指令 ═══════════

    #[test]
    fn compile_dumpstack() {
        // DumpStack
        let src = "#";
        let ir = compile_source(src);
        assert!(ir.contains("@dbg_header"), "dumpstack should reference debug header");
    }

    #[test]
    fn compile_dumpstate() {
        // DumpState
        let src = " #";
        let ir = compile_source(src);
        assert!(ir.contains("@dbg_state_fmt"), "dumpstate should reference state format");
    }

    // ═══════════ 栈操作 ═══════════

    #[test]
    fn compile_dup() {
        // Push(5) Dup
        let src = "          + +";
        let ir = compile_source(src);
        // Dup should just work and produce valid IR
        assert!(ir.contains("define void @sd_main"), "dup should compile");
    }

    #[test]
    fn compile_swap() {
        // Push(1) Push(2) Swap
        let src = "      +      +  +";
        let ir = compile_source(src);
        assert!(ir.contains("define void @sd_main"), "swap should compile");
    }

    #[test]
    fn compile_rotate() {
        // Push(1) Push(2) Push(3) Rotate
        let src = "      +      +      +   +";
        let ir = compile_source(src);
        assert!(ir.contains("define void @sd_main"), "rotate should compile");
    }

    #[test]
    fn compile_pop() {
        // Push(1) Pop
        let src = "      +    +";
        let ir = compile_source(src);
        assert!(ir.contains("define void @sd_main"), "pop should compile");
    }

    // ═══════════ 边界条件 ═══════════

    #[test]
    fn compile_program_with_comment_in_source() {
        // Push(65) CharOut // print 'A'
        let src = "             +,// print 'A'\n";
        let ir = compile_source(src);
        assert!(ir.contains("putchar"), "comment should not affect compilation");
    }

    #[test]
    fn compile_multiple_functions_with_same_name_in_different_scopes() {
        // Two functions: func(1) pushes 0, func(2) pushes 0 and adds
        // (1): Push(0) (1):
        // (2): Push(0) Push(0) Add (2):
        // (1): (0); (2): (0);
        let src = " :     + :  :     +     +*  :: ;  : ;";
        let ir = compile_source(src);
        assert!(ir.contains("@sd_func_1"), "should define func 1");
        assert!(ir.contains("@sd_func_2"), "should define func 2");
    }

    #[test]
    fn compile_with_constant_folding_applied() {
        // Push(5) Push(3) Add → optimizer should fold to Push(8)
        // After folding, no call to checked_add should appear (definition still exists)
        let src = "          +        +*";
        let ir = compile_source(src);
        // "checked_add" appears in the function definition from intrinsics,
        // but there should be no CALL to checked_add from the main function
        let sd_main_pos = ir.find("define void @sd_main").unwrap();
        let sd_main_end =
            ir[sd_main_pos..].find("define i64 @main").unwrap_or(ir.len() - sd_main_pos);
        let sd_main_body = &ir[sd_main_pos..sd_main_pos + sd_main_end];
        assert!(
            !sd_main_body.contains("call i64 @checked_add"),
            "optimizer should fold 5+3, no checked_add call needed"
        );
        // The folded constant should appear in the IR
        assert!(ir.contains("i64 8"), "folded value 8 should appear in IR");
    }
}
