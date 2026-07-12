use std::fmt;

/// Parse error with position info.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Parse error at position {}: {}",
            self.position, self.message
        )
    }
}

pub(super) fn parse_error(message: impl Into<String>, position: usize) -> ParseError {
    ParseError {
        message: message.into(),
        position,
    }
}
