//! 运行时内建函数 — 以 LLVM IR 文本形式提供
//!
//! 每个编译产物都需要链接这些运行时支持函数。
//! 包括：溢出检查、除零检查、I/O 辅助、错误处理。

/// 所有运行时内建函数的 LLVM IR 声明和定义。
///
/// 这些函数是 `internal` 的，会被链接到每个编译产物中。
/// LLVM 链接时会自动去重或内联它们。
pub const INTRINSICS_IR: &str = r#"
; ═══════════════════════════════════════════════════════════════
; 运行时错误处理
; ═══════════════════════════════════════════════════════════════

@err_overflow   = private constant [24 x i8] c"integer overflow at %s\0A\00"
@err_divzero    = private constant [24 x i8] c"division by zero at %s\0A\00"
@err_modzero    = private constant [22 x i8] c"modulo by zero at %s\0A\00"
@err_badascii   = private constant [24 x i8] c"ASCII %ld out of range\0A\00"
@err_stack      = private constant [23 x i8] c"stack underflow at %s\0A\00"
@err_calldepth  = private constant [27 x i8] c"call depth exceeded at %s\0A\00"
@err_heap_oob   = private constant [22 x i8] c"heap address %ld OOB\0A\00"

define internal void @runtime_overflow() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([25 x i8], [25 x i8]* @err_overflow, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_divzero() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([23 x i8], [23 x i8]* @err_divzero, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_modzero() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([20 x i8], [20 x i8]* @err_modzero, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_bad_ascii(i64 %val) {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([25 x i8], [25 x i8]* @err_badascii, i32 0, i32 0), i64 %val)
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_stack_underflow() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([22 x i8], [22 x i8]* @err_stack, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_call_depth() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([27 x i8], [27 x i8]* @err_calldepth, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_heap_oob(i64 %addr) {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([24 x i8], [24 x i8]* @err_heap_oob, i32 0, i32 0), i64 %addr)
  call void @exit(i64 1)
  unreachable
}

; ═══════════════════════════════════════════════════════════════
; 溢出检查包装 — 使用 LLVM 内建 intrinsic
; ═══════════════════════════════════════════════════════════════

declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.smul.with.overflow.i64(i64, i64)

define internal i64 @checked_add(i64 %a, i64 %b) {
  %r = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %a, i64 %b)
  %val = extractvalue { i64, i1 } %r, 0
  %ovf = extractvalue { i64, i1 } %r, 1
  br i1 %ovf, label %overflow, label %ok
overflow:
  call void @runtime_overflow()
  unreachable
ok:
  ret i64 %val
}

define internal i64 @checked_sub(i64 %a, i64 %b) {
  %r = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %a, i64 %b)
  %val = extractvalue { i64, i1 } %r, 0
  %ovf = extractvalue { i64, i1 } %r, 1
  br i1 %ovf, label %overflow, label %ok
overflow:
  call void @runtime_overflow()
  unreachable
ok:
  ret i64 %val
}

define internal i64 @checked_mul(i64 %a, i64 %b) {
  %r = call { i64, i1 } @llvm.smul.with.overflow.i64(i64 %a, i64 %b)
  %val = extractvalue { i64, i1 } %r, 0
  %ovf = extractvalue { i64, i1 } %r, 1
  br i1 %ovf, label %overflow, label %ok
overflow:
  call void @runtime_overflow()
  unreachable
ok:
  ret i64 %val
}
"#;

/// 外部 C 函数声明 — 所有编译产物都需要这些。
pub const EXTERNAL_DECLS: &str = r#"
; ── 外部 C 标准库函数 ─────────────────────────────────────────
declare i32 @printf(i8*, ...)
declare i32 @putchar(i32)
declare i32 @getchar()
declare i32 @scanf(i8*, ...)
declare i32 @fprintf(i8*, i8*, ...)
declare i8* @malloc(i64)
declare void @free(i8*)
declare void @exit(i64)

; ── I/O 格式字符串 ────────────────────────────────────────────
@fmt_numout  = private constant [4 x i8] c"%ld\00"
@fmt_numout_nl = private constant [5 x i8] c"%ld\0A\00"
@fmt_numin   = private constant [4 x i8] c"%ld\00"
@fmt_stderr  = private constant [4 x i8] c"%s\0A\00"
"#;
