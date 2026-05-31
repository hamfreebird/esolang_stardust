//! Stardust IR 优化器
//!
//! 在生成 LLVM IR 之前对 Stardust 指令序列进行优化：
//! 1. 常量折叠 — 编译期已知值的算术运算直接求值
//! 2. 死代码消除 — 移除无意义的 Push/Pop 对
//! 3. 窥孔优化 — 相邻指令的模式化简
//!
//! 所有优化保持栈语义等价（stack-effect-identical）。

use crate::stardust::{InstrMeta, Instruction};

/// 优化一段指令序列，返回优化后的等价序列。
pub fn optimize(instructions: &[Instruction]) -> Vec<Instruction> {
    let mut instrs = instructions.to_vec();
    // 反复应用优化直到不动点（通常 2-3 轮就收敛）
    loop {
        let before = instrs.len();
        instrs = constant_fold(&instrs);
        instrs = dead_pop_elim(&instrs);
        instrs = peephole(&instrs);
        if instrs.len() == before {
            break;
        }
    }
    instrs
}

/// 优化整个函数表（入口和所有函数体），返回新的函数表。
pub fn optimize_functions(
    main_instructions: &[Instruction],
    functions: &std::collections::HashMap<usize, Vec<Instruction>>,
) -> (
    Vec<Instruction>,
    std::collections::HashMap<usize, Vec<Instruction>>,
) {
    let main_opt = optimize(main_instructions);
    let mut funcs_opt = std::collections::HashMap::new();
    for (name, body) in functions {
        funcs_opt.insert(*name, optimize(body));
    }
    (main_opt, funcs_opt)
}

// ── 1. 常量折叠 ──────────────────────────────────────────────

fn constant_fold(instructions: &[Instruction]) -> Vec<Instruction> {
    let _meta = InstrMeta::default();
    let mut result: Vec<Instruction> = Vec::new();
    let mut i = 0;

    while i < instructions.len() {
        let folded = try_fold_window(instructions, i);
        match folded {
            Some((new_instrs, skip)) => {
                result.extend(new_instrs);
                i += skip;
            }
            None => {
                result.push(instructions[i].clone());
                i += 1;
            }
        }
    }
    result
}

/// 尝试对从位置 `pos` 开始的指令窗口做折叠。
fn try_fold_window(instrs: &[Instruction], pos: usize) -> Option<(Vec<Instruction>, usize)> {
    let meta = InstrMeta::default();

    // 模式: Push(a) Push(b) Add  →  Push(a+b)   (需要 a,b 都在 i64 安全范围内)
    if pos + 2 < instrs.len() {
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::Add(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            if let Some(r) = a.checked_add(*b) {
                return Some((vec![Instruction::Push(r, meta.clone())], 3));
            }
        }

        // Push(a) Push(b) Sub  →  Push(a-b)
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::Sub(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            if let Some(r) = a.checked_sub(*b) {
                return Some((vec![Instruction::Push(r, meta.clone())], 3));
            }
        }

        // Push(a) Push(b) Mul  →  Push(a*b)
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::Mul(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            if let Some(r) = a.checked_mul(*b) {
                return Some((vec![Instruction::Push(r, meta.clone())], 3));
            }
        }

        // Push(a) Push(b) Div  →  Push(a/b)  (b != 0)
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::Div(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            if *b != 0 {
                return Some((vec![Instruction::Push(a / b, meta.clone())], 3));
            }
        }

        // Push(a) Push(b) Mod  →  Push(a%b)  (b != 0)
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::Mod(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            if *b != 0 {
                return Some((vec![Instruction::Push(a % b, meta.clone())], 3));
            }
        }

        // Push(a) Push(b) Eq → Push(1 or 0)
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::Eq(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            return Some((
                vec![Instruction::Push(if a == b { 1 } else { 0 }, meta.clone())],
                3,
            ));
        }

        // Push(a) Push(b) 比较运算折叠
        if pos + 2 < instrs.len() {
            let cmp_result = match (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2]) {
                (Instruction::Push(a, _), Instruction::Push(b, _), Instruction::Ne(_)) => {
                    Some(if a != b { 1 } else { 0 })
                }
                (Instruction::Push(a, _), Instruction::Push(b, _), Instruction::Lt(_)) => {
                    Some(if a < b { 1 } else { 0 })
                }
                (Instruction::Push(a, _), Instruction::Push(b, _), Instruction::Gt(_)) => {
                    Some(if a > b { 1 } else { 0 })
                }
                (Instruction::Push(a, _), Instruction::Push(b, _), Instruction::Le(_)) => {
                    Some(if a <= b { 1 } else { 0 })
                }
                (Instruction::Push(a, _), Instruction::Push(b, _), Instruction::Ge(_)) => {
                    Some(if a >= b { 1 } else { 0 })
                }
                _ => None,
            };
            if let Some(r) = cmp_result {
                return Some((vec![Instruction::Push(r, meta.clone())], 3));
            }
        }

        // Push(a) Push(b) And → Push(1 or 0)
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::And(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            return Some((
                vec![Instruction::Push(
                    if *a != 0 && *b != 0 { 1 } else { 0 },
                    meta.clone(),
                )],
                3,
            ));
        }

        // Push(a) Push(b) Or → Push(1 or 0)
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::Or(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            return Some((
                vec![Instruction::Push(
                    if *a != 0 || *b != 0 { 1 } else { 0 },
                    meta.clone(),
                )],
                3,
            ));
        }

        // Push(a) Push(b) Xor → Push(1 or 0)
        if let (
            Instruction::Push(a, _),
            Instruction::Push(b, _),
            Instruction::Xor(_),
        ) = (&instrs[pos], &instrs[pos + 1], &instrs[pos + 2])
        {
            return Some((
                vec![Instruction::Push(
                    if (*a != 0) ^ (*b != 0) { 1 } else { 0 },
                    meta.clone(),
                )],
                3,
            ));
        }

    }

    // ── 2 指令窗口模式（在 pos+2 检查之外）──────────────────

    // Push(a) Not → Push(1 or 0)
    if pos + 1 < instrs.len() {
        if let (Instruction::Push(a, _), Instruction::Not(_)) =
            (&instrs[pos], &instrs[pos + 1])
        {
            return Some((
                vec![Instruction::Push(if *a == 0 { 1 } else { 0 }, meta.clone())],
                2,
            ));
        }
    }

    // 模式: Push(0) Jump(name)  →  UncondJump(name)   (Jump 知道栈顶是 0，永不跳转，相当于 NOP)
    // 模式: Push(nonzero) Jump(name)  →  UncondJump(name)   (条件恒真 → 无条件)

    // 模式: Dup Pop  →  NOP (栈不变)
    // 由 dead_pop_elim 处理

    None
}

