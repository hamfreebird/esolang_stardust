use crate::stardust::{
    ErrorKind, Instruction, ParseResult, Parser, SourceSpan, StardustError, Token, TokenType,
};
use std::collections::HashMap;

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            instructions: Vec::new(),
            marks: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn span(&self) -> SourceSpan {
        if let Some(tok) = self.current() {
            SourceSpan {
                line: tok.line,
                column: tok.column,
            }
        } else {
            SourceSpan { line: 0, column: 0 } // EOF
        }
    }

    fn error(&self, kind: ErrorKind) -> StardustError {
        StardustError::new(kind, Some(self.span()))
    }

    /// 解析整个程序，返回 ParseResult
    pub fn parse(mut self) -> Result<ParseResult, StardustError> {
        while self.pos < self.tokens.len() {
            self.parse_top_level()?;
        }
        self.resolve_main_marks()?;
        Ok(ParseResult {
            main_instructions: self.instructions,
            main_marks: self.marks,
            functions: self.functions,
        })
    }

    fn parse_top_level(&mut self) -> Result<(), StardustError> {
        let token = self
            .current()
            .ok_or_else(|| self.error(ErrorKind::UnexpectedEof))?
            .clone();
        match token.token_type {
            TokenType::Colon => {
                self.parse_colon_at_top_level()?;
            }
            _ => {
                let inst = self.parse_simple_instruction(&token)?;
                self.instructions.push(inst);
                self.advance();
            }
        }
        Ok(())
    }

    /// 处理以 Colon 开头的结构：可能是函数声明或调用
    fn parse_colon_at_top_level(&mut self) -> Result<(), StardustError> {
        let name = self.current().unwrap().spaces;
        self.advance(); // 跳过第一个 colon

        // 检查调用模式：下一个 token 是 Semicolon
        if self.is_call_pattern() {
            let argc = self.tokens[self.pos].spaces;  // Semicolon 的 spaces = n2 = 参数个数
            self.instructions.push(Instruction::Call { name, argc });
            self.pos += 1;  // 跳过 Semicolon
            return Ok(());
        }

        // 否则为函数声明
        let func_name = name;
        let mut body = Vec::new();
        let mut found_end = false;

        // 收集函数体，直到遇到匹配的结束 Colon
        while self.pos < self.tokens.len() {
            let tok = self.current().unwrap();
            if tok.token_type == TokenType::Colon && tok.spaces == func_name {
                self.advance(); // 消耗结束 Colon
                found_end = true;
                break;
            } else {
                let inst = self.parse_instruction_in_function()?;
                body.push(inst);
            }
        }

        if !found_end {
            return Err(self.error(ErrorKind::UnclosedFunction { name: func_name }));
        }

        if self.functions.insert(func_name, body).is_some() {
            return Err(self.error(ErrorKind::DuplicateFunction { name: func_name }));
        }
        Ok(())
    }

    /// 检查当前位置开始是否构成调用模式：Colon (任意空格) + Semicolon
    fn is_call_pattern(&self) -> bool {
        if self.pos >= self.tokens.len() {
            return false;
        }
        self.tokens[self.pos].token_type == TokenType::Semicolon
    }

    /// 在函数体内解析一条指令
    /// 注意：函数体内不允许出现函数声明，只允许普通指令或调用
    fn parse_instruction_in_function(&mut self) -> Result<Instruction, StardustError> {
        let token = self
            .current()
            .ok_or_else(|| self.error(ErrorKind::UnexpectedEof))?
            .clone();
        match token.token_type {
            TokenType::Colon => {
                let name = token.spaces;
                self.advance(); // 消耗 colon
                if self.pos >= self.tokens.len() {
                    return Err(self.error(ErrorKind::IncompleteFunctionCall));
                }
                let next = &self.tokens[self.pos];
                if next.token_type != TokenType::Semicolon {
                    return Err(self.error(ErrorKind::ExpectedSemicolonAfterCall));
                }
                let argc = next.spaces;        // Semicolon 的 spaces = 参数个数
                self.advance();                // 消耗 Semicolon
                Ok(Instruction::Call { name, argc })
            }
            _ => {
                let inst = self.parse_simple_instruction(&token)?;
                self.advance();
                Ok(inst)
            }
        }
    }

    /// 解析非 Colon 开头的简单指令
    fn parse_simple_instruction(&self, token: &Token) -> Result<Instruction, StardustError> {
        let spaces = token.spaces;
        let kind = match token.token_type {
            TokenType::Plus => match spaces {
                0 => ErrorKind::InvalidSpacesForPlus,
                1 => return Ok(Instruction::Dup),
                2 => return Ok(Instruction::Swap),
                3 => return Ok(Instruction::Rotate),
                4 => return Ok(Instruction::Pop),
                n => return Ok(Instruction::Push((n - 5) as i64)),
            },
            TokenType::Star => match spaces {
                0 => return Ok(Instruction::Add),
                1 => return Ok(Instruction::Sub),
                2 => return Ok(Instruction::Mul),
                3 => return Ok(Instruction::Div),
                4 => return Ok(Instruction::Mod),
                5 => return Ok(Instruction::Reverse),
                _ => ErrorKind::InvalidSpacesForStar { spaces },
            },
            TokenType::Dot => match spaces {
                0 => return Ok(Instruction::NumOut),
                1 => return Ok(Instruction::NumIn),
                _ => ErrorKind::InvalidSpacesForDot { spaces },
            },
            TokenType::Comma => match spaces {
                0 => return Ok(Instruction::CharOut),
                1 => return Ok(Instruction::CharIn),
                _ => ErrorKind::InvalidSpacesForComma { spaces },
            },
            TokenType::Backtick => return Ok(Instruction::Mark {
                name: spaces,
                span: SourceSpan {
                    line: token.line,
                    column: token.column,
                },
            }),
            TokenType::Quote => return Ok(Instruction::Jump { name: spaces }),
            TokenType::Tilde => return Ok(Instruction::UnconditionalJump { name: spaces }),
            _ => ErrorKind::UnexpectedToken {
                expected: "instruction symbol".to_string(),
                found: format!("{:?}", token.token_type),
            },
        };
        Err(self.error(kind))
    }

    /// 在主指令序列中收集 Mark 位置，并检查重复
    fn resolve_main_marks(&mut self) -> Result<(), StardustError> {
        for (idx, inst) in self.instructions.iter().enumerate() {
            if let Instruction::Mark { name, span } = inst {
                if self.marks.insert(*name, idx).is_some() {
                    return Err(StardustError::new(
                        ErrorKind::DuplicateMark { name: *name },
                        Some(span.clone()),
                    ));
                }
            }
        }
        Ok(())
    }
}

