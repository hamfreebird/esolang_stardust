//! REPL 简写解析器
//!
//! 将可读的指令名映射为 Instruction 枚举值。
//! 支持：
//! - `Push(5) CharOut`  指令简写
//! - `"Hello"`          字符串字面量自动展开

use crate::stardust::{InstrMeta, Instruction};

#[derive(Debug, PartialEq)]
pub enum ParseReplError {
    UnknownInstruction(String),
    ExpectedArg,
    UnclosedParen,
    InvalidNumber(String),
    EmptyInput,
}

impl std::fmt::Display for ParseReplError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseReplError::UnknownInstruction(s) => write!(f, "unknown instruction '{}'", s),
            ParseReplError::ExpectedArg => write!(f, "expected '(' with argument"),
            ParseReplError::UnclosedParen => write!(f, "unclosed parenthesis"),
            ParseReplError::InvalidNumber(s) => write!(f, "invalid number '{}'", s),
            ParseReplError::EmptyInput => write!(f, "empty input"),
        }
    }
}

/// 解析一行简写指令
pub fn parse_shorthand(input: &str) -> Result<Vec<Instruction>, ParseReplError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Vec::new());
    }

    // 先展开字符串字面量
    let expanded = expand_strings(input);
    let mut instructions = Vec::new();
    let mut rest: &str = &expanded;

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let (inst, remaining) = parse_one(rest)?;
        instructions.push(inst);
        rest = remaining;
    }

    Ok(instructions)
}

