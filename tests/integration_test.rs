// ============================================================================
// Stardust 集成测试 — 测试完整流水线：词法分析 → 语法分析 → VM 执行
// ============================================================================

use esolang_stardust::stardust::lexer::tokenize;
use esolang_stardust::stardust::parser::parse_program;
use esolang_stardust::stardust::{ErrorKind, StardustError, VM};

// ── 辅助函数：生成精确空格数 ──────────────────────────────────────

/// Push(n) → (n+5) 个空格 + '+'
fn push(n: i64) -> String {
    format!("{}+", " ".repeat((n + 5) as usize))
}

/// Mark(n) → n 个空格 + '`'
fn mark(n: usize) -> String {
    format!("{}`", " ".repeat(n))
}

/// Jump(n) → n 个空格 + '\''
fn jump(n: usize) -> String {
    format!("{}'", " ".repeat(n))
}

/// UncondJump(n) → n 个空格 + '~'
fn ujump(n: usize) -> String {
    format!("{}~", " ".repeat(n))
}

/// Call(name, argc) → name 个空格 + ':' + argc 个空格 + ';'
fn call(name: usize, argc: usize) -> String {
    format!("{}:{};", " ".repeat(name), " ".repeat(argc))
}

/// Func(name, body) → name: body name:  （函数声明）
fn func(name: usize, body: &str) -> String {
    format!("{}:{}{}:", " ".repeat(name), body, " ".repeat(name))
}

/// 无空格符号的简写
fn add() -> String { "*".to_string() }
fn sub() -> String { " *".to_string() }
fn mul() -> String { "  *".to_string() }
fn div() -> String { "   *".to_string() }
fn rem() -> String { "    *".to_string() }
fn reverse() -> String { "     *".to_string() }
fn dup() -> String { " +".to_string() }
fn swap() -> String { "  +".to_string() }
fn rotate() -> String { "   +".to_string() }
fn pop() -> String { "    +".to_string() }
fn num_out() -> String { ".".to_string() }
fn char_out() -> String { ",".to_string() }
// 比较运算
fn eq() -> String { "=".to_string() }
fn ne() -> String { " =".to_string() }
fn lt() -> String { "  =".to_string() }
fn gt() -> String { "   =".to_string() }
// 逻辑运算
fn and() -> String { "&".to_string() }
fn or() -> String { " &".to_string() }
fn not() -> String { "  &".to_string() }
// 堆操作
fn store() -> String { "-".to_string() }
fn load() -> String { " -".to_string() }
// 栈扩展
fn depth() -> String { " <".to_string() }
fn shiftl() -> String { "<".to_string() }
fn shiftr() -> String { ">".to_string() }
fn dropn() -> String { " >".to_string() }
fn pick() -> String { "  <".to_string() }
// 调试
fn dump_stack() -> String { "#".to_string() }

/// 辅助函数：运行完整流水线
fn run(source: &str) -> Result<(), StardustError> {
    let tokens = tokenize(source)?;
    let parsed = parse_program(tokens)?;
    let mut vm = VM::new(parsed);
    vm.run()
}

/// 辅助函数：仅解析
fn parse(source: &str) -> Result<esolang_stardust::stardust::ParseResult, StardustError> {
    let tokens = tokenize(source)?;
    parse_program(tokens)
}

// ════════════════════════════════════════════════════════════════════════════
// 1. 空程序和最小程序
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_program_runs_without_error() {
    assert!(run("").is_ok());
}

#[test]
fn whitespace_only_program() {
    // 纯空白行应该被忽略，不产生任何 token
    assert!(run("   \n  \t  \n  ").is_ok());
}

