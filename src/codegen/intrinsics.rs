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

;  注意：这些消息里的 `%ld` 只在带参调用点使用；
;  早期版本带有 `at %s` 后缀，但调用点并未传参，会让 printf 读到
;  未初始化的内存（运行时表现为错误消息尾部出现乱码字节）。
@err_overflow   = private constant [18 x i8] c"integer overflow\0A\00"
@err_divzero    = private constant [18 x i8] c"division by zero\0A\00"
@err_modzero    = private constant [16 x i8] c"modulo by zero\0A\00"
@err_badascii   = private constant [24 x i8] c"ASCII %ld out of range\0A\00"
@err_stack      = private constant [17 x i8] c"stack underflow\0A\00"
@err_calldepth  = private constant [21 x i8] c"call depth exceeded\0A\00"
@err_heap_oob   = private constant [22 x i8] c"heap address %ld OOB\0A\00"
@err_io         = private constant [19 x i8] c"stdin read failed\0A\00"
@err_badunicode = private constant [23 x i8] c"character %ld invalid\0A\00"

define internal void @runtime_overflow() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([18 x i8], [18 x i8]* @err_overflow, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_divzero() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([18 x i8], [18 x i8]* @err_divzero, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_modzero() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([16 x i8], [16 x i8]* @err_modzero, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_bad_ascii(i64 %val) {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([24 x i8], [24 x i8]* @err_badascii, i32 0, i32 0), i64 %val)
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_stack_underflow() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([17 x i8], [17 x i8]* @err_stack, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_call_depth() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([21 x i8], [21 x i8]* @err_calldepth, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_heap_oob(i64 %addr) {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([22 x i8], [22 x i8]* @err_heap_oob, i32 0, i32 0), i64 %addr)
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_io_error() {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([19 x i8], [19 x i8]* @err_io, i32 0, i32 0))
  call void @exit(i64 1)
  unreachable
}

; ═══════════════════════════════════════════════════════════════
; Unicode / UTF-8 字符输出
; ═══════════════════════════════════════════════════════════════

define internal void @runtime_bad_unicode(i64 %val) {
  %msg = call i32 (i8*, ...) @printf(i8* getelementptr ([23 x i8], [23 x i8]* @err_badunicode, i32 0, i32 0), i64 %val)
  call void @exit(i64 1)
  unreachable
}

define internal void @runtime_char_out_utf8(i64 %cp) {
entry:
  %low = icmp sge i64 %cp, 0
  %high = icmp sle i64 %cp, 1114111
  %ok = and i1 %low, %high
  br i1 %ok, label %surrogate_check, label %bad

surrogate_check:
  %s_low = icmp sge i64 %cp, 55296
  %s_high = icmp sle i64 %cp, 57343
  %surrogate = and i1 %s_low, %s_high
  br i1 %surrogate, label %bad, label %dispatch

bad:
  call void @runtime_bad_unicode(i64 %cp)
  unreachable

dispatch:
  %is_ascii = icmp slt i64 %cp, 128
  br i1 %is_ascii, label %ascii, label %two_check

ascii:
  %ascii32 = trunc i64 %cp to i32
  %r0 = call i32 @putchar(i32 %ascii32)
  ret void

two_check:
  %is_two = icmp slt i64 %cp, 2048
  br i1 %is_two, label %two, label %three_check

two:
  %b0s = lshr i64 %cp, 6
  %b0o = or i64 %b0s, 192
  %b0 = trunc i64 %b0o to i32
  %r1 = call i32 @putchar(i32 %b0)
  %b1m = and i64 %cp, 63
  %b1o = or i64 %b1m, 128
  %b1 = trunc i64 %b1o to i32
  %r2 = call i32 @putchar(i32 %b1)
  ret void

three_check:
  %is_three = icmp slt i64 %cp, 65536
  br i1 %is_three, label %three, label %four

three:
  %b0s3 = lshr i64 %cp, 12
  %b0o3 = or i64 %b0s3, 224
  %b0_3 = trunc i64 %b0o3 to i32
  %r3 = call i32 @putchar(i32 %b0_3)
  %b1s3 = lshr i64 %cp, 6
  %b1m3 = and i64 %b1s3, 63
  %b1o3 = or i64 %b1m3, 128
  %b1_3 = trunc i64 %b1o3 to i32
  %r4 = call i32 @putchar(i32 %b1_3)
  %b2m3 = and i64 %cp, 63
  %b2o3 = or i64 %b2m3, 128
  %b2_3 = trunc i64 %b2o3 to i32
  %r5 = call i32 @putchar(i32 %b2_3)
  ret void

four:
  %b0s4 = lshr i64 %cp, 18
  %b0o4 = or i64 %b0s4, 240
  %b0_4 = trunc i64 %b0o4 to i32
  %r6 = call i32 @putchar(i32 %b0_4)
  %b1s4 = lshr i64 %cp, 12
  %b1m4 = and i64 %b1s4, 63
  %b1o4 = or i64 %b1m4, 128
  %b1_4 = trunc i64 %b1o4 to i32
  %r7 = call i32 @putchar(i32 %b1_4)
  %b2s4 = lshr i64 %cp, 6
  %b2m4 = and i64 %b2s4, 63
  %b2o4 = or i64 %b2m4, 128
  %b2_4 = trunc i64 %b2o4 to i32
  %r8 = call i32 @putchar(i32 %b2_4)
  %b3m4 = and i64 %cp, 63
  %b3o4 = or i64 %b3m4, 128
  %b3_4 = trunc i64 %b3o4 to i32
  %r9 = call i32 @putchar(i32 %b3_4)
  ret void
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
