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
                if next.token_type != TokenType::Colon {
                    return Err(self.error(ErrorKind::ExpectedColonInCall));
                }
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
