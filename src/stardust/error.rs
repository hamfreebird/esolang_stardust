use std::fmt;
use crate::stardust::{ErrorKind, SourceSpan, StardustError};

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

impl StardustError {
    pub fn new(kind: ErrorKind, span: Option<SourceSpan>) -> Self {
        let message = kind.default_message();
        StardustError { kind, span, message }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }
}

impl ErrorKind {
    fn default_message(&self) -> String {
        match self {
            ErrorKind::InvalidCharacter { ch } => format!("invalid character '{}'", ch),
            ErrorKind::NonSymbolicCharacter => "non-symbolic character".to_string(),
            ErrorKind::TrailingSpaces => "trailing spaces without following symbol".to_string(),
            ErrorKind::UnexpectedToken { expected, found } => {
                format!("expected {}, found {}", expected, found)
            }
            ErrorKind::DuplicateMark { name } => format!("duplicate mark '{}'", name),
            ErrorKind::DuplicateFunction { name } => format!("duplicate function '{}'", name),
            ErrorKind::UndefinedMark { name } => format!("undefined mark '{}'", name),
            ErrorKind::UndefinedFunction { name } => format!("undefined function '{}'", name),
            ErrorKind::CallInsideFunction => "function call not allowed inside function body".to_string(),
            ErrorKind::UnclosedFunction { name } => format!("unclosed function '{}'", name),
            ErrorKind::StackUnderflow => "stack underflow".to_string(),
            ErrorKind::DivisionByZero => "division by zero".to_string(),
            ErrorKind::ModuloByZero => "modulo by zero".to_string(),
            ErrorKind::InvalidAscii { value } => format!("ASCII value {} out of range (0-127)", value),
            ErrorKind::InvalidIntegerInput => "invalid integer input".to_string(),
            ErrorKind::IoError { reason } => format!("I/O error: {}", reason),
            ErrorKind::UnexpectedEof => "unexpected end of file".to_string(),
            ErrorKind::IncompleteFunctionCall => "incomplete function call".to_string(),
            ErrorKind::ExpectedColonInCall => "expected ':' after function name in call".to_string(),
            ErrorKind::ExpectedSemicolonAfterCall => "expected ';' at end of function call".to_string(),
            ErrorKind::InvalidSpacesForPlus => "invalid number of spaces for '+' instruction (must be >= 1)".to_string(),
            ErrorKind::InvalidSpacesForStar { spaces } => {
                format!("invalid number of spaces ({}) for '*' instruction (must be 0-5)", spaces)
            }
            ErrorKind::InvalidSpacesForDot { spaces } => {
                format!("invalid number of spaces ({}) for '.' instruction (must be 0 or 1)", spaces)
            }
            ErrorKind::InvalidSpacesForComma { spaces } => {
                format!("invalid number of spaces ({}) for ',' instruction (must be 0 or 1)", spaces)
            }
            ErrorKind::InvalidInstructionContext => "instruction not allowed in this context".to_string(),
            ErrorKind::NotEnoughArguments { func, expected, actual } => {
                format!(
                    "not enough arguments for function '{}': expected {}, but stack has {}",
                    func, expected, actual
                )
            }
        }
    }
}

impl fmt::Display for StardustError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(span) = &self.span {
            write!(f, "{} at {}", self.message, span)
        } else {
            write!(f, "{}", self.message)
        }
    }
}