pub fn parse_program(tokens: Vec<Token>) -> Result<ParseResult, StardustError> {
    Parser::new(tokens).parse()
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stardust::TokenType;

    fn tok(spaces: usize, tt: TokenType) -> Token {
        Token { spaces, token_type: tt, line: 1, column: 1, byte_pos: 0 }
    }

    fn tok_at(spaces: usize, tt: TokenType, line: usize, col: usize) -> Token {
        Token { spaces, token_type: tt, line, column: col, byte_pos: 0 }
    }

    fn colon(spaces: usize) -> Token { tok(spaces, TokenType::Colon) }
    fn semicolon(spaces: usize) -> Token { tok(spaces, TokenType::Semicolon) }
    fn plus(spaces: usize) -> Token { tok(spaces, TokenType::Plus) }
    fn star(spaces: usize) -> Token { tok(spaces, TokenType::Star) }
    fn dot(spaces: usize) -> Token { tok(spaces, TokenType::Dot) }
    fn comma(spaces: usize) -> Token { tok(spaces, TokenType::Comma) }
    fn backtick(spaces: usize) -> Token { tok(spaces, TokenType::Backtick) }
    fn quote(spaces: usize) -> Token { tok(spaces, TokenType::Quote) }
    fn tilde(spaces: usize) -> Token { tok(spaces, TokenType::Tilde) }

    // ═══════════ 1. Push / 栈操作 (符号 +) ═══════════

    #[test]
    fn push_5_spaces_is_0() {
        // 5空格+ → Push(5-5) = Push(0)
        let result = parse_program(vec![plus(5)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Push(0)]);
    }

    #[test]
    fn push_6_spaces_is_1() {
        let result = parse_program(vec![plus(6)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Push(1)]);
    }

    #[test]
    fn push_10_spaces_is_5() {
        let result = parse_program(vec![plus(10)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Push(5)]);
    }

    #[test]
    fn push_large_value() {
        // Push(100) → 105 空格
        let result = parse_program(vec![plus(105)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Push(100)]);
    }

    #[test]
    fn dup_1_space_plus() {
        let result = parse_program(vec![plus(1)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Dup]);
    }

    #[test]
    fn swap_2_spaces_plus() {
        let result = parse_program(vec![plus(2)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Swap]);
    }

    #[test]
    fn rotate_3_spaces_plus() {
        let result = parse_program(vec![plus(3)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Rotate]);
    }

    #[test]
    fn pop_4_spaces_plus() {
        let result = parse_program(vec![plus(4)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Pop]);
    }

    #[test]
    fn invalid_spaces_for_plus_0_spaces() {
        let result = parse_program(vec![plus(0)]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidSpacesForPlus);
    }

    // ═══════════ 2. 算术运算 (符号 *) ═══════════

    #[test]
    fn add_0_star() {
        let result = parse_program(vec![star(0)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Add]);
    }

    #[test]
    fn sub_1_star() {
        let result = parse_program(vec![star(1)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Sub]);
    }

    #[test]
    fn mul_2_star() {
        let result = parse_program(vec![star(2)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Mul]);
    }

    #[test]
    fn div_3_star() {
        let result = parse_program(vec![star(3)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Div]);
    }

    #[test]
    fn mod_4_star() {
        let result = parse_program(vec![star(4)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Mod]);
    }

    #[test]
    fn reverse_5_star() {
        let result = parse_program(vec![star(5)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Reverse]);
    }

    #[test]
    fn invalid_spaces_for_star_6() {
        let result = parse_program(vec![star(6)]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidSpacesForStar { spaces: 6 });
    }

    // ═══════════ 3. I/O 指令 (符号 . 和 ,) ═══════════

    #[test]
    fn num_out_0_dot() {
        let result = parse_program(vec![dot(0)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::NumOut]);
    }

    #[test]
    fn num_in_1_dot() {
        let result = parse_program(vec![dot(1)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::NumIn]);
    }

    #[test]
    fn invalid_spaces_for_dot_2() {
        let result = parse_program(vec![dot(2)]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidSpacesForDot { spaces: 2 });
    }

    #[test]
    fn char_out_0_comma() {
        let result = parse_program(vec![comma(0)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::CharOut]);
    }

    #[test]
    fn char_in_1_comma() {
        let result = parse_program(vec![comma(1)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::CharIn]);
    }

    #[test]
    fn invalid_spaces_for_comma_2() {
        let result = parse_program(vec![comma(2)]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidSpacesForComma { spaces: 2 });
    }

    // ═══════════ 4. 控制流 (符号 ` ' ~) ═══════════

    #[test]
    fn mark_with_name_0() {
        let result = parse_program(vec![backtick(0)]).unwrap();
        assert_eq!(result.main_marks.len(), 1);
        assert!(result.main_marks.contains_key(&0));
        match &result.main_instructions[0] {
            Instruction::Mark { name, .. } => assert_eq!(*name, 0),
            _ => panic!("expected Mark"),
        }
    }

    #[test]
    fn mark_with_name_5() {
        let result = parse_program(vec![backtick(5)]).unwrap();
        assert_eq!(result.main_marks.len(), 1);
        match &result.main_instructions[0] {
            Instruction::Mark { name, .. } => assert_eq!(*name, 5),
            _ => panic!("expected Mark"),
        }
    }

    #[test]
    fn jump_with_name_3() {
        let result = parse_program(vec![quote(3)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::Jump { name: 3 }]);
    }

    #[test]
    fn unconditional_jump() {
        let result = parse_program(vec![tilde(7)]).unwrap();
        assert_eq!(result.main_instructions, vec![Instruction::UnconditionalJump { name: 7 }]);
    }

    // ═══════════ 5. 重复标志错误 ═══════════

    #[test]
    fn duplicate_mark_error() {
        let result = parse_program(vec![backtick(1), backtick(1)]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::DuplicateMark { name: 1 });
    }

    #[test]
    fn multiple_different_marks() {
        let result = parse_program(vec![backtick(1), backtick(2), backtick(3)]).unwrap();
        assert_eq!(result.main_marks.len(), 3);
    }

    // ═══════════ 6. 函数声明 ═══════════

    #[test]
    fn simple_function_declaration() {
        // (1): Push(0) CharOut (1):
        let tokens = vec![
            colon(1),
            plus(5),    // Push(0)
            comma(0),   // CharOut
            colon(1),   // 结束标志
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.functions.len(), 1);
        assert!(result.functions.contains_key(&1));
        let body = &result.functions[&1];
        assert_eq!(body.len(), 2);
        assert_eq!(body[0], Instruction::Push(0));
        assert_eq!(body[1], Instruction::CharOut);
        assert!(result.main_instructions.is_empty());
    }

    #[test]
    fn function_with_mark_and_jump() {
        // (2): Push(1) 0` 0' Push(0) CharOut (2):
        let tokens = vec![
            colon(2),
            plus(6),     // Push(1)
            backtick(0), // Mark(0)
            quote(0),    // Jump(0) — loops until stack empty
            plus(5),     // Push(0)
            comma(0),    // CharOut
            colon(2),
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.functions.len(), 1);
        assert!(result.functions.contains_key(&2));
        assert_eq!(result.functions[&2].len(), 5);
    }

    #[test]
    fn duplicate_function_error() {
        let tokens = vec![
            colon(1), plus(5), colon(1),
            colon(1), plus(6), colon(1),
        ];
        let result = parse_program(tokens);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::DuplicateFunction { name: 1 });
    }

    #[test]
    fn unclosed_function_error() {
        // 函数声明没有匹配的结束 colon
        let tokens = vec![
            colon(1),
            plus(5),
            // 缺少 colon(1) 结束
        ];
        let result = parse_program(tokens);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::UnclosedFunction { name: 1 });
    }

    #[test]
    fn function_with_nested_call_is_allowed_by_parser() {
        // Parser 允许函数体内出现调用语法（Colon + Semicolon）
        // VM 运行时会拒绝（CallInsideFunction 错误由 VM 检查）
        let tokens = vec![
            colon(1),
            colon(2), semicolon(3),  // Call(name=2, argc=3) — 函数体内调用
            colon(1),
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.functions.len(), 1);
        let body = &result.functions[&1];
        assert_eq!(body.len(), 1);
        assert_eq!(body[0], Instruction::Call { name: 2, argc: 3 });
    }

    // ═══════════ 7. 函数调用 (顶层) ═══════════

    #[test]
    fn function_call_at_top_level() {
        // (3): (2);  →  Call(name=3, argc=2)
        let tokens = vec![
            colon(3),
            semicolon(2),
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.main_instructions.len(), 1);
        assert_eq!(result.main_instructions[0], Instruction::Call { name: 3, argc: 2 });
    }

    #[test]
    fn function_call_with_zero_args() {
        // (1): (0);
        let tokens = vec![
            colon(1),
            semicolon(0),
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.main_instructions[0], Instruction::Call { name: 1, argc: 0 });
    }

    #[test]
    fn function_declaration_then_call() {
        // 声明函数1: Push(0) CharOut ; 然后调用它: (1): (0);
        let tokens = vec![
            colon(1), plus(5), comma(0), colon(1),  // 函数声明
            colon(1), semicolon(0),                  // 函数调用
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.main_instructions.len(), 1);
        assert_eq!(result.main_instructions[0], Instruction::Call { name: 1, argc: 0 });
    }

    // ═══════════ 8. 混合指令 ═══════════

    #[test]
    fn multiple_instructions_sequence() {
        let tokens = vec![
            plus(5),     // Push(0)
            plus(5),     // Push(0)
            star(0),     // Add
            dot(0),      // NumOut
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.main_instructions.len(), 4);
    }

    #[test]
    fn hello_world_style_sequence() {
        // Push(72) CharOut → 'H'
        let tokens = vec![
            plus(77),   // Push(72)
            comma(0),   // CharOut
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.main_instructions[0], Instruction::Push(72));
        assert_eq!(result.main_instructions[1], Instruction::CharOut);
    }

    #[test]
    fn mark_then_jump_then_unconditional_jump() {
        let tokens = vec![
            backtick(0),  // Mark(0)
            plus(5),      // Push(0)
            quote(0),     // Jump(0) — 条件跳转
            tilde(0),     // UncondJump(0)
        ];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.main_instructions.len(), 4);
        assert_eq!(result.main_marks.len(), 1);
        assert!(result.main_marks.contains_key(&0));
    }

    // ═══════════ 9. 空程序 ═══════════

    #[test]
    fn empty_program() {
        let result = parse_program(vec![]).unwrap();
        assert!(result.main_instructions.is_empty());
        assert!(result.main_marks.is_empty());
        assert!(result.functions.is_empty());
    }

    // ═══════════ 10. 参数个数 = Semicolon 的前导空格数 ═══════════

    #[test]
    fn call_argc_from_semicolon_spaces() {
        // (0): (3);  调用 name=0, argc=3
        let tokens = vec![colon(0), semicolon(3)];
        let result = parse_program(tokens).unwrap();
        assert_eq!(result.main_instructions[0], Instruction::Call { name: 0, argc: 3 });
    }

    // ═══════════ 11. Mark 中的 SourceSpan ═══════════

    #[test]
    fn mark_stores_source_position() {
        let t = tok_at(1, TokenType::Backtick, 5, 10);
        let result = parse_program(vec![t]).unwrap();
        match &result.main_instructions[0] {
            Instruction::Mark { name: 1, span } => {
                assert_eq!(span.line, 5);
                assert_eq!(span.column, 10);
            }
            _ => panic!("expected Mark with span"),
        }
    }
}
