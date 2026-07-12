use super::validation::{parse_error, ParseError};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    Keyword(String),
    Identifier(String),
    Colon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Hyphen,
    Arrow,
    RevArrow,
    Comma,
    Dot,
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Star,
    StringLiteral(String),
    NumberLiteral(String),
    Eof,
}

pub(super) struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub(super) fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        self.pos += 1;
        ch
    }

    fn eat_while(&mut self, pred: fn(char) -> bool) -> String {
        let mut s = String::new();
        while let Some(ch) = self.peek_char() {
            if pred(ch) {
                s.push(ch);
                self.pos += 1;
            } else {
                break;
            }
        }
        s
    }

    pub(super) fn next_token(&mut self) -> Result<Token, ParseError> {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }

        let ch = match self.peek_char() {
            Some(c) => c,
            None => return Ok(Token::Eof),
        };

        match ch {
            '(' => self.single(Token::LParen),
            ')' => self.single(Token::RParen),
            '[' => self.single(Token::LBracket),
            ']' => self.single(Token::RBracket),
            ':' => self.single(Token::Colon),
            ',' => self.single(Token::Comma),
            '.' => self.single(Token::Dot),
            '*' => self.single(Token::Star),
            '-' => self.hyphen_or_arrow(),
            '<' => self.left_operator(),
            '>' => self.greater_operator(),
            '=' => self.single(Token::Eq),
            '!' => self.bang_operator(),
            '\'' | '"' => self.string_literal(ch),
            _ if ch.is_ascii_digit() => {
                let num = self.eat_while(|c| c.is_ascii_digit() || c == '.');
                Ok(Token::NumberLiteral(num))
            }
            _ if ch.is_alphabetic() || ch == '_' => Ok(self.identifier_or_keyword()),
            _ => Err(parse_error(
                format!("Unexpected character: '{}'", ch),
                self.pos,
            )),
        }
    }

    pub(super) fn pos_before(&self) -> usize {
        self.pos.saturating_sub(1)
    }

    fn single(&mut self, token: Token) -> Result<Token, ParseError> {
        self.pos += 1;
        Ok(token)
    }

    fn hyphen_or_arrow(&mut self) -> Result<Token, ParseError> {
        self.pos += 1;
        if self.peek_char() == Some('>') {
            self.pos += 1;
            Ok(Token::Arrow)
        } else {
            Ok(Token::Hyphen)
        }
    }

    fn left_operator(&mut self) -> Result<Token, ParseError> {
        self.pos += 1;
        if self.peek_char() == Some('-') {
            self.pos += 1;
            Ok(Token::RevArrow)
        } else if self.peek_char() == Some('=') {
            self.pos += 1;
            Ok(Token::Lte)
        } else {
            Ok(Token::Lt)
        }
    }

    fn greater_operator(&mut self) -> Result<Token, ParseError> {
        self.pos += 1;
        if self.peek_char() == Some('=') {
            self.pos += 1;
            Ok(Token::Gte)
        } else {
            Ok(Token::Gt)
        }
    }

    fn bang_operator(&mut self) -> Result<Token, ParseError> {
        self.pos += 1;
        if self.peek_char() == Some('=') {
            self.pos += 1;
            Ok(Token::Neq)
        } else {
            Err(parse_error("Expected '=' after '!'", self.pos))
        }
    }

    fn string_literal(&mut self, quote: char) -> Result<Token, ParseError> {
        self.pos += 1;
        let mut s = String::new();
        loop {
            match self.next_char() {
                Some(c) if c == quote => break,
                Some(c) => s.push(c),
                None => return Err(parse_error("Unterminated string literal", self.pos)),
            }
        }
        Ok(Token::StringLiteral(s))
    }

    fn identifier_or_keyword(&mut self) -> Token {
        let ident = self.eat_while(|c| c.is_alphanumeric() || c == '_');
        let upper = ident.to_uppercase();
        match upper.as_str() {
            "MATCH" | "WHERE" | "RETURN" | "LIMIT" | "AND" | "OR" | "LIKE" | "CONTAINS"
            | "COUNT" | "NULL" | "TRUE" | "FALSE" | "NOT" | "MAX" | "MIN" | "AVG" | "SUM" => {
                Token::Keyword(ident)
            }
            _ => Token::Identifier(ident),
        }
    }
}