// ── 2. 死代码消除 ────────────────────────────────────────────

/// 消除无操作序列，如 Push(x) Pop、Dup Pop 等
fn dead_pop_elim(instructions: &[Instruction]) -> Vec<Instruction> {
    let mut result: Vec<Instruction> = Vec::new();
    let mut i = 0;

    while i < instructions.len() {
        // Push(v) Pop → nothing (压入又立即丢弃)
        if i + 1 < instructions.len() {
            if let (Instruction::Push(_, _), Instruction::Pop(_)) =
                (&instructions[i], &instructions[i + 1])
            {
                i += 2;
                continue;
            }
            // Dup Pop → nothing (复制后丢弃，等价于没发生)
            if let (Instruction::Dup(_), Instruction::Pop(_)) =
                (&instructions[i], &instructions[i + 1])
            {
                i += 2;
                continue;
            }
        }

        result.push(instructions[i].clone());
        i += 1;
    }

    // 多轮扫描才能消除连锁效应:
    // Push(1) Push(2) Pop → 第一轮消去 Push(2) Pop, 第二轮消去 Push(1) Pop
    if result.len() != instructions.len() {
        return dead_pop_elim(&result);
    }

    result
}

// ── 3. 窥孔优化 ──────────────────────────────────────────────

fn peephole(instructions: &[Instruction]) -> Vec<Instruction> {
    let meta = InstrMeta::default();
    let mut result: Vec<Instruction> = Vec::new();
    let mut i = 0;

    while i < instructions.len() {
        let applied = {
            // Push(a) Push(b) Swap → Push(b) Push(a)
            if i + 2 < instructions.len() {
                if let (
                    Instruction::Push(a, _),
                    Instruction::Push(b, _),
                    Instruction::Swap(_),
                ) = (&instructions[i], &instructions[i + 1], &instructions[i + 2])
                {
                    result.push(Instruction::Push(*b, meta.clone()));
                    result.push(Instruction::Push(*a, meta.clone()));
                    i += 3;
                    continue;
                }
            }

            // Push(0) Add → nothing (加 0 无影响)
            if i + 1 < instructions.len() {
                if let (Instruction::Push(0, _), Instruction::Add(_)) =
                    (&instructions[i], &instructions[i + 1])
                {
                    i += 2;
                    continue;
                }
                // Push(0) Sub → nothing (减 0 无影响)
                if let (Instruction::Push(0, _), Instruction::Sub(_)) =
                    (&instructions[i], &instructions[i + 1])
                {
                    i += 2;
                    continue;
                }
                // Push(1) Mul → nothing (乘 1 无影响)
                if let (Instruction::Push(1, _), Instruction::Mul(_)) =
                    (&instructions[i], &instructions[i + 1])
                {
                    i += 2;
                    continue;
                }
                // Push(0) Push(x) Store Pop → 等价于 Pop (store 需要 addr val, 先简化)
                // 这个太复杂，不在此处处理
            }

            // Reverse Reverse → nothing
            if i + 1 < instructions.len() {
                if let (Instruction::Reverse(_), Instruction::Reverse(_)) =
                    (&instructions[i], &instructions[i + 1])
                {
                    i += 2;
                    continue;
                }
            }

            // ShiftL ShiftR → nothing (循环左移+右移恢复原状)
            if i + 1 < instructions.len() {
                if let (Instruction::ShiftL(_), Instruction::ShiftR(_)) =
                    (&instructions[i], &instructions[i + 1])
                {
                    i += 2;
                    continue;
                }
                if let (Instruction::ShiftR(_), Instruction::ShiftL(_)) =
                    (&instructions[i], &instructions[i + 1])
                {
                    i += 2;
                    continue;
                }
            }

            false // 没有匹配的模式
        };

        if !applied {
            result.push(instructions[i].clone());
            i += 1;
        }
    }

    result
}