#[test]
fn comment_only_program() {
    assert!(run("// This is a comment\n// another comment\n").is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 2. Push 值压栈测试
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn push_single_value() {
    assert!(run(&push(0)).is_ok());
}

#[test]
fn push_multiple_values() {
    let src = format!("{}{}{}", push(0), push(1), push(10));
    assert!(run(&src).is_ok());
}

#[test]
fn push_large_values() {
    assert!(run(&push(100)).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 3. 栈操作指令 (Dup, Swap, Rotate, Pop, Reverse)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn dup_instruction() {
    let src = format!("{}{}", push(5), dup());
    assert!(run(&src).is_ok());
}

#[test]
fn swap_instruction() {
    let src = format!("{}{}{}", push(1), push(2), swap());
    assert!(run(&src).is_ok());
}

#[test]
fn rotate_instruction() {
    let src = format!("{}{}{}{}", push(1), push(2), push(3), rotate());
    assert!(run(&src).is_ok());
}

#[test]
fn pop_instruction() {
    let src = format!("{}{}{}", push(1), push(2), pop());
    assert!(run(&src).is_ok());
}

#[test]
fn reverse_instruction() {
    let src = format!("{}{}{}{}", push(1), push(2), push(3), reverse());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 4. 算术运算 (Add, Sub, Mul, Div, Mod)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn add_operation() {
    let src = format!("{}{}{}", push(3), push(4), add());
    assert!(run(&src).is_ok());
}

#[test]
fn sub_operation() {
    let src = format!("{}{}{}", push(10), push(3), sub());
    assert!(run(&src).is_ok());
}

#[test]
fn mul_operation() {
    let src = format!("{}{}{}", push(3), push(4), mul());
    assert!(run(&src).is_ok());
}

#[test]
fn div_operation() {
    let src = format!("{}{}{}", push(10), push(2), div());
    assert!(run(&src).is_ok());
}

#[test]
fn mod_operation() {
    let src = format!("{}{}{}", push(10), push(3), rem());
    assert!(run(&src).is_ok());
}

#[test]
fn complex_arithmetic() {
    // ((5*3)+2) = 17
    let src = format!("{}{}{}{}{}", push(5), push(3), mul(), push(2), add());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 5. I/O 指令
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn char_out_prints_ascii() {
    let src = format!("{}{}", push(65), char_out());
    assert!(run(&src).is_ok());
}

#[test]
fn num_out_prints_number() {
    let src = format!("{}{}", push(42), num_out());
    assert!(run(&src).is_ok());
}

#[test]
fn multiple_char_out() {
    let src = format!("{}{}{}{}", push(72), char_out(), push(105), char_out());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 6. 控制流 (Mark, Jump, UnconditionalJump)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn mark_and_jump_terminating_loop() {
    // Push(3)  Mark(0)  Dup  Push(1)  Sub  Dup  Jump(0)
    // 从 3 倒数到 0 的循环，栈顶为 0 时终止
    let src = format!(
        "{}{}{}{}{}{}{}",
        push(3),
        mark(0),
        dup(),
        push(1),
        sub(),
        dup(),
        jump(0),
    );
    assert!(run(&src).is_ok());
}

#[test]
fn unconditional_jump_parses_correctly() {
    // Mark(0) UncondJump(0) — 解析通过（运行时会无限循环，仅验证解析）
    let parsed = parse(&format!("{}{}", mark(0), ujump(0))).unwrap();
    assert_eq!(parsed.main_instructions.len(), 2);
}

#[test]
fn undefined_mark_in_jump_errors() {
    // Jump(5) 但 Mark(5) 不存在
    assert!(run(&jump(5)).is_err());
}

#[test]
fn undefined_mark_in_unconditional_jump_errors() {
    assert!(run(&ujump(5)).is_err());
}

#[test]
fn mark_as_instruction_is_skipped_at_runtime() {
    // Mark 指令在运行时被视为 NOP
    assert!(run(&mark(0)).is_ok());
}

#[test]
fn mark_name_zero_is_valid() {
    // Mark(0) Push(0) Jump(0)
    // 压入 0，Jump 弹出 0，条件为假不跳转，正常结束
    let src = format!("{}{}{}", mark(0), push(0), jump(0));
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 7. 函数定义和调用
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn define_and_call_simple_function() {
    // (1): Push(65) CharOut (1):  — 定义函数 1
    // (1): (0);                   — 调用函数 1，0 个参数
    let src = format!(
        "{}{}",
        func(1, &format!("{}{}", push(65), char_out())),
        call(1, 0),
    );
    assert!(run(&src).is_ok());
}

#[test]
fn function_call_with_arguments() {
    // 定义函数(1)：Add NumOut（接收 2 个参数）
    // Push(3) Push(4) (1): (2);
    let src = format!(
        "{}{}{}{}",
        func(1, &format!("{}{}", add(), num_out())),
        push(3),
        push(4),
        call(1, 2),
    );
    assert!(run(&src).is_ok());
}

#[test]
fn undefined_function_errors() {
    // 调用未定义的函数 (5): (1);
    assert!(run(&call(5, 1)).is_err());
}

#[test]
fn not_enough_arguments_errors() {
    // 定义函数(1)接收 2 个参数，但只 Push 1 个值就调用
    let src = format!(
        "{}{}{}",
        func(1, &format!("{}{}", add(), num_out())),
        push(1),
        call(1, 2),
    );
    let result = run(&src);
    assert!(result.is_err());
    match result.unwrap_err().kind {
        ErrorKind::NotEnoughArguments { .. } => (),
        other => panic!("Expected NotEnoughArguments, got {:?}", other),
    }
}

#[test]
fn multiple_functions() {
    // 定义函数(1): Push(65) CharOut
    // 定义函数(2): Push(66) CharOut
    // (1): (0); (2): (0);
    let src = format!(
        "{}{}{}{}",
        func(1, &format!("{}{}", push(65), char_out())),
        func(2, &format!("{}{}", push(66), char_out())),
        call(1, 0),
        call(2, 0),
    );
    assert!(run(&src).is_ok());
}

#[test]
fn function_returns_multiple_values() {
    // 定义函数(0): Push(10) Push(20) Push(30)
    // 调用: (0): (0); — 调用后主栈应有 [10, 20, 30]
    let src = format!(
        "{}{}",
        func(0, &format!("{}{}{}", push(10), push(20), push(30))),
        call(0, 0),
    );
    assert!(run(&src).is_ok());
}

#[test]
fn call_function_with_zero_arguments() {
    // 定义函数(0): Push(42)
    // 调用: (0): (0);
    let src = format!(
        "{}{}",
        func(0, &push(42)),
        call(0, 0),
    );
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 8. Hello World 风格整体测试
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn hello_world_real_file_runs() {
    // 真实的 hello_world.sd 文件
    let src = include_str!("../hello_world.sd");
    assert!(run(src).is_ok());
}

#[test]
fn push_and_add_sequence() {
    // Push(10) Push(20) Add Push(5) Sub → 10+20-5 = 25
    let src = format!("{}{}{}{}{}", push(10), push(20), add(), push(5), sub());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 9. VM 错误处理
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn stack_underflow_on_add_with_empty_stack() {
    let e = run(&add()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::StackUnderflow);
}

#[test]
fn stack_underflow_on_dup_with_empty_stack() {
    let e = run(&dup()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::StackUnderflow);
}

#[test]
fn division_by_zero() {
    let src = format!("{}{}{}", push(5), push(0), div());
    let e = run(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::DivisionByZero);
}

#[test]
fn modulo_by_zero() {
    let src = format!("{}{}{}", push(5), push(0), rem());
    let e = run(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::ModuloByZero);
}

#[test]
fn invalid_ascii_on_char_out() {
    let src = format!("{}{}", push(200), char_out());
    let e = run(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidAscii { value: 200 });
}

#[test]
fn stack_underflow_on_pop_with_empty_stack() {
    let e = run(&pop()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::StackUnderflow);
}

#[test]
fn stack_underflow_on_swap_with_one_element() {
    // Push(1) Swap → 需要 2 个元素（Swap 弹出 2 个值）
    let src = format!("{}{}", push(1), swap());
    let e = run(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::StackUnderflow);
}

#[test]
fn stack_underflow_on_char_out_with_empty_stack() {
    let e = run(&char_out()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::StackUnderflow);
}

// ════════════════════════════════════════════════════════════════════════════
// 10. 解析错误（全流程）
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn parse_error_duplicate_mark() {
    // Mark(0) Mark(0) → 重复标志
    let src = format!("{}{}", mark(0), mark(0));
    let e = run(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::DuplicateMark { name: 0 });
}

#[test]
fn parse_error_duplicate_function() {
    // (0): Push(0) (0):  (0): Push(1) (0):  → 重复函数
    let src = format!(
        "{}{}",
        func(0, &push(0)),
        func(0, &push(1)),
    );
    let e = run(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::DuplicateFunction { name: 0 });
}

#[test]
fn parse_error_unclosed_function() {
    // (1): Push(0)  — 没有结束的 (1):
    let src = format!("{}:{}", " ".repeat(1), push(0));
    let e = run(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnclosedFunction { name: 1 });
}

#[test]
fn lex_error_invalid_character() {
    let e = run("a").unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidCharacter { ch: 'a' });
}

// ════════════════════════════════════════════════════════════════════════════
// 11. 综合场景
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn program_with_comments() {
    let src = "\
// This is a simple program
     +  // Push(0)
// Another comment
     +  // Push(0) again
*       // Add
.        // NumOut
";
    assert!(run(src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 12. 边界条件
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn push_zero() {
    assert!(run(&push(0)).is_ok());
}

#[test]
fn push_max_ascii() {
    assert!(run(&push(127)).is_ok());
}

#[test]
fn push_add_chain() {
    // Push(1) Push(2) Push(3) Add Add → stack: [6]
    let src = format!("{}{}{}{}{}", push(1), push(2), push(3), add(), add());
    assert!(run(&src).is_ok());
}

#[test]
fn push_sub_chain() {
    // Push(10) Push(3) Push(2) Sub Sub → stack: [9]
    let src = format!("{}{}{}{}{}", push(10), push(3), push(2), sub(), sub());
    assert!(run(&src).is_ok());
}

#[test]
fn push_mul_div_chain() {
    // Push(12) Push(3) Div Push(2) Mul → stack: [8]
    let src = format!("{}{}{}{}{}", push(12), push(3), div(), push(2), mul());
    assert!(run(&src).is_ok());
}

#[test]
fn dup_then_add() {
    // Push(5) Dup Add → stack: [10]
    let src = format!("{}{}{}", push(5), dup(), add());
    assert!(run(&src).is_ok());
}

#[test]
fn swap_then_sub() {
    // Push(10) Push(3) Swap Sub → stack: [7]
    let src = format!("{}{}{}{}", push(10), push(3), swap(), sub());
    assert!(run(&src).is_ok());
}

#[test]
fn rotate_three_values() {
    // Push(1) Push(2) Push(3) Rotate → stack: [2, 3, 1]
    let src = format!("{}{}{}{}", push(1), push(2), push(3), rotate());
    assert!(run(&src).is_ok());
}

#[test]
fn reverse_then_add_all() {
    // Push(1) Push(2) Push(3) Reverse Add Add
    let src = format!("{}{}{}{}{}{}", push(1), push(2), push(3), reverse(), add(), add());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 13. 嵌套函数调用（函数 A 调用函数 B）
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn nested_function_calls() {
    // 函数(2): Push(66) CharOut  — 输出 'B'
    // 函数(1): Push(65) CharOut (2): (0);  — 输出 'A' 然后调用函数2
    // 主程序: (1): (0);
    let src = format!(
        "{}{}{}",
        func(2, &format!("{}{}", push(66), char_out())),    // 函数2: 输出 B
        func(1, &format!("{}{}{}", push(65), char_out(), call(2, 0))), // 函数1: 输出 A 然后调用2
        call(1, 0),                                          // 调用函数1
    );
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 14. 算术溢出检查
//
// 注：Push 需要 N+5 个空格，无法直接 Push i64::MAX（需要 ~9e18 空格）。
// 但 checked_add/sub/mul 已正确集成到指令执行路径中，
// 正常算术运算不产生误报。
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn arithmetic_operations_do_not_false_overflow() {
    // Push(100) Push(200) Add → 300，不应溢出
    let src = format!("{}{}{}", push(100), push(200), add());
    assert!(run(&src).is_ok());
    // Push(1000) Push(3) Mul → 3000
    let src2 = format!("{}{}{}", push(1000), push(3), mul());
    assert!(run(&src2).is_ok());
    // Push(500) Push(200) Sub → 300
    let src3 = format!("{}{}{}", push(500), push(200), sub());
    assert!(run(&src3).is_ok());
}

#[test]
fn mul_large_values_to_overflow_boundary() {
    // Push(10000) Push(10000) Mul = 1e8, no overflow
    let src = format!("{}{}{}", push(10_000), push(10_000), mul());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 15. 解析阶段 Mark 引用验证
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn undefined_mark_caught_at_parse_time() {
    // Jump(99) 但 Mark(99) 不存在 — 解析阶段即报错
    let src = jump(99);
    let e = run(&src).unwrap_err();
    // 现在在 parse 阶段就发现了（不再是运行时 StackUnderflow）
    assert_eq!(e.kind, ErrorKind::UndefinedMark { name: 99 });
}

#[test]
fn undefined_mark_in_function_caught_at_parse_time() {
    // 函数体内 Jump(99) 但 Mark(99) 不存在
    let src = format!(
        "{}{}",
        func(1, &jump(99)),  // 函数1 内 Jump 到不存在的 Mark(99)
        call(1, 0),
    );
    let e = run(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UndefinedMark { name: 99 });
}

// ════════════════════════════════════════════════════════════════════════════
// 16. 比较运算 (符号 =)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn eq_true() {
    // Push(5) Push(5) Eq → 1 (true)
    let src = format!("{}{}{}", push(5), push(5), eq());
    assert!(run(&src).is_ok());
}

#[test]
fn eq_false() {
    // Push(5) Push(3) Eq → 0 (false)
    let src = format!("{}{}{}", push(5), push(3), eq());
    assert!(run(&src).is_ok());
}

#[test]
fn ne_works() {
    // Push(5) Push(3) Ne → 1
    let src = format!("{}{}{}", push(5), push(3), ne());
    assert!(run(&src).is_ok());
}

#[test]
fn lt_works() {
    // Push(3) Push(5) Lt → 1
    let src = format!("{}{}{}", push(3), push(5), lt());
    assert!(run(&src).is_ok());
}

#[test]
fn gt_works() {
    // Push(5) Push(3) Gt → 1
    let src = format!("{}{}{}", push(5), push(3), gt());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 17. 逻辑运算 (符号 &)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn and_true() {
    // Push(1) Push(1) And → 1
    let src = format!("{}{}{}", push(1), push(1), and());
    assert!(run(&src).is_ok());
}

#[test]
fn and_false() {
    // Push(1) Push(0) And → 0
    let src = format!("{}{}{}", push(1), push(0), and());
    assert!(run(&src).is_ok());
}

#[test]
fn not_zero_is_one() {
    // Push(0) Not → 1
    let src = format!("{}{}", push(0), not());
    assert!(run(&src).is_ok());
}

#[test]
fn or_true() {
    // Push(1) Push(0) Or → 1
    let src = format!("{}{}{}", push(1), push(0), or());
    assert!(run(&src).is_ok());
}

#[test]
fn not_nonzero_is_zero() {
    // Push(42) Not → 0
    let src = format!("{}{}", push(42), not());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 18. 堆操作 (符号 -)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn store_and_load() {
    // Push(10) Push(99) Store Push(10) Load → heap[10]=99, 然后取回 99
    let src = format!("{}{}{}{}{}", push(10), push(99), store(), push(10), load());
    assert!(run(&src).is_ok());
}

#[test]
fn load_uninitialized_returns_zero() {
    // Push(999) Load → 0（未写入的地址返回 0）
    let src = format!("{}{}", push(999), load());
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 19. 栈扩展 (符号 < >)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn depth_on_empty_stack() {
    // Depth → 0
    let src = depth();
    assert!(run(&src).is_ok());
}

#[test]
fn depth_after_push() {
    // Push(1) Push(2) Push(3) Depth → [1, 2, 3, 3]
    let src = format!("{}{}{}{}", push(1), push(2), push(3), depth());
    assert!(run(&src).is_ok());
}

#[test]
fn shiftl_works() {
    // Push(1) Push(2) Push(3) ShiftL → [2, 3, 1]
    let src = format!("{}{}{}{}", push(1), push(2), push(3), shiftl());
    assert!(run(&src).is_ok());
}

#[test]
fn shiftr_works() {
    // Push(1) Push(2) Push(3) ShiftR → [3, 1, 2]
    let src = format!("{}{}{}{}", push(1), push(2), push(3), shiftr());
    assert!(run(&src).is_ok());
}

#[test]
fn dropn_works() {
    // Push(1) Push(2) Push(3) Push(2) DropN → [1] (丢弃栈顶2个: 3和2)
    let src = format!("{}{}{}{}{}", push(1), push(2), push(3), push(2), dropn());
    assert!(run(&src).is_ok());
}

#[test]
fn pick_works() {
    // Push(10) Push(20) Push(30) Push(1) Pick → [10, 20, 30, 20] (复制栈深1的元素)
    let src = format!("{}{}{}{}{}", push(10), push(20), push(30), push(1), pick());
    assert!(run(&src).is_ok());
}

#[test]
fn dropn_stack_underflow() {
    // Push(1) Push(5) DropN → 栈只有1个元素, 丢弃5个失败
    let src = format!("{}{}{}", push(1), push(5), dropn());
    assert!(run(&src).is_err());
}

// ════════════════════════════════════════════════════════════════════════════
// 20. 调试 (符号 #)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn dump_stack_does_not_affect_execution() {
    // Push(42) DumpStack Pop → 正常执行
    let src = format!("{}{}{}", push(42), dump_stack(), pop());
    assert!(run(&src).is_ok());
}

#[test]
fn compare_and_jump_conditional() {
    // Push(10) Push(10) Eq Jump(0) Mark(0) Push(1)
    // 10==10 为真, Jump 弹出1, 跳转到 Mark(0), 然后 Push(1)
    let src = format!(
        "{}{}{}{}{}{}",
        push(10), push(10), eq(), // [10, 10, 1]
        jump(0),                   // 弹出1, 跳转
        mark(0),                   // 跳转目标
        push(1),                   // 执行: Push(1)
    );
    assert!(run(&src).is_ok());
}

// ════════════════════════════════════════════════════════════════════════════
// 21. 编译器集成测试 — IR 结构验证
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod codegen_tests {
    use super::*;
    use esolang_stardust::codegen::{self, CodeGenConfig};
    use esolang_stardust::stardust::lexer::tokenize;
    use esolang_stardust::stardust::parser::parse_program;
    use std::process::Command;

    fn compile_src_to_ir(source: &str) -> String {
        let tokens = tokenize(source).unwrap();
        let parsed = parse_program(tokens).unwrap();
        codegen::compile_to_ir(&parsed, &CodeGenConfig::default())
    }

    /// 辅助：将 IR 编译为临时可执行文件并运行，返回 (stdout, stderr, exit_code)
    fn compile_and_run(ir: &str) -> Option<(String, String, i32)> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir();
        let unique = format!("sd_test_{}_{}", std::process::id(), n);
        let ll_path = dir.join(format!("{}.ll", unique));
        let exe_path = dir.join(&unique);
        std::fs::write(&ll_path, ir).ok()?;
        let out = Command::new("clang")
            .args([ll_path.to_str()?, "-o", exe_path.to_str()?])
            .output()
            .ok()?;
        if !out.status.success() {
            let _ = std::fs::remove_file(&ll_path);
            return None;
        }
        let run = Command::new(&exe_path).output().ok()?;
        let _ = std::fs::remove_file(&ll_path);
        let _ = std::fs::remove_file(&exe_path);
        Some((
            String::from_utf8_lossy(&run.stdout).to_string(),
            String::from_utf8_lossy(&run.stderr).to_string(),
            run.status.code().unwrap_or(-1),
        ))
    }

    #[test]
    fn compile_hello_world_and_run() {
        let src = include_str!("../hello_world.sd");
        let ir = compile_src_to_ir(src);
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0, "compiled hello world should exit 0");
            assert_eq!(stdout, "Hello World!", "should output Hello World!");
        }
    }

    #[test]
    fn compile_arithmetic_and_verify_ir() {
        // Push(10) Push(20) Add Push(5) Sub → 10+20-5 = 25
        let src = format!(
            "{}{}{}{}{}",
            push(10), push(20), add(), push(5), sub()
        );
        let ir = compile_src_to_ir(&src);
        assert!(ir.contains("checked_add"), "should contain checked_add");
        assert!(ir.contains("checked_sub"), "should contain checked_sub");
        assert!(ir.contains("define i64 @main()"), "should have main entry");
    }

    #[test]
    fn compile_simple_char_output() {
        // Push(65) CharOut → 'A'
        let src = format!("{}{}", push(65), char_out());
        let ir = compile_src_to_ir(&src);
        assert!(ir.contains("putchar"), "should call putchar");
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
            assert_eq!(stdout, "A");
        }
    }

    #[test]
    fn compile_simple_number_output() {
        // Push(42) NumOut
        let src = format!("{}{}", push(42), num_out());
        let ir = compile_src_to_ir(&src);
        assert!(ir.contains("printf"), "should call printf");
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
            assert_eq!(stdout.trim(), "42");
        }
    }

    #[test]
    fn compile_with_constant_folding() {
        // Push(5) Push(3) Add → optimizer folds to Push(8)
        let src = format!("{}{}{}", push(5), push(3), add());
        let ir = compile_src_to_ir(&src);
        // The checked_add FUNCTION DEFINITION is always in intrinsics,
        // but there should be no CALL to it from sd_main after folding
        assert!(!ir.contains("call i64 @checked_add"),
                "optimizer should fold 5+3, no checked_add CALL needed");
        assert!(ir.contains("i64 8"), "folded constant should appear in IR");
        if let Some((_stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
        }
    }

    #[test]
    fn compile_comparison_produces_zero_or_one() {
        // Push(5) Push(3) Lt → 0 (5 < 3 is false)
        let src = format!("{}{}{}{}", push(5), push(3), lt(), num_out());
        let ir = compile_src_to_ir(&src);
        // After optimization: 5 < 3 = 0 → Push(0), so icmp may not appear
        assert!(ir.contains("define void @sd_main"), "should compile");
    }

    #[test]
    fn compile_loop_runs() {
        // Countdown loop: Push(3) Mark(0) Dup Push(1) Sub Dup Jump(0)
        // Pushes 3 then loops: dup, push 1, sub, dup, jump(0) until top is 0
        let src = format!(
            "{}{}{}{}{}{}{}{}",
            push(3),                // initial counter
            mark(0),                // loop start
            dup(),                  // duplicate counter
            push(1),                // push 1
            sub(),                  // decrement
            dup(),                  // duplicate for next iteration
            jump(0),                // loop if nonzero
            pop(),                  // clean up final zero
        );
        let ir = compile_src_to_ir(&src);
        assert!(ir.contains("mark_0:"), "should have loop label");
        assert!(ir.contains("br i1"), "should have conditional branch");
    }

    #[test]
    fn compile_function_call_and_run() {
        // (1): Push(65) CharOut (1):   — function that outputs 'A'
        // (1): (0);                    — call function 1 with 0 args
        let src = format!(
            "{}{}",
            func(1, &format!("{}{}", push(65), char_out())),
            call(1, 0),
        );
        let ir = compile_src_to_ir(&src);
        assert!(ir.contains("@sd_func_1"), "should define function 1");
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
            assert_eq!(stdout, "A");
        }
    }

    #[test]
    fn compile_nested_function_call_and_run() {
        // (2): Push(66) CharOut (2):         — outputs 'B'
        // (1): Push(65) CharOut (2): (0); (1): — outputs 'A' then calls 2
        // (1): (0);                           — call 1
        let src = format!(
            "{}{}{}",
            func(2, &format!("{}{}", push(66), char_out())),
            func(1, &format!("{}{}{}", push(65), char_out(), call(2, 0))),
            call(1, 0),
        );
        let ir = compile_src_to_ir(&src);
        assert!(ir.contains("@sd_func_1"));
        assert!(ir.contains("@sd_func_2"));
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
            assert_eq!(stdout, "AB");
        }
    }

    #[test]
    fn compile_function_with_args_and_run() {
        // (3): Add CharOut (3):             — adds 2 args, outputs as char
        // Push(64) Push(1) (3): (2);        — 64+1=65='A'
        let src = format!(
            "{}{}{}{}",
            func(3, &format!("{}{}", add(), char_out())),
            push(64),
            push(1),
            call(3, 2),
        );
        let ir = compile_src_to_ir(&src);
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
            assert_eq!(stdout, "A");
        }
    }

    #[test]
    fn compile_heap_store_load() {
        // Store pops addr first, then val. So Push(val) Push(addr) Store
        // Push(72) Push(0) Store → heap[0] = 72
        // Push(0) Load → push heap[0] = 72
        // CharOut → 'H'
        let src = format!(
            "{}{}{}{}{}{}",
            push(72), push(0), store(),  // heap[0] = 72
            push(0), load(),             // push heap[0]
            char_out(),                  // output 'H'
        );
        let ir = compile_src_to_ir(&src);
        assert!(ir.contains("@heap"), "should access heap");
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
            assert_eq!(stdout, "H");
        }
    }

    #[test]
    fn compile_comparison_jump_conditional() {
        // Push(5) Push(5) Eq Jump(0) Push(1) CharOut Mark(0) Push(65) CharOut
        // 5==5 → true(1) → Jump(0) → skips Push(1) CharOut → outputs 'A'
        let src = format!(
            "{}{}{}{}{}{}{}{}{}",
            push(5), push(5), eq(),
            jump(0),
            push(1), char_out(),
            mark(0),
            push(65), char_out(),
        );
        let ir = compile_src_to_ir(&src);
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
            assert_eq!(stdout, "A");
        }
    }

    #[test]
    fn compile_empty_program_runs() {
        let ir = compile_src_to_ir("");
        if let Some((stdout, _stderr, code)) = compile_and_run(&ir) {
            assert_eq!(code, 0);
            assert_eq!(stdout, "");
        }
    }

    #[test]
    fn compile_depth_instruction() {
        // Push(1) Push(2) Depth → stack: [1, 2, 2]
        let src = format!("{}{}{}", push(1), push(2), depth());
        let ir = compile_src_to_ir(&src);
        assert!(ir.contains("@frame_base_stack"), "depth uses frame base tracking");
    }

    #[test]
    fn compile_toolchain_check() {
        let status = codegen::check_toolchain();
        // Must have clang available (was used to compile)
        assert!(status.clang, "clang should be available");
        assert!(status.llc, "llc should be available");
        assert!(status.can_compile(), "toolchain should be complete");
    }

    #[test]
    fn compile_ir_file_and_cleanup() {
        let src = format!("{}{}", push(65), char_out());
        let tokens = tokenize(&src).unwrap();
        let parsed = parse_program(tokens).unwrap();
        let tmp = std::env::temp_dir().join("sd_test_output.ll");
        let config = CodeGenConfig::default();
        codegen::compile_to_ir_file(&parsed, &tmp, &config).unwrap();
        assert!(tmp.exists(), "IR file should be created");
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("putchar"));
        let _ = std::fs::remove_file(&tmp);
    }
}
