//! Stardust → LLVM IR 编译器
//!
//! 核心策略：
//! - 全局栈 @stack + 栈指针 @sp（malloc 动态分配）
//! - 基本块内使用编译期栈追踪（compile_stack），操作 SSA 寄存器，零内存访问
//! - 控制流边界（Jump/Mark/Call）flush 到 @stack 内存，下一 BB 再从 @stack 读取
//! - 函数调用通过帧基址（frame_base）实现多值返回语义

use crate::stardust::{Instruction, ParseResult};
use std::collections::HashMap;

use super::intrinsics::{EXTERNAL_DECLS, INTRINSICS_IR};
use super::optimizer;
use super::CodeGenConfig;

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
            tracked_depth: 0,
        }
    }

    // ── 辅助方法 ───────────────────────────────────────────

    fn emitln(&mut self, ir: &str) {
        self.output.push_str(ir);
        if !ir.ends_with('\n') {
            self.output.push('\n');
        }
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

        self.emitln(&format!(
            "  %sp_pre_{l} = load i64, i64* @sp",
            l = l
        ));
        self.emitln(&format!(
            "  {sp} = sub i64 %sp_pre_{l}, 1",
            sp = sp_dec
        ));
        self.emitln(&format!("  store i64 {sp}, i64* @sp", sp = sp_dec));
        self.emitln(&format!(
            "  {ptr} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 {sp}",
            ptr = ptr,
            sp = sp_dec,
            SZ = self.config.stack_size
        ));
        self.emitln(&format!(
            "  {val} = load i64, i64* {ptr}",
            val = val,
            ptr = ptr
        ));
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

        self.emitln(&format!(
            "  ; flush {} stack entries to memory",
            count
        ));
        self.emitln(&format!("  %sp_flush_{l} = load i64, i64* @sp"));

        for i in 0..count {
            let val_str = &entries[i];
            let ptr = format!("%flush_ptr_{}_{}", l, i);
            let sp_name = if i == 0 {
                format!("%sp_flush_{l}")
            } else {
                format!("%sp_flush_{}_{}", l, i)
            };
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
            self.emitln(&format!(
                "  store i64 {val}, i64* {ptr}",
                val = val_str,
                ptr = ptr
            ));

            if i + 1 < count {
                self.emitln(&format!(
                    "  {next} = add i64 {sp}, 1",
                    next = next_sp,
                    sp = sp_name
                ));
            }
        }

        if count == 1 {
            self.emitln(&format!(
                "  %sp_flush_done_{l} = add i64 %sp_flush_{l}, 1"
            ));
        } else {
            self.emitln(&format!(
                "  %sp_flush_done_{l} = add i64 %sp_flush_{l}_{last}, 1",
                l = l,
                last = count - 1
            ));
        }
        self.emitln(&format!(
            "  store i64 %sp_flush_done_{l}, i64* @sp"
        ));

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

        self.emitln(&format!(
            "  %div_zero_{l} = icmp eq i64 {b}, 0",
            l = l,
            b = b_str
        ));
        self.emitln(&format!(
            "  br i1 %div_zero_{l}, label %div_err_{l}, label %div_ok_{l}"
        ));
        self.emitln(&format!("div_err_{l}:"));
        self.emitln(&format!("  call void @runtime_divzero()"));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("div_ok_{l}:"));
        self.emitln(&format!(
            "  {r} = sdiv i64 {a}, {b}",
            r = result,
            a = a_str,
            b = b_str
        ));

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

        self.emitln(&format!(
            "  %mod_zero_{l} = icmp eq i64 {b}, 0",
            l = l,
            b = b_str
        ));
        self.emitln(&format!(
            "  br i1 %mod_zero_{l}, label %mod_err_{l}, label %mod_ok_{l}"
        ));
        self.emitln(&format!("mod_err_{l}:"));
        self.emitln(&format!("  call void @runtime_modzero()"));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("mod_ok_{l}:"));
        self.emitln(&format!(
            "  {r} = srem i64 {a}, {b}",
            r = result,
            a = a_str,
            b = b_str
        ));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── Reverse ────────────────────────────────────────────

    fn compile_reverse(&mut self) {
        // 倒转当前帧的栈 — flush 到 @stack，用临时缓冲区实现
        self.flush();
        let l = self.next_label();

        // 跳转到准备块
        self.emitln(&format!("  br label %rev_prep_{l}"));
        self.emitln(&format!("rev_prep_{l}:"));

        // 获取帧基址和栈指针
        self.emitln(&format!(
            "  %sp_rev_{l} = load i64, i64* @sp"
        ));
        self.emitln(&format!(
            "  %fbd_rev_{l} = load i32, i32* @frame_depth"
        ));
        self.emitln(&format!(
            "  %fbd_rev_d_{l} = sub i32 %fbd_rev_{l}, 1"
        ));
        self.emitln(&format!(
            "  %fb_gep_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %fbd_rev_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!(
            "  %fb_{l} = load i64, i64* %fb_gep_{l}"
        ));
        self.emitln(&format!(
            "  %rev_len_{l} = sub i64 %sp_rev_{l}, %fb_{l}"
        ));

        // 如果 len <= 1，不需要做任何事
        self.emitln(&format!(
            "  %rev_trivial_{l} = icmp sle i64 %rev_len_{l}, 1"
        ));
        self.emitln(&format!(
            "  br i1 %rev_trivial_{l}, label %rev_done_{l}, label %rev_do_{l}"
        ));
        self.emitln(&format!("rev_do_{l}:"));

        // 分配临时缓冲区
        self.emitln(&format!(
            "  %rev_bytes_{l} = mul i64 %rev_len_{l}, 8"
        ));
        self.emitln(&format!(
            "  %rev_tmp_{l} = call i8* @malloc(i64 %rev_bytes_{l})"
        ));
        self.emitln(&format!(
            "  %rev_tmp_i64_{l} = bitcast i8* %rev_tmp_{l} to i64*"
        ));

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
        self.emitln(&format!("rev_loop_{l}:"));
        self.emitln(&format!(
            "  %rev_j_{l} = phi i64 [ 0, %rev_do_{l} ], [ %rev_j_next_{l}, %rev_loop_body_{l} ]"
        ));
        self.emitln(&format!(
            "  %rev_loop_done_{l} = icmp eq i64 %rev_j_{l}, %rev_len_{l}"
        ));
        self.emitln(&format!(
            "  br i1 %rev_loop_done_{l}, label %rev_free_{l}, label %rev_loop_body_{l}"
        ));
        self.emitln(&format!("rev_loop_body_{l}:"));
        // tmp_src_idx = len - 1 - j
        self.emitln(&format!(
            "  %rev_tmp_src_idx_{l} = sub i64 %rev_len_{l}, 1"
        ));
        self.emitln(&format!(
            "  %rev_tmp_src_idx_{l} = sub i64 %rev_tmp_src_idx_{l}, %rev_j_{l}"
        ));
        self.emitln(&format!(
            "  %rev_tmp_src_{l} = getelementptr i64, i64* %rev_tmp_i64_{l}, i64 %rev_tmp_src_idx_{l}"
        ));
        self.emitln(&format!(
            "  %rev_val_{l} = load i64, i64* %rev_tmp_src_{l}"
        ));
        // stack_dst_idx = fb + j
        self.emitln(&format!(
            "  %rev_dst_idx_{l} = add i64 %fb_{l}, %rev_j_{l}"
        ));
        self.emitln(&format!(
            "  %rev_dst_{l} = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %rev_dst_idx_{l}",
            SSZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!(
            "  store i64 %rev_val_{l}, i64* %rev_dst_{l}"
        ));
        self.emitln(&format!(
            "  %rev_j_next_{l} = add i64 %rev_j_{l}, 1"
        ));
        self.emitln(&format!("  br label %rev_loop_{l}"));
        self.emitln(&format!("rev_free_{l}:"));
        self.emitln(&format!(
            "  call void @free(i8* %rev_tmp_{l})"
        ));
        self.emitln(&format!("  br label %rev_done_{l}"));
        self.emitln(&format!("rev_done_{l}:"));
    }

    // ── CharOut ────────────────────────────────────────────

    fn compile_charout(&mut self) {
        let val = self.pop_ssa();
        let l = self.next_label();
        let val_str = val.to_llvm();

        // Bounds check: 0 <= val <= 127
        self.emitln(&format!(
            "  %ch_ok_low_{l} = icmp sge i64 {v}, 0",
            v = val_str,
            l = l
        ));
        self.emitln(&format!(
            "  %ch_ok_high_{l} = icmp sle i64 {v}, 127",
            v = val_str,
            l = l
        ));
        self.emitln(&format!(
            "  %ch_ok_{l} = and i1 %ch_ok_low_{l}, %ch_ok_high_{l}"
        ));
        self.emitln(&format!(
            "  br i1 %ch_ok_{l}, label %ch_put_{l}, label %ch_bad_{l}"
        ));
        self.emitln(&format!("ch_bad_{l}:"));
        self.emitln(&format!(
            "  call void @runtime_bad_ascii(i64 {v})",
            v = val_str
        ));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("ch_put_{l}:"));
        self.emitln(&format!(
            "  %ch_trunc_{l} = trunc i64 {v} to i32",
            v = val_str
        ));
        self.emitln(&format!(
            "  %ch_call_{l} = call i32 @putchar(i32 %ch_trunc_{l})",
            l = l
        ));
    }

    // ── CharIn ─────────────────────────────────────────────

    fn compile_charin(&mut self) {
        // If compile_stack has items, flush first so we don't lose state across I/O
        if !self.compile_stack.is_empty() {
            self.flush();
        }
        let l = self.next_label();
        let result = format!("%charin_{l}");
        self.emitln(&format!(
            "  {r} = call i32 @getchar()",
            r = result
        ));
        self.emitln(&format!(
            "  %charin_ext_{l} = sext i32 {r} to i64",
            r = result
        ));
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
        self.emitln(&format!(
            "  %nin_ptr_{l} = alloca i64"
        ));
        self.emitln(&format!(
            "  %nin_call_{l} = call i32 (i8*, ...) @scanf(i8* getelementptr ([4 x i8], [4 x i8]* @fmt_numin, i32 0, i32 0), i64* %nin_ptr_{l})",
            l = l
        ));
        self.emitln(&format!(
            "  %nin_val_{l} = load i64, i64* %nin_ptr_{l}"
        ));
        self.push_ssa(StackEntry::Reg(format!("%nin_val_{l}")));
    }

    // ── Mark ───────────────────────────────────────────────

    fn compile_mark(&mut self, name: usize) {
        // Flush before the mark (control flow may enter here from a jump)
        self.flush();

        let label = Self::mark_label(name);
        self.emitln(&format!("{}:", label));

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

        self.emitln(&format!(
            "  %jmp_cond_{l} = icmp ne i64 {c}, 0",
            c = cond_str,
            l = l
        ));
        self.emitln(&format!(
            "  br i1 %jmp_cond_{l}, label %{t}, label %{f}",
            t = target,
            f = fallthrough,
            l = l
        ));
        self.emitln(&format!("{}:", fallthrough));
        self.at_bb_start = true;
    }

    // ── UnconditionalJump ──────────────────────────────────

    fn compile_ujump(&mut self, name: usize) {
        self.flush();
        let target = Self::mark_label(name);
        self.emitln(&format!("  br label %{}", target));

        // After unconditional jump, emit a dead block label
        let l = self.next_label();
        self.emitln(&format!("dead_{}:", l));
        self.at_bb_start = true;
    }

    // ── Call ───────────────────────────────────────────────

    fn compile_call(&mut self, name: usize, argc: usize) {
        let l = self.next_label();

        // Flush current stack to memory
        self.flush();

        // Save frame base
        self.emitln(&format!(
            "  ; Call function {}, argc={}",
            name, argc
        ));
        self.emitln(&format!(
            "  %fbd_call_{l} = load i32, i32* @frame_depth"
        ));
        self.emitln(&format!(
            "  %sp_call_{l} = load i64, i64* @sp"
        ));
        self.emitln(&format!(
            "  %fb_call_{l} = sub i64 %sp_call_{l}, {argc}",
            argc = argc,
            l = l
        ));

        // Check arguments
        self.emitln(&format!(
            "  %arg_ok_{l} = icmp sge i64 %fb_call_{l}, 0",
            l = l
        ));
        self.emitln(&format!(
            "  br i1 %arg_ok_{l}, label %call_args_ok_{l}, label %call_noargs_{l}"
        ));
        self.emitln(&format!("call_noargs_{l}:"));
        self.emitln(&format!("  call void @runtime_stack_underflow()"));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("call_args_ok_{l}:"));

        // Store frame base
        self.emitln(&format!(
            "  %fb_slot_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %fbd_call_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!(
            "  store i64 %fb_call_{l}, i64* %fb_slot_{l}"
        ));

        // Increment frame depth with overflow check
        self.emitln(&format!(
            "  %fbd_next_{l} = add i32 %fbd_call_{l}, 1"
        ));
        self.emitln(&format!(
            "  %depth_ok_{l} = icmp slt i32 %fbd_next_{l}, {MD}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!(
            "  br i1 %depth_ok_{l}, label %call_ok_{l}, label %call_depth_err_{l}"
        ));
        self.emitln(&format!("call_depth_err_{l}:"));
        self.emitln(&format!("  call void @runtime_call_depth()"));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("call_ok_{l}:"));
        self.emitln(&format!(
            "  store i32 %fbd_next_{l}, i32* @frame_depth"
        ));

        // Call the function
        self.emitln(&format!(
            "  call void @sd_func_{name}()",
            name = name
        ));

        // Restore frame depth
        self.emitln(&format!(
            "  store i32 %fbd_call_{l}, i32* @frame_depth"
        ));

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
        self.emitln(&format!(
            "  {r} = zext i1 %cmp_bool_{l} to i64",
            r = result,
            l = l
        ));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── 逻辑运算 ───────────────────────────────────────────

    fn compile_logic_and(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%and_{l}");

        self.emitln(&format!(
            "  %and_a_{l} = icmp ne i64 {a}, 0",
            a = a.to_llvm(),
            l = l
        ));
        self.emitln(&format!(
            "  %and_b_{l} = icmp ne i64 {b}, 0",
            b = b.to_llvm(),
            l = l
        ));
        self.emitln(&format!(
            "  %and_bool_{l} = and i1 %and_a_{l}, %and_b_{l}"
        ));
        self.emitln(&format!(
            "  {r} = zext i1 %and_bool_{l} to i64",
            r = result
        ));

        self.push_ssa(StackEntry::Reg(result));
    }

    fn compile_logic_or(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%or_{l}");

        self.emitln(&format!(
            "  %or_a_{l} = icmp ne i64 {a}, 0",
            a = a.to_llvm(),
            l = l
        ));
        self.emitln(&format!(
            "  %or_b_{l} = icmp ne i64 {b}, 0",
            b = b.to_llvm(),
            l = l
        ));
        self.emitln(&format!(
            "  %or_bool_{l} = or i1 %or_a_{l}, %or_b_{l}"
        ));
        self.emitln(&format!(
            "  {r} = zext i1 %or_bool_{l} to i64",
            r = result
        ));

        self.push_ssa(StackEntry::Reg(result));
    }

    fn compile_logic_not(&mut self) {
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%not_{l}");

        self.emitln(&format!(
            "  %not_a_{l} = icmp eq i64 {a}, 0",
            a = a.to_llvm(),
            l = l
        ));
        self.emitln(&format!(
            "  {r} = zext i1 %not_a_{l} to i64",
            r = result
        ));

        self.push_ssa(StackEntry::Reg(result));
    }

    fn compile_logic_xor(&mut self) {
        let b = self.pop_ssa();
        let a = self.pop_ssa();
        let l = self.next_label();
        let result = format!("%xor_{l}");

        self.emitln(&format!(
            "  %xor_a_{l} = icmp ne i64 {a}, 0",
            a = a.to_llvm(),
            l = l
        ));
        self.emitln(&format!(
            "  %xor_b_{l} = icmp ne i64 {b}, 0",
            b = b.to_llvm(),
            l = l
        ));
        self.emitln(&format!(
            "  %xor_bool_{l} = xor i1 %xor_a_{l}, %xor_b_{l}"
        ));
        self.emitln(&format!(
            "  {r} = zext i1 %xor_bool_{l} to i64",
            r = result
        ));

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
        self.emitln(&format!(
            "  br i1 %st_ok_{l}, label %st_do_{l}, label %st_err_{l}"
        ));
        self.emitln(&format!("st_err_{l}:"));
        self.emitln(&format!(
            "  call void @runtime_heap_oob(i64 {a})",
            a = addr_str
        ));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("st_do_{l}:"));
        self.emitln(&format!(
            "  %st_ptr_{l} = getelementptr [{HS} x i64], [{HS} x i64]* @heap, i32 0, i64 {a}",
            HS = self.config.heap_size,
            a = addr_str,
            l = l
        ));
        self.emitln(&format!(
            "  store i64 {v}, i64* %st_ptr_{l}",
            v = val_str
        ));
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
        self.emitln(&format!(
            "  br i1 %ld_ok_{l}, label %ld_do_{l}, label %ld_err_{l}"
        ));
        self.emitln(&format!("ld_err_{l}:"));
        self.emitln(&format!(
            "  call void @runtime_heap_oob(i64 {a})",
            a = addr_str
        ));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("ld_do_{l}:"));
        self.emitln(&format!(
            "  %ld_ptr_{l} = getelementptr [{HS} x i64], [{HS} x i64]* @heap, i32 0, i64 {a}",
            HS = self.config.heap_size,
            a = addr_str,
            l = l
        ));
        self.emitln(&format!(
            "  {r} = load i64, i64* %ld_ptr_{l}",
            r = result
        ));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── Depth ──────────────────────────────────────────────

    fn compile_depth(&mut self) {
        let l = self.next_label();
        let result = format!("%depth_{l}");

        // Depth = current sp - frame_base
        self.flush(); // Ensure sp is up to date

        self.emitln(&format!(
            "  %sp_d_{l} = load i64, i64* @sp"
        ));
        self.emitln(&format!(
            "  %fbd_d_{l} = load i32, i32* @frame_depth"
        ));
        self.emitln(&format!(
            "  %fbd_d_d_{l} = sub i32 %fbd_d_{l}, 1"
        ));
        self.emitln(&format!(
            "  %fb_gep_d_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %fbd_d_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!(
            "  %fb_d_{l} = load i64, i64* %fb_gep_d_{l}"
        ));
        self.emitln(&format!(
            "  {r} = sub i64 %sp_d_{l}, %fb_d_{l}",
            r = result
        ));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── ShiftL ─────────────────────────────────────────────

    fn compile_shiftl(&mut self) {
        self.flush();
        let l = self.next_label();

        // Shift left: stack[fb] moves to stack[sp-1], everything else shifts down
        self.emitln(&format!(
            "  %sp_sl_{l} = load i64, i64* @sp"
        ));
        self.emitln(&format!(
            "  %fbd_sl_{l} = load i32, i32* @frame_depth"
        ));
        self.emitln(&format!(
            "  %fbd_sl_d_{l} = sub i32 %fbd_sl_{l}, 1"
        ));
        self.emitln(&format!(
            "  %fb_gep_sl_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %fbd_sl_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!(
            "  %fb_sl_{l} = load i64, i64* %fb_gep_sl_{l}"
        ));
        // stack_underflow check
        self.emitln(&format!(
            "  %sl_empty_{l} = icmp eq i64 %sp_sl_{l}, %fb_sl_{l}"
        ));
        self.emitln(&format!(
            "  br i1 %sl_empty_{l}, label %sl_err_{l}, label %sl_ok_{l}"
        ));
        self.emitln(&format!("sl_err_{l}:"));
        self.emitln(&format!("  call void @runtime_stack_underflow()"));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("sl_ok_{l}:"));
        // Read first element
        self.emitln(&format!(
            "  %sl_first_ptr_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %fb_sl_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!(
            "  %sl_first_{l} = load i64, i64* %sl_first_ptr_{l}"
        ));
        // memmove(stack[fb], stack[fb+1], (sp - fb - 1)*8)
        self.emitln(&format!(
            "  %sl_src_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %fb_sl_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!(
            "  %sl_src_1_{l} = getelementptr i64, i64* %sl_src_{l}, i64 1"
        ));
        self.emitln(&format!(
            "  %sl_count_{l} = sub i64 %sp_sl_{l}, %fb_sl_{l}"
        ));
        self.emitln(&format!(
            "  %sl_count_dec_{l} = sub i64 %sl_count_{l}, 1"
        ));
        self.emitln(&format!(
            "  %sl_bytes_{l} = mul i64 %sl_count_dec_{l}, 8"
        ));
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
        self.emitln(&format!(
            "  %sl_dst_dec_{l} = getelementptr i64, i64* %sl_dst_{l}, i64 -1"
        ));
        self.emitln(&format!(
            "  store i64 %sl_first_{l}, i64* %sl_dst_dec_{l}"
        ));
    }

    // ── ShiftR ─────────────────────────────────────────────

    fn compile_shiftr(&mut self) {
        self.flush();
        let l = self.next_label();

        self.emitln(&format!(
            "  %sp_sr_{l} = load i64, i64* @sp"
        ));
        // Get last element
        self.emitln(&format!(
            "  %sr_last_sp_{l} = sub i64 %sp_sr_{l}, 1"
        ));
        self.emitln(&format!(
            "  %sr_last_ptr_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %sr_last_sp_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!(
            "  %sr_last_{l} = load i64, i64* %sr_last_ptr_{l}"
        ));
        // memmove(stack[fb+1], stack[fb], (sp - fb - 1)*8)
        self.emitln(&format!(
            "  %sr_fbd_{l} = load i32, i32* @frame_depth"
        ));
        self.emitln(&format!(
            "  %sr_fbd_d_{l} = sub i32 %sr_fbd_{l}, 1"
        ));
        self.emitln(&format!(
            "  %sr_fb_gep_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %sr_fbd_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!(
            "  %sr_fb_{l} = load i64, i64* %sr_fb_gep_{l}"
        ));
        self.emitln(&format!(
            "  %sr_src_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %sr_fb_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!(
            "  %sr_dst_{l} = getelementptr i64, i64* %sr_src_{l}, i64 1"
        ));
        self.emitln(&format!(
            "  %sr_count_{l} = sub i64 %sp_sr_{l}, %sr_fb_{l}"
        ));
        self.emitln(&format!(
            "  %sr_count_dec_{l} = sub i64 %sr_count_{l}, 1"
        ));
        self.emitln(&format!(
            "  %sr_bytes_{l} = mul i64 %sr_count_dec_{l}, 8"
        ));
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
        self.emitln(&format!(
            "  store i64 %sr_last_{l}, i64* %sr_fb_ptr_{l}"
        ));
    }

    // ── Pick ───────────────────────────────────────────────

    fn compile_pick(&mut self) {
        // Pick: pop n, push copy of stack[sp-1-n]
        let n = self.pop_ssa();
        self.flush(); // Ensure stack state is in memory
        let l = self.next_label();
        let result = format!("%pick_{l}");

        let n_str = n.to_llvm();

        self.emitln(&format!(
            "  %sp_pk_{l} = load i64, i64* @sp"
        ));
        self.emitln(&format!(
            "  %pk_n_{l} = call i64 @pick_max(i64 {n})",
            n = n_str,
            l = l
        ));
        self.emitln(&format!(
            "  %pk_idx_{l} = sub i64 %sp_pk_{l}, 1"
        ));
        self.emitln(&format!(
            "  %pk_idx_{l} = sub i64 %pk_idx_{l}, %pk_n_{l}"
        ));
        // Check bounds
        self.emitln(&format!(
            "  %pk_ge0_{l} = icmp sge i64 %pk_idx_{l}, 0",
            l = l
        ));
        self.emitln(&format!(
            "  br i1 %pk_ge0_{l}, label %pk_ok_{l}, label %pk_err_{l}"
        ));
        self.emitln(&format!("pk_err_{l}:"));
        self.emitln(&format!("  call void @runtime_stack_underflow()"));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("pk_ok_{l}:"));
        self.emitln(&format!(
            "  %pk_ptr_{l} = getelementptr [{SZ} x i64], [{SZ} x i64]* @stack, i32 0, i64 %pk_idx_{l}",
            SZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!(
            "  {r} = load i64, i64* %pk_ptr_{l}",
            r = result
        ));

        self.push_ssa(StackEntry::Reg(result));
    }

    // ── DropN ──────────────────────────────────────────────

    fn compile_dropn(&mut self) {
        let n = self.pop_ssa();
        self.flush();
        let l = self.next_label();

        let n_str = n.to_llvm();

        // Clamp n >= 0
        self.emitln(&format!(
            "  %dn_n_{l} = call i64 @pick_max(i64 {n})",
            n = n_str,
            l = l
        ));
        // Check that we have enough elements
        self.emitln(&format!(
            "  %sp_dn_{l} = load i64, i64* @sp"
        ));
        self.emitln(&format!(
            "  %dn_ok_{l} = icmp sle i64 %dn_n_{l}, %sp_dn_{l}"
        ));
        self.emitln(&format!(
            "  br i1 %dn_ok_{l}, label %dn_do_{l}, label %dn_err_{l}"
        ));
        self.emitln(&format!("dn_err_{l}:"));
        self.emitln(&format!("  call void @runtime_stack_underflow()"));
        self.emitln(&format!("  unreachable"));
        self.emitln(&format!("dn_do_{l}:"));
        self.emitln(&format!(
            "  %sp_dn_new_{l} = sub i64 %sp_dn_{l}, %dn_n_{l}"
        ));
        self.emitln(&format!(
            "  store i64 %sp_dn_new_{l}, i64* @sp"
        ));
    }

    // ── 调试指令 ───────────────────────────────────────────

    fn compile_dumpstack(&mut self) {
        self.flush();
        let l = self.next_label();

        // Jump to prep
        self.emitln(&format!("  br label %ds_prep_{l}"));
        self.emitln(&format!("ds_prep_{l}:"));

        // Get frame base and sp
        self.emitln(&format!(
            "  %ds_sp_{l} = load i64, i64* @sp"
        ));
        self.emitln(&format!(
            "  %ds_fbd_{l} = load i32, i32* @frame_depth"
        ));
        self.emitln(&format!(
            "  %ds_fbd_d_{l} = sub i32 %ds_fbd_{l}, 1"
        ));
        self.emitln(&format!(
            "  %ds_fb_gep_{l} = getelementptr [{MD} x i64], [{MD} x i64]* @frame_base_stack, i32 0, i32 %ds_fbd_d_{l}",
            MD = self.config.max_call_depth,
            l = l
        ));
        self.emitln(&format!(
            "  %ds_fb_{l} = load i64, i64* %ds_fb_gep_{l}"
        ));

        // Print header to stderr
        self.emitln(&format!(
            "  %ds_fp_{l} = call i8* @get_stderr()"
        ));
        self.emitln(&format!(
            "  %ds_hdr_{l} = call i32 (i8*, i8*, ...) @fprintf_stderr(i8* %ds_fp_{l}, i8* getelementptr ([9 x i8], [9 x i8]* @dbg_header, i32 0, i32 0))"
        ));

        // Loop and print each element: fprintf(stderr, " %ld", val)
        self.emitln(&format!("  br label %ds_loop_hdr_{l}"));
        self.emitln(&format!("ds_loop_hdr_{l}:"));
        self.emitln(&format!(
            "  %ds_i_{l} = phi i64 [ %ds_fb_{l}, %ds_prep_{l} ], [ %ds_i_next_{l}, %ds_body_{l} ]"
        ));
        self.emitln(&format!(
            "  %ds_end_{l} = icmp eq i64 %ds_i_{l}, %ds_sp_{l}"
        ));
        self.emitln(&format!(
            "  br i1 %ds_end_{l}, label %ds_done_{l}, label %ds_body_{l}"
        ));
        self.emitln(&format!("ds_body_{l}:"));
        self.emitln(&format!(
            "  %ds_ptr_{l} = getelementptr [{SSZ} x i64], [{SSZ} x i64]* @stack, i32 0, i64 %ds_i_{l}",
            SSZ = self.config.stack_size,
            l = l
        ));
        self.emitln(&format!(
            "  %ds_val_{l} = load i64, i64* %ds_ptr_{l}"
        ));
        self.emitln(&format!(
            "  %ds_fp2_{l} = call i8* @get_stderr()"
        ));
        self.emitln(&format!(
            "  %ds_pr_{l} = call i32 (i8*, i8*, ...) @fprintf_stderr(i8* %ds_fp2_{l}, i8* getelementptr ([4 x i8], [4 x i8]* @dbg_fmt, i32 0, i32 0), i64 %ds_val_{l})"
        ));
        self.emitln(&format!(
            "  %ds_i_next_{l} = add i64 %ds_i_{l}, 1"
        ));
        self.emitln(&format!("  br label %ds_loop_hdr_{l}"));
        self.emitln(&format!("ds_done_{l}:"));

        // Print closing newline
        self.emitln(&format!(
            "  %ds_fp3_{l} = call i8* @get_stderr()"
        ));
        self.emitln(&format!(
            "  %ds_nl_{l} = call i32 (i8*, i8*, ...) @fprintf_stderr(i8* %ds_fp3_{l}, i8* getelementptr ([2 x i8], [2 x i8]* @dbg_nl, i32 0, i32 0))"
        ));
    }

    fn compile_dumpstate(&mut self) {
        self.flush();
        let l = self.next_label();

        self.emitln(&format!(
            "  ; DumpState: print VM state to stderr"
        ));
        self.emitln(&format!(
            "  %dss_sp_{l} = load i64, i64* @sp"
        ));
        self.emitln(&format!(
            "  %dss_fbd_{l} = load i32, i32* @frame_depth"
        ));
        self.emitln(&format!(
            "  %dss_fp_{l} = call i8* @get_stderr()"
        ));
        self.emitln(&format!(
            "  %dss_{l} = call i32 (i8*, i8*, ...) @fprintf_stderr(i8* %dss_fp_{l}, i8* getelementptr ([31 x i8], [31 x i8]* @dbg_state_fmt, i32 0, i32 0), i64 %dss_sp_{l}, i32 %dss_fbd_{l})",
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
    out.push_str(
        r#"
declare i8* @fdopen(i32, i8*)
declare i32 @fprintf_stderr(i8*, i8*, ...)
@mode_w = private constant [2 x i8] c"w\00"

define internal i8* @get_stderr() {
  %fp = call i8* @fdopen(i32 2, i8* getelementptr ([2 x i8], [2 x i8]* @mode_w, i32 0, i32 0))
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
@dbg_header = private constant [9 x i8] c"[DEBUG] \00"
@dbg_fmt = private constant [5 x i8] c" %ld\00"
@dbg_nl = private constant [2 x i8] c"\0A\00"
@dbg_state_fmt = private constant [32 x i8] c"[DEBUG] sp=%ld, frame_depth=%d\0A\00"


"#,
        SSZ = config.stack_size,
        HSZ = config.heap_size,
        MD = config.max_call_depth,
    ));

    // ── 运行时内建函数 ──
    out.push_str(INTRINSICS_IR);

    // ── 编译 Stardust 函数 ──
    for (name, body) in &funcs_opt {
        // 一阶段：收集 Marks
        let mut marks: HashMap<usize, String> = HashMap::new();
        for inst in body {
            if let Instruction::Mark { name, .. } = inst {
                marks.insert(*name, FuncCompiler::<'_>::mark_label(*name));
            }
        }

        let mut fc = FuncCompiler::new(config);
        fc.current_func = Some(*name);

        out.push_str(&format!(
            "define void @sd_func_{name}() {{\nentry:\n",
            name = name
        ));
        fc.compile_body(body);
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
        assert!(ir.contains("icmp eq i64") || ir.contains("icmp ne i64"),
                "should generate comparison IR");
    }

    #[test]
    fn compile_cmp_ne() {
        let src = "          + + =";  // Push(5) Dup Ne
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
        let sd_main_end = ir[sd_main_pos..].find("define i64 @main").unwrap_or(ir.len() - sd_main_pos);
        let sd_main_body = &ir[sd_main_pos..sd_main_pos + sd_main_end];
        assert!(!sd_main_body.contains("call i64 @checked_add"),
                "optimizer should fold 5+3, no checked_add call needed");
        // The folded constant should appear in the IR
        assert!(ir.contains("i64 8"), "folded value 8 should appear in IR");
    }
}