// ── 测试 ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stardust::InstrMeta;

    fn meta() -> InstrMeta {
        InstrMeta::default()
    }

    fn push(v: i64) -> Instruction {
        Instruction::Push(v, meta())
    }

    #[test]
    fn fold_add_constants() {
        let input = vec![push(5), push(3), Instruction::Add(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(8));
    }

    #[test]
    fn fold_sub_constants() {
        let input = vec![push(10), push(3), Instruction::Sub(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(7));
    }

    #[test]
    fn fold_mul_constants() {
        let input = vec![push(6), push(7), Instruction::Mul(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(42));
    }

    #[test]
    fn fold_div_constants() {
        let input = vec![push(10), push(2), Instruction::Div(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(5));
    }

    #[test]
    fn fold_mod_constants() {
        let input = vec![push(10), push(3), Instruction::Mod(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    #[test]
    fn fold_eq_true() {
        let input = vec![push(5), push(5), Instruction::Eq(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    #[test]
    fn fold_eq_false() {
        let input = vec![push(5), push(3), Instruction::Eq(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(0));
    }

    #[test]
    fn fold_cmp_lt() {
        let input = vec![push(3), push(5), Instruction::Lt(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    #[test]
    fn fold_cmp_gt() {
        let input = vec![push(3), push(5), Instruction::Gt(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(0));
    }

    #[test]
    fn fold_logic_and() {
        let input = vec![push(1), push(1), Instruction::And(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    #[test]
    fn fold_logic_or() {
        let input = vec![push(0), push(0), Instruction::Or(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(0));
    }

    #[test]
    fn fold_logic_xor() {
        let input = vec![push(1), push(1), Instruction::Xor(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(0));
    }

    #[test]
    fn fold_not() {
        let input = vec![push(0), Instruction::Not(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    #[test]
    fn dead_push_pop() {
        let input = vec![push(42), Instruction::Pop(meta())];
        let result = optimize(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn dead_dup_pop() {
        let input = vec![Instruction::Dup(meta()), Instruction::Pop(meta())];
        let result = optimize(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn peephole_swap_push() {
        let input = vec![push(1), push(2), Instruction::Swap(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], push(2));
        assert_eq!(result[1], push(1));
    }

    #[test]
    fn no_fold_div_by_zero() {
        let input = vec![push(5), push(0), Instruction::Div(meta())];
        let result = optimize(&input);
        // 除零不能折叠，保留原样
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn no_fold_with_unknown() {
        // Push(3) Dup Add → 应保留（Add 的操作数不是两个常量）
        let input = vec![push(3), Instruction::Dup(meta()), Instruction::Add(meta())];
        let result = optimize(&input);
        // Dup 不能折叠为常量，所以不能消去
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn chain_fold() {
        // Push(2) Push(3) Mul Push(4) Add → Push(10)
        let input = vec![
            push(2),
            push(3),
            Instruction::Mul(meta()),
            push(4),
            Instruction::Add(meta()),
        ];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(10));
    }

    #[test]
    fn preserve_jump_targets() {
        // 不应改变控制流结构
        let input = vec![
            push(5),
            push(3),
            Instruction::Add(meta()), // 可折叠
            Instruction::Mark {
                name: 0,
                meta: meta(),
            },
            Instruction::Jump {
                name: 0,
                meta: meta(),
            },
        ];
        let result = optimize(&input);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], push(8));
        assert!(matches!(result[1], Instruction::Mark { name: 0, .. }));
        assert!(matches!(result[2], Instruction::Jump { name: 0, .. }));
    }

    // ═══════════ 补充折叠测试 ═══════════

    #[test]
    fn fold_ne_true() {
        let input = vec![push(5), push(3), Instruction::Ne(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    #[test]
    fn fold_le_true() {
        let input = vec![push(3), push(5), Instruction::Le(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    #[test]
    fn fold_ge_true() {
        let input = vec![push(5), push(5), Instruction::Ge(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    // ═══════════ Peephole: 算术恒等式消除 ═══════════

    #[test]
    fn peephole_push_zero_add_removed() {
        // Push(0) Add → nothing (x + 0 = x)
        let input = vec![
            push(5),
            push(0),
            Instruction::Add(meta()),
            push(0),
            Instruction::Add(meta()),
        ];
        let result = optimize(&input);
        // After folding: Push(5) Push(0) Add → Push(5), then Push(0) Add → Push(5)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(5));
    }

    #[test]
    fn peephole_push_zero_sub_removed() {
        // Push(0) Sub → nothing (x - 0 = x)
        let input = vec![push(42), push(0), Instruction::Sub(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(42));
    }

    #[test]
    fn peephole_push_one_mul_removed() {
        // Push(1) Mul → nothing (x * 1 = x)
        let input = vec![push(99), push(1), Instruction::Mul(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(99));
    }

    // ═══════════ 死代码消除 — 连锁效应 ═══════════

    #[test]
    fn dead_push_push_pop_chain() {
        // Push(1) Push(2) Pop → Push(1)
        let input = vec![push(1), push(2), Instruction::Pop(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    #[test]
    fn dead_dup_pop_at_end() {
        // Push(1) Dup Pop → Push(1)
        let input = vec![push(1), Instruction::Dup(meta()), Instruction::Pop(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }

    // ═══════════ 大型常量折叠 ═══════════

    #[test]
    fn fold_large_values() {
        // Push(1000) Push(2000) Sub → Push(-1000)
        let input = vec![push(1000), push(2000), Instruction::Sub(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(-1000));
    }

    #[test]
    fn fold_mul_by_zero() {
        // Push(0) Push(99999) Mul → Push(0)
        let input = vec![push(0), push(99999), Instruction::Mul(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(0));
    }

    #[test]
    fn fold_nested_chain() {
        // Push(2) Push(3) Mul Push(4) Push(5) Add Mul → Push(6) Push(9) Mul → Push(54)
        let input = vec![
            push(2),
            push(3),
            Instruction::Mul(meta()),
            push(4),
            push(5),
            Instruction::Add(meta()),
            Instruction::Mul(meta()),
        ];
        let result = optimize(&input);
        // First round: fold 2*3→6, 4+5→9 → [Push(6), Push(9), Mul]
        // Second round: fold 6*9→54 → [Push(54)]
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(54));
    }

    // ═══════════ 优化器幂等性 ═══════════

    #[test]
    fn optimize_is_idempotent() {
        let input = vec![
            push(1),
            push(2),
            Instruction::Add(meta()),
            push(0),
            Instruction::Mul(meta()),
            Instruction::Pop(meta()),
        ];
        let pass1 = optimize(&input);
        let pass2 = optimize(&pass1);
        assert_eq!(pass1, pass2, "optimizer should reach fixed point in one pass");
    }

    // ═══════════ 不优化场景 ═══════════

    #[test]
    fn no_fold_mod_by_zero() {
        let input = vec![push(10), push(0), Instruction::Mod(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 3, "mod by zero should not be folded");
    }

    #[test]
    fn no_fold_with_control_flow_mixed() {
        // Mark intervening should prevent folding
        let input = vec![
            push(1),
            push(2),
            Instruction::Mark { name: 0, meta: meta() },
            Instruction::Add(meta()),
        ];
        let result = optimize(&input);
        // Push(1), Mark(0), Push(2), Add → Mark breaks the Push-Push pattern
        // Cannot fold because Push(2) is separated from Push(1) by Mark
        assert!(result.len() >= 3, "mark should prevent cross-block folding");
    }

    #[test]
    fn fold_with_same_value_push_add() {
        // Push(7) Push(7) Add → Push(14)
        let input = vec![push(7), push(7), Instruction::Add(meta())];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(14));
    }

    #[test]
    fn dead_multiple_pops() {
        // Push(1) Push(2) Push(3) Pop Pop → Push(1)
        let input = vec![
            push(1),
            push(2),
            push(3),
            Instruction::Pop(meta()),
            Instruction::Pop(meta()),
        ];
        let result = optimize(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], push(1));
    }
}