/// 展开 "..." 字符串为 Push+CharOut 序列
fn expand_strings(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '"' {
            let start = i + 1;
            if let Some(end) = input[start..].find('"') {
                let s = &input[start..start + end];
                for ch in s.chars() {
                    result.push_str(&format!("Push({}) CharOut ", ch as i64));
                }
                i = start + end + 1;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// 解析一条指令
fn parse_one(input: &str) -> Result<(Instruction, &str), ParseReplError> {
    let meta = InstrMeta::default();

    // 提取指令名（到空格或 '(' 为止）
    let name_end = input
        .find(|c: char| c.is_whitespace() || c == '(')
        .unwrap_or(input.len());
    let name = &input[..name_end];
    let rest = &input[name_end..];

    if name.is_empty() {
        return Err(ParseReplError::EmptyInput);
    }

    match name {
        "Push" => {
            let (n, rest) = parse_one_arg(rest)?;
            Ok((Instruction::Push(n, meta), rest))
        }
        "Dup" => Ok((Instruction::Dup(meta), rest)),
        "Swap" => Ok((Instruction::Swap(meta), rest)),
        "Rotate" => Ok((Instruction::Rotate(meta), rest)),
        "Pop" => Ok((Instruction::Pop(meta), rest)),
        "Add" => Ok((Instruction::Add(meta), rest)),
        "Sub" => Ok((Instruction::Sub(meta), rest)),
        "Mul" => Ok((Instruction::Mul(meta), rest)),
        "Div" => Ok((Instruction::Div(meta), rest)),
        "Mod" => Ok((Instruction::Mod(meta), rest)),
        "Reverse" => Ok((Instruction::Reverse(meta), rest)),
        "CharOut" => Ok((Instruction::CharOut(meta), rest)),
        "CharIn" => Ok((Instruction::CharIn(meta), rest)),
        "NumOut" => Ok((Instruction::NumOut(meta), rest)),
        "NumIn" => Ok((Instruction::NumIn(meta), rest)),
        "Eq" => Ok((Instruction::Eq(meta), rest)),
        "Ne" => Ok((Instruction::Ne(meta), rest)),
        "Lt" => Ok((Instruction::Lt(meta), rest)),
        "Gt" => Ok((Instruction::Gt(meta), rest)),
        "Le" => Ok((Instruction::Le(meta), rest)),
        "Ge" => Ok((Instruction::Ge(meta), rest)),
        "And" => Ok((Instruction::And(meta), rest)),
        "Or" => Ok((Instruction::Or(meta), rest)),
        "Not" => Ok((Instruction::Not(meta), rest)),
        "Xor" => Ok((Instruction::Xor(meta), rest)),
        "Store" => Ok((Instruction::Store(meta), rest)),
        "Load" => Ok((Instruction::Load(meta), rest)),
        "ShiftL" => Ok((Instruction::ShiftL(meta), rest)),
        "ShiftR" => Ok((Instruction::ShiftR(meta), rest)),
        "Depth" => Ok((Instruction::Depth(meta), rest)),
        "DropN" => Ok((Instruction::DropN(meta), rest)),
        "Pick" => Ok((Instruction::Pick(meta), rest)),
        "Mark" => {
            let (n, rest) = parse_one_arg(rest)?;
            Ok((Instruction::Mark { name: n as usize, meta }, rest))
        }
        "Jump" => {
            let (n, rest) = parse_one_arg(rest)?;
            Ok((Instruction::Jump { name: n as usize, meta }, rest))
        }
        "UncondJump" => {
            let (n, rest) = parse_one_arg(rest)?;
            Ok((Instruction::UnconditionalJump { name: n as usize, meta }, rest))
        }
        "Call" => {
            let ((name, argc), rest) = parse_two_args(rest)?;
            Ok((Instruction::Call { name: name as usize, argc: argc as usize, meta }, rest))
        }
        "Breakpoint" => Ok((Instruction::Breakpoint(meta), rest)),
        "DumpStack" => Ok((Instruction::DumpStack(meta), rest)),
        "DumpState" => Ok((Instruction::DumpState(meta), rest)),
        _ => Err(ParseReplError::UnknownInstruction(name.to_string())),
    }
}

fn parse_one_arg(input: &str) -> Result<(i64, &str), ParseReplError> {
    let input = input.trim_start();
    if !input.starts_with('(') {
        return Err(ParseReplError::ExpectedArg);
    }
    let close = input.find(')').ok_or(ParseReplError::UnclosedParen)?;
    let inner = &input[1..close].trim();
    let n: i64 = inner
        .parse()
        .map_err(|_| ParseReplError::InvalidNumber(inner.to_string()))?;
    Ok((n, &input[close + 1..]))
}

fn parse_two_args(input: &str) -> Result<((i64, i64), &str), ParseReplError> {
    let input = input.trim_start();
    if !input.starts_with('(') {
        return Err(ParseReplError::ExpectedArg);
    }
    let close = input.find(')').ok_or(ParseReplError::UnclosedParen)?;
    let inner = &input[1..close];
    let mut parts = inner.split(',');
    let a: i64 = parts
        .next()
        .unwrap_or("0")
        .trim()
        .parse()
        .map_err(|_| ParseReplError::InvalidNumber(inner.to_string()))?;
    let b: i64 = parts
        .next()
        .unwrap_or("0")
        .trim()
        .parse()
        .map_err(|_| ParseReplError::InvalidNumber(inner.to_string()))?;
    Ok(((a, b), &input[close + 1..]))
}

// ── 测试 ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> InstrMeta {
        InstrMeta::default()
    }

    #[test]
    fn parse_single_push() {
        let r = parse_shorthand("Push(65)").unwrap();
        assert_eq!(r, vec![Instruction::Push(65, meta())]);
    }

    #[test]
    fn parse_multiple() {
        let r = parse_shorthand("Push(72) CharOut").unwrap();
        assert_eq!(
            r,
            vec![Instruction::Push(72, meta()), Instruction::CharOut(meta())]
        );
    }

    #[test]
    fn parse_arithmetic() {
        let r = parse_shorthand("Push(3) Push(5) Add").unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], Instruction::Push(3, meta()));
        assert_eq!(r[1], Instruction::Push(5, meta()));
        assert_eq!(r[2], Instruction::Add(meta()));
    }

    #[test]
    fn parse_all_comparisons() {
        for (name, expected) in [
            ("Eq", Instruction::Eq(meta())),
            ("Ne", Instruction::Ne(meta())),
            ("Lt", Instruction::Lt(meta())),
            ("Gt", Instruction::Gt(meta())),
            ("Le", Instruction::Le(meta())),
            ("Ge", Instruction::Ge(meta())),
        ] {
            let r = parse_shorthand(name).unwrap();
            assert_eq!(r, vec![expected], "failed for {}", name);
        }
    }

    #[test]
    fn parse_all_logic() {
        for name in ["And", "Or", "Not", "Xor"] {
            let r = parse_shorthand(name).unwrap();
            assert_eq!(r.len(), 1, "failed for {}", name);
        }
    }

    #[test]
    fn parse_mark_and_jump() {
        let r = parse_shorthand("Mark(0) Jump(0)").unwrap();
        assert_eq!(r.len(), 2);
        assert!(matches!(r[0], Instruction::Mark { name: 0, .. }));
        assert!(matches!(r[1], Instruction::Jump { name: 0, .. }));
    }

    #[test]
    fn parse_uncond_jump() {
        let r = parse_shorthand("UncondJump(3)").unwrap();
        assert!(matches!(r[0], Instruction::UnconditionalJump { name: 3, .. }));
    }

    #[test]
    fn parse_call() {
        let r = parse_shorthand("Call(1, 2)").unwrap();
        assert!(matches!(r[0], Instruction::Call { name: 1, argc: 2, .. }));
    }

    #[test]
    fn parse_string_literal() {
        let r = parse_shorthand(r#""AB""#).unwrap();
        // A=65, B=66 → Push(65) CharOut Push(66) CharOut
        assert_eq!(r.len(), 4);
        assert_eq!(r[0], Instruction::Push(65, meta()));
        assert_eq!(r[1], Instruction::CharOut(meta()));
        assert_eq!(r[2], Instruction::Push(66, meta()));
        assert_eq!(r[3], Instruction::CharOut(meta()));
    }

    #[test]
    fn parse_heap_ops() {
        for name in ["Store", "Load"] {
            let r = parse_shorthand(name).unwrap();
            assert_eq!(r.len(), 1, "failed for {}", name);
        }
    }

    #[test]
    fn parse_stack_ext() {
        for name in ["ShiftL", "ShiftR", "Depth", "DropN", "Pick"] {
            let r = parse_shorthand(name).unwrap();
            assert_eq!(r.len(), 1, "failed for {}", name);
        }
    }

    #[test]
    fn parse_empty() {
        let r = parse_shorthand("").unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn parse_dbg_instructions() {
        for name in ["Breakpoint", "DumpStack", "DumpState"] {
            let r = parse_shorthand(name).unwrap();
            assert_eq!(r.len(), 1, "failed for {}", name);
        }
    }

    #[test]
    fn unknown_instruction() {
        let r = parse_shorthand("Foobar");
        assert!(r.is_err());
    }
}
