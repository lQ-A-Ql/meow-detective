//! GQL parser — converts query strings into an AST.
//!
//! Supports a Cypher-inspired syntax:
//!
//! ```text
//! MATCH (n:NodeType)-[e:EdgeType]->(m:NodeType)
//! WHERE n.property = 'value' AND e.confidence > 0.7
//! RETURN n, e, m, count(*)
//! LIMIT 50
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

// ── AST types ──

/// A parsed GQL query.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub match_clause: MatchClause,
    pub where_clause: Option<WhereClause>,
    pub return_clause: ReturnClause,
    pub limit: Option<u32>,
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.match_clause)?;
        if let Some(ref w) = self.where_clause {
            write!(f, " {}", w)?;
        }
        write!(f, " {}", self.return_clause)?;
        if let Some(lim) = self.limit {
            write!(f, " LIMIT {}", lim)?;
        }
        Ok(())
    }
}

/// MATCH clause specifying the graph pattern.
///
/// Pattern: `(source:NodeType)-[edge:EdgeType]->(target:NodeType)`
/// Direction can be left-to-right (`->`) or right-to-left (`<-`).
#[derive(Debug, Clone, PartialEq)]
pub struct MatchClause {
    pub source_var: String,
    pub source_type: Option<String>,
    pub edge_var: String,
    pub edge_type: Option<String>,
    pub target_var: String,
    pub target_type: Option<String>,
    pub direction: MatchDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchDirection {
    LeftToRight,
    RightToLeft,
}

impl fmt::Display for MatchClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MATCH ({}", self.source_var)?;
        if let Some(ref t) = self.source_type {
            write!(f, ":{}", t)?;
        }
        match self.direction {
            MatchDirection::LeftToRight => write!(f, ")-[{}", self.edge_var)?,
            MatchDirection::RightToLeft => write!(f, ")<-[{}", self.edge_var)?,
        }
        if let Some(ref t) = self.edge_type {
            write!(f, ":{}", t)?;
        }
        match self.direction {
            MatchDirection::LeftToRight => write!(f, "]->({}", self.target_var)?,
            MatchDirection::RightToLeft => write!(f, "]-({}", self.target_var)?,
        }
        if let Some(ref t) = self.target_type {
            write!(f, ":{}", t)?;
        }
        write!(f, ")")
    }
}

/// WHERE clause with optional predicates.
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    pub predicates: Vec<Predicate>,
    pub connector: LogicalConnector,
}

impl fmt::Display for WhereClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WHERE ")?;
        for (i, p) in self.predicates.iter().enumerate() {
            if i > 0 {
                match self.connector {
                    LogicalConnector::And => write!(f, " AND ")?,
                    LogicalConnector::Or => write!(f, " OR ")?,
                }
            }
            write!(f, "{}", p)?;
        }
        Ok(())
    }
}

/// A single predicate in a WHERE clause.
#[derive(Debug, Clone, PartialEq)]
pub struct Predicate {
    pub variable: String,
    pub property: String,
    pub operator: ComparisonOp,
    pub value: Value,
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{} {} {}",
            self.variable, self.property, self.operator, self.value
        )
    }
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    Contains,
}

impl fmt::Display for ComparisonOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComparisonOp::Eq => write!(f, "="),
            ComparisonOp::Neq => write!(f, "!="),
            ComparisonOp::Gt => write!(f, ">"),
            ComparisonOp::Gte => write!(f, ">="),
            ComparisonOp::Lt => write!(f, "<"),
            ComparisonOp::Lte => write!(f, "<="),
            ComparisonOp::Like => write!(f, "LIKE"),
            ComparisonOp::Contains => write!(f, "CONTAINS"),
        }
    }
}

/// Literal values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "'{}'", s),
            Value::Number(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Null => write!(f, "NULL"),
        }
    }
}

/// Logical connectors for WHERE predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalConnector {
    And,
    Or,
}

/// RETURN clause specifying which variables or aggregates to project.
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnClause {
    pub items: Vec<ReturnItem>,
}

impl fmt::Display for ReturnClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RETURN ")?;
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            match item {
                ReturnItem::Variable(v) => write!(f, "{}", v)?,
                ReturnItem::CountStar => write!(f, "count(*)")?,
                ReturnItem::Count(v) => write!(f, "count({})", v)?,
                ReturnItem::Aggregate { func, variable } => {
                    write!(f, "{}({})", func, variable)?;
                }
            }
        }
        Ok(())
    }
}

/// Items that can appear in a RETURN clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnItem {
    Variable(String),
    CountStar,
    Count(String),
    Aggregate { func: String, variable: String },
}

/// Expressions for property paths etc.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Identifier(String),
    Property { variable: String, field: String },
    Literal(Value),
}

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

// ── Tokenizer ──

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Keyword(String),
    Identifier(String),
    Colon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Hyphen,
    Arrow,    // ->
    RevArrow, // <-
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

struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    fn new(input: &str) -> Self {
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

    fn next_token(&mut self) -> Result<Token, ParseError> {
        // Skip whitespace
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

        // Single-character tokens
        match ch {
            '(' => {
                self.pos += 1;
                Ok(Token::LParen)
            }
            ')' => {
                self.pos += 1;
                Ok(Token::RParen)
            }
            '[' => {
                self.pos += 1;
                Ok(Token::LBracket)
            }
            ']' => {
                self.pos += 1;
                Ok(Token::RBracket)
            }
            ':' => {
                self.pos += 1;
                Ok(Token::Colon)
            }
            ',' => {
                self.pos += 1;
                Ok(Token::Comma)
            }
            '.' => {
                self.pos += 1;
                Ok(Token::Dot)
            }
            '*' => {
                self.pos += 1;
                Ok(Token::Star)
            }
            '-' => {
                self.pos += 1;
                // Check for arrow ->
                if self.peek_char() == Some('>') {
                    self.pos += 1;
                    Ok(Token::Arrow)
                } else {
                    Ok(Token::Hyphen)
                }
            }
            '<' => {
                self.pos += 1;
                // Check for <-
                if self.peek_char() == Some('-') {
                    self.pos += 1;
                    Ok(Token::RevArrow)
                }
                // Check for <=
                else if self.peek_char() == Some('=') {
                    self.pos += 1;
                    Ok(Token::Lte)
                } else {
                    Ok(Token::Lt)
                }
            }
            '>' => {
                self.pos += 1;
                if self.peek_char() == Some('=') {
                    self.pos += 1;
                    Ok(Token::Gte)
                } else {
                    Ok(Token::Gt)
                }
            }
            '=' => {
                self.pos += 1;
                Ok(Token::Eq)
            }
            '!' => {
                self.pos += 1;
                if self.peek_char() == Some('=') {
                    self.pos += 1;
                    Ok(Token::Neq)
                } else {
                    Err(ParseError {
                        message: "Expected '=' after '!'".to_string(),
                        position: self.pos,
                    })
                }
            }
            '\'' | '"' => {
                let quote = ch;
                self.pos += 1;
                let mut s = String::new();
                loop {
                    match self.next_char() {
                        Some(c) if c == quote => break,
                        Some(c) => s.push(c),
                        None => {
                            return Err(ParseError {
                                message: "Unterminated string literal".to_string(),
                                position: self.pos,
                            });
                        }
                    }
                }
                Ok(Token::StringLiteral(s))
            }
            _ if ch.is_ascii_digit() => {
                let num = self.eat_while(|c| c.is_ascii_digit() || c == '.');
                Ok(Token::NumberLiteral(num))
            }
            _ if ch.is_alphabetic() || ch == '_' => {
                let ident = self.eat_while(|c| c.is_alphanumeric() || c == '_');
                // Check keywords
                let upper = ident.to_uppercase();
                match upper.as_str() {
                    "MATCH" | "WHERE" | "RETURN" | "LIMIT" | "AND" | "OR" | "LIKE" | "CONTAINS"
                    | "COUNT" | "NULL" | "TRUE" | "FALSE" | "NOT" | "MAX" | "MIN" | "AVG"
                    | "SUM" => Ok(Token::Keyword(ident)),
                    _ => Ok(Token::Identifier(ident)),
                }
            }
            _ => Err(ParseError {
                message: format!("Unexpected character: '{}'", ch),
                position: self.pos,
            }),
        }
    }

    fn pos_before(&self) -> usize {
        self.pos.saturating_sub(1)
    }
}

// ── Parser ──

struct Parser {
    lexer: Lexer,
    current: Token,
}

impl Parser {
    fn new(input: &str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token()?;
        Ok(Self { lexer, current })
    }

    fn advance(&mut self) -> Result<Token, ParseError> {
        let old = std::mem::replace(&mut self.current, self.lexer.next_token()?);
        Ok(old)
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<(), ParseError> {
        let kw_upper = kw.to_uppercase();
        match &self.current {
            Token::Keyword(k) if k.to_uppercase() == kw_upper => {
                self.advance()?;
                Ok(())
            }
            _ => Err(ParseError {
                message: format!("Expected keyword '{}', got {:?}", kw, self.current),
                position: self.lexer.pos_before(),
            }),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.advance()? {
            Token::Identifier(s) => Ok(s),
            tok => Err(ParseError {
                message: format!("Expected identifier, got {:?}", tok),
                position: self.lexer.pos_before(),
            }),
        }
    }

    fn expect_punct(&mut self, tok: Token) -> Result<(), ParseError> {
        let expected_variant = std::mem::discriminant(&tok);
        let current_variant = std::mem::discriminant(&self.current);
        if expected_variant == current_variant {
            self.advance()?;
            Ok(())
        } else {
            Err(ParseError {
                message: format!("Expected {:?}, got {:?}", tok, self.current),
                position: self.lexer.pos_before(),
            })
        }
    }

    /// Parse a full query.
    fn parse_query(&mut self) -> Result<Query, ParseError> {
        let match_clause = self.parse_match()?;
        let where_clause = self.parse_where()?;
        let return_clause = self.parse_return()?;
        let limit = self.parse_limit()?;

        // Should be at end
        if self.current != Token::Eof {
            return Err(ParseError {
                message: format!("Unexpected token after query: {:?}", self.current),
                position: self.lexer.pos_before(),
            });
        }

        Ok(Query {
            match_clause,
            where_clause,
            return_clause,
            limit,
        })
    }

    /// Parse: MATCH (var:Type)-[var:Type]->(var:Type)  or  MATCH (var:Type)<-[var:Type]-(var:Type)
    fn parse_match(&mut self) -> Result<MatchClause, ParseError> {
        self.expect_keyword("MATCH")?;

        // Parse source node: (var:Type)
        self.expect_punct(Token::LParen)?;
        let source_var = self.expect_ident()?;
        let source_type = if self.current == Token::Colon {
            self.advance()?;
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.expect_punct(Token::RParen)?;

        // Determine direction: either ->[...]-> or <-[...]-
        let direction = match &self.current {
            Token::Hyphen | Token::Arrow => MatchDirection::LeftToRight,
            Token::RevArrow => MatchDirection::RightToLeft,
            _ => {
                return Err(ParseError {
                    message: format!("Expected edge pattern, got {:?}", self.current),
                    position: self.lexer.pos_before(),
                })
            }
        };

        match direction {
            MatchDirection::LeftToRight => {
                // Consume the hyphen (or arrow if no edge brackets)
                if self.current == Token::Arrow {
                    // Bare arrow with no edge: (a)-->(b) or (a)->(b)
                    self.advance()?;
                    // Parse target
                    self.expect_punct(Token::LParen)?;
                    let target_var = self.expect_ident()?;
                    let target_type = if self.current == Token::Colon {
                        self.advance()?;
                        Some(self.expect_ident()?)
                    } else {
                        None
                    };
                    self.expect_punct(Token::RParen)?;
                    return Ok(MatchClause {
                        source_var,
                        source_type,
                        edge_var: "_".to_string(),
                        edge_type: None,
                        target_var,
                        target_type,
                        direction,
                    });
                }

                // Expect hyphen; after hyphen, check if next is Arrow (bare edge: -- > )
                self.expect_punct(Token::Hyphen)?;
                if self.current == Token::Arrow {
                    // Bare edge: (a)-->(b)
                    self.advance()?;
                    self.expect_punct(Token::LParen)?;
                    let target_var = self.expect_ident()?;
                    let target_type = if self.current == Token::Colon {
                        self.advance()?;
                        Some(self.expect_ident()?)
                    } else {
                        None
                    };
                    self.expect_punct(Token::RParen)?;
                    return Ok(MatchClause {
                        source_var,
                        source_type,
                        edge_var: "_".to_string(),
                        edge_type: None,
                        target_var,
                        target_type,
                        direction,
                    });
                }
                self.expect_punct(Token::LBracket)?;
                let edge_var = self.expect_ident()?;
                let edge_type = if self.current == Token::Colon {
                    self.advance()?;
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect_punct(Token::RBracket)?;
                self.expect_punct(Token::Arrow)?;

                // Parse target node
                self.expect_punct(Token::LParen)?;
                let target_var = self.expect_ident()?;
                let target_type = if self.current == Token::Colon {
                    self.advance()?;
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect_punct(Token::RParen)?;

                Ok(MatchClause {
                    source_var,
                    source_type,
                    edge_var,
                    edge_type,
                    target_var,
                    target_type,
                    direction,
                })
            }
            MatchDirection::RightToLeft => {
                // <-
                self.expect_punct(Token::RevArrow)?;

                // Check if there's a bracketed edge
                let (edge_var, edge_type) = if self.current == Token::LBracket {
                    self.advance()?;
                    let ev = self.expect_ident()?;
                    let et = if self.current == Token::Colon {
                        self.advance()?;
                        Some(self.expect_ident()?)
                    } else {
                        None
                    };
                    self.expect_punct(Token::RBracket)?;
                    (ev, et)
                } else {
                    ("_".to_string(), None)
                };

                // Expect hyphen
                self.expect_punct(Token::Hyphen)?;

                // Parse target node
                self.expect_punct(Token::LParen)?;
                let target_var = self.expect_ident()?;
                let target_type = if self.current == Token::Colon {
                    self.advance()?;
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect_punct(Token::RParen)?;

                Ok(MatchClause {
                    source_var,
                    source_type,
                    edge_var,
                    edge_type,
                    target_var,
                    target_type,
                    direction,
                })
            }
        }
    }

    /// Parse optional WHERE clause.
    fn parse_where(&mut self) -> Result<Option<WhereClause>, ParseError> {
        if self.current != Token::Keyword("WHERE".to_string())
            && self.current != Token::Keyword("where".to_string())
        {
            // Peek ahead - check the token content
            match &self.current {
                Token::Keyword(k) if k.to_uppercase() == "WHERE" => {}
                _ => return Ok(None),
            }
        }

        self.expect_keyword("WHERE")?;

        let mut predicates = Vec::new();
        let mut connector = LogicalConnector::And;

        // Parse first predicate
        predicates.push(self.parse_predicate()?);

        // Parse additional predicates connected by AND/OR
        while let Token::Keyword(ref k) = self.current {
            let upper = k.to_uppercase();
            if upper == "AND" || upper == "OR" {
                let is_or = upper == "OR";
                if !predicates.is_empty() && is_or {
                    connector = LogicalConnector::Or;
                }
                if is_or && connector != LogicalConnector::Or {
                    // mixed connectors not supported yet
                }
                self.advance()?;
                predicates.push(self.parse_predicate()?);
            } else {
                break;
            }
        }

        Ok(Some(WhereClause {
            predicates,
            connector,
        }))
    }

    /// Parse a single predicate: variable.property operator value
    fn parse_predicate(&mut self) -> Result<Predicate, ParseError> {
        let variable = self.expect_ident()?;
        self.expect_punct(Token::Dot)?;
        let property = self.expect_ident()?;

        // Check for NOT before a keyword operator
        let mut negated = false;
        if let Token::Keyword(ref k) = self.current {
            if k.to_uppercase() == "NOT" {
                self.advance()?;
                negated = true;
            }
        }

        // Parse operator
        let operator = match &self.current {
            Token::Eq => {
                self.advance()?;
                if negated {
                    ComparisonOp::Neq
                } else {
                    ComparisonOp::Eq
                }
            }
            Token::Neq => {
                self.advance()?;
                ComparisonOp::Neq
            }
            Token::Gt => {
                self.advance()?;
                ComparisonOp::Gt
            }
            Token::Gte => {
                self.advance()?;
                ComparisonOp::Gte
            }
            Token::Lt => {
                self.advance()?;
                ComparisonOp::Lt
            }
            Token::Lte => {
                self.advance()?;
                ComparisonOp::Lte
            }
            Token::Keyword(ref k) => {
                let upper = k.to_uppercase();
                match upper.as_str() {
                    "LIKE" => {
                        self.advance()?;
                        ComparisonOp::Like
                    }
                    "CONTAINS" => {
                        self.advance()?;
                        ComparisonOp::Contains
                    }
                    _ => {
                        return Err(ParseError {
                            message: format!("Expected comparison operator, got keyword '{}'", k),
                            position: self.lexer.pos_before(),
                        });
                    }
                }
            }
            _ => {
                return Err(ParseError {
                    message: format!("Expected comparison operator, got {:?}", self.current),
                    position: self.lexer.pos_before(),
                });
            }
        };

        let value = self.parse_value()?;

        Ok(Predicate {
            variable,
            property,
            operator,
            value,
        })
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        match &self.current {
            Token::StringLiteral(s) => {
                let val = Value::String(s.clone());
                self.advance()?;
                Ok(val)
            }
            Token::NumberLiteral(s) => {
                let n: f64 = s.parse().map_err(|_| ParseError {
                    message: format!("Invalid number: {}", s),
                    position: self.lexer.pos_before(),
                })?;
                self.advance()?;
                Ok(Value::Number(n))
            }
            Token::Keyword(ref k) => {
                let upper = k.to_uppercase();
                match upper.as_str() {
                    "TRUE" => {
                        self.advance()?;
                        Ok(Value::Bool(true))
                    }
                    "FALSE" => {
                        self.advance()?;
                        Ok(Value::Bool(false))
                    }
                    "NULL" => {
                        self.advance()?;
                        Ok(Value::Null)
                    }
                    _ => Err(ParseError {
                        message: format!("Expected value, got keyword '{}'", k),
                        position: self.lexer.pos_before(),
                    }),
                }
            }
            _ => Err(ParseError {
                message: format!("Expected literal value, got {:?}", self.current),
                position: self.lexer.pos_before(),
            }),
        }
    }

    /// Parse RETURN clause.
    fn parse_return(&mut self) -> Result<ReturnClause, ParseError> {
        self.expect_keyword("RETURN")?;

        let mut items = Vec::new();
        items.push(self.parse_return_item()?);

        while self.current == Token::Comma {
            self.advance()?;
            items.push(self.parse_return_item()?);
        }

        Ok(ReturnClause { items })
    }

    fn parse_return_item(&mut self) -> Result<ReturnItem, ParseError> {
        // check for count(*)
        if let Token::Keyword(ref k) = self.current {
            let upper = k.to_uppercase();
            if upper == "COUNT" {
                self.advance()?;
                self.expect_punct(Token::LParen)?;
                if self.current == Token::Star {
                    self.advance()?;
                    self.expect_punct(Token::RParen)?;
                    return Ok(ReturnItem::CountStar);
                }
                let var = self.expect_ident()?;
                self.expect_punct(Token::RParen)?;
                return Ok(ReturnItem::Count(var));
            }
            // Other aggregates: MIN(var.prop), MAX(var.prop), AVG(var.prop), SUM(var.prop)
            if matches!(upper.as_str(), "MIN" | "MAX" | "AVG" | "SUM") {
                let func = upper.to_lowercase();
                self.advance()?;
                self.expect_punct(Token::LParen)?;
                let first = self.expect_ident()?;
                let variable = if self.current == Token::Dot {
                    self.advance()?;
                    let second = self.expect_ident()?;
                    format!("{}.{}", first, second)
                } else {
                    first
                };
                self.expect_punct(Token::RParen)?;
                return Ok(ReturnItem::Aggregate { func, variable });
            }
        }

        // Simple variable reference
        let var = self.expect_ident()?;
        Ok(ReturnItem::Variable(var))
    }

    /// Parse optional LIMIT clause.
    fn parse_limit(&mut self) -> Result<Option<u32>, ParseError> {
        if let Token::Keyword(ref k) = self.current {
            if k.to_uppercase() == "LIMIT" {
                self.advance()?;
                if let Token::NumberLiteral(ref s) = self.current {
                    let n: u32 = s.parse().map_err(|_| ParseError {
                        message: format!("Invalid limit: {}", s),
                        position: self.lexer.pos_before(),
                    })?;
                    self.advance()?;
                    return Ok(Some(n));
                }
                return Err(ParseError {
                    message: "Expected number after LIMIT".to_string(),
                    position: self.lexer.pos_before(),
                });
            }
        }
        Ok(None)
    }
}

// ── Public API ──

/// Parse a GQL query string into an AST.
///
/// # Examples
///
/// ```
/// use gql::parser::parse;
///
/// let query = parse(
///     "MATCH (n:File)-[e:References]->(m:Artifact) WHERE e.confidence > 0.7 RETURN n, e, m LIMIT 50"
/// ).unwrap();
/// assert_eq!(query.match_clause.source_var, "n");
/// ```
pub fn parse(input: &str) -> Result<Query, ParseError> {
    let mut parser = Parser::new(input)?;
    parser.parse_query()
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parser tests ──

    #[test]
    fn parse_simple_match_with_types() {
        let q = parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN n").unwrap();
        assert_eq!(q.match_clause.source_var, "n");
        assert_eq!(q.match_clause.source_type, Some("File".to_string()));
        assert_eq!(q.match_clause.edge_var, "e");
        assert_eq!(q.match_clause.edge_type, Some("References".to_string()));
        assert_eq!(q.match_clause.target_var, "m");
        assert_eq!(q.match_clause.target_type, Some("Artifact".to_string()));
        assert_eq!(q.match_clause.direction, MatchDirection::LeftToRight);
        assert_eq!(q.return_clause.items.len(), 1);
        assert!(q.where_clause.is_none());
    }

    #[test]
    fn parse_match_no_type_annotations() {
        let q = parse("MATCH (a)-[r]->(b) RETURN a, b").unwrap();
        assert_eq!(q.match_clause.source_var, "a");
        assert_eq!(q.match_clause.source_type, None);
        assert_eq!(q.match_clause.edge_var, "r");
        assert_eq!(q.match_clause.edge_type, None);
        assert_eq!(q.match_clause.target_var, "b");
        assert_eq!(q.match_clause.target_type, None);
    }

    #[test]
    fn parse_match_reverse_direction() {
        let q = parse("MATCH (a:File)<-[r:References]-(b:File) RETURN a").unwrap();
        assert_eq!(q.match_clause.direction, MatchDirection::RightToLeft);
        assert_eq!(q.match_clause.source_type, Some("File".to_string()));
        assert_eq!(q.match_clause.target_type, Some("File".to_string()));
    }

    #[test]
    fn parse_reverse_direction_bare_arrow() {
        let q = parse("MATCH (a)<--(b) RETURN a, b").unwrap();
        assert_eq!(q.match_clause.direction, MatchDirection::RightToLeft);
        assert_eq!(q.match_clause.source_var, "a");
    }

    #[test]
    fn parse_where_single_predicate_eq_string() {
        let q = parse("MATCH (n:File)-[e]->(m) WHERE n.label = 'cmd.exe' RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates.len(), 1);
        assert_eq!(w.predicates[0].variable, "n");
        assert_eq!(w.predicates[0].property, "label");
        assert_eq!(w.predicates[0].operator, ComparisonOp::Eq);
        assert_eq!(w.predicates[0].value, Value::String("cmd.exe".to_string()));
    }

    #[test]
    fn parse_where_predicate_gt_number() {
        let q = parse("MATCH (n)-[e]->(m) WHERE e.confidence > 0.7 RETURN n, e, m").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates[0].operator, ComparisonOp::Gt);
        assert_eq!(w.predicates[0].value, Value::Number(0.7));
    }

    #[test]
    fn parse_where_predicate_gte_number() {
        let q = parse("MATCH (n)-[e]->(m) WHERE e.confidence >= 0.5 RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates[0].operator, ComparisonOp::Gte);
    }

    #[test]
    fn parse_where_predicate_lte_number() {
        let q = parse("MATCH (n)-[e]->(m) WHERE e.confidence <= 0.9 RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates[0].operator, ComparisonOp::Lte);
    }

    #[test]
    fn parse_where_multiple_predicates_and() {
        let q =
            parse("MATCH (n)-[e]->(m) WHERE n.label = 'cmd.exe' AND e.confidence > 0.7 RETURN n")
                .unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates.len(), 2);
        assert_eq!(w.connector, LogicalConnector::And);
    }

    #[test]
    fn parse_where_multiple_predicates_or() {
        let q = parse("MATCH (n)-[e]->(m) WHERE n.label = 'a' OR n.label = 'b' RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.connector, LogicalConnector::Or);
    }

    #[test]
    fn parse_where_like_operator() {
        let q = parse("MATCH (n:File)-[e]->(m) WHERE n.label LIKE '%cmd%' RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates[0].operator, ComparisonOp::Like);
    }

    #[test]
    fn parse_where_contains_operator() {
        let q =
            parse("MATCH (n:File)-[e]->(m) WHERE n.tags CONTAINS 'executable' RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates[0].operator, ComparisonOp::Contains);
    }

    #[test]
    fn parse_where_not_eq() {
        let q = parse("MATCH (n)-[e]->(m) WHERE n.label != 'test' RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates[0].operator, ComparisonOp::Neq);
    }

    #[test]
    fn parse_return_count_star() {
        let q = parse("MATCH (n)-[e]->(m) RETURN count(*)").unwrap();
        assert_eq!(q.return_clause.items.len(), 1);
        assert_eq!(q.return_clause.items[0], ReturnItem::CountStar);
    }

    #[test]
    fn parse_return_count_var() {
        let q = parse("MATCH (n)-[e]->(m) RETURN count(n)").unwrap();
        assert_eq!(q.return_clause.items[0], ReturnItem::Count("n".to_string()));
    }

    #[test]
    fn parse_return_multiple_items() {
        let q = parse("MATCH (n)-[e]->(m) RETURN n, e, m, count(*)").unwrap();
        assert_eq!(q.return_clause.items.len(), 4);
    }

    #[test]
    fn parse_limit() {
        let q = parse("MATCH (n)-[e]->(m) RETURN n LIMIT 100").unwrap();
        assert_eq!(q.limit, Some(100));
    }

    #[test]
    fn parse_no_limit() {
        let q = parse("MATCH (n)-[e]->(m) RETURN n").unwrap();
        assert_eq!(q.limit, None);
    }

    #[test]
    fn parse_case_insensitive_keywords() {
        let q = parse(
            "match (n:File)-[e:References]->(m:Artifact) where n.label = 'x' return n limit 10",
        )
        .unwrap();
        assert_eq!(q.match_clause.source_type, Some("File".to_string()));
        assert!(q.where_clause.is_some());
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn parse_bare_edge_no_brackets() {
        let q = parse("MATCH (a)-->(b) RETURN a, b").unwrap();
        assert_eq!(q.match_clause.source_var, "a");
        assert_eq!(q.match_clause.target_var, "b");
        assert_eq!(q.match_clause.edge_var, "_");
        assert_eq!(q.match_clause.edge_type, None);
    }

    #[test]
    fn parse_bool_values() {
        let q = parse("MATCH (n)-[e]->(m) WHERE e.confidence = true RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates[0].value, Value::Bool(true));
    }

    #[test]
    fn parse_null_value() {
        let q = parse("MATCH (n)-[e]->(m) WHERE e.provenance = null RETURN n").unwrap();
        let w = q.where_clause.as_ref().unwrap();
        assert_eq!(w.predicates[0].value, Value::Null);
    }

    #[test]
    fn parse_with_aggregate_min() {
        let q = parse("MATCH (n)-[e]->(m) RETURN min(e.confidence)").unwrap();
        assert_eq!(
            q.return_clause.items[0],
            ReturnItem::Aggregate {
                func: "min".to_string(),
                variable: "e.confidence".to_string()
            }
        );
    }

    #[test]
    fn parse_with_aggregate_max() {
        let q = parse("MATCH (n)-[e]->(m) RETURN max(e.confidence)").unwrap();
        assert_eq!(
            q.return_clause.items[0],
            ReturnItem::Aggregate {
                func: "max".to_string(),
                variable: "e.confidence".to_string()
            }
        );
    }

    #[test]
    fn parse_error_on_invalid() {
        let result = parse("INVALID QUERY HERE");
        assert!(result.is_err());
    }

    #[test]
    fn parse_error_unterminated_string() {
        let result = parse("MATCH (n)-[e]->(m) WHERE n.label = 'unterminated RETURN n");
        assert!(result.is_err());
    }

    // ── Display / round-trip tests ──

    #[test]
    fn display_roundtrip_simple() {
        let input = "MATCH (n:File)-[e:References]->(m:Artifact) WHERE n.label = 'test' AND e.confidence > 0.5 RETURN n, e, m LIMIT 50";
        let q = parse(input).unwrap();
        let output = q.to_string();
        assert!(
            output.contains("MATCH"),
            "Display output '{}' should contain MATCH",
            output
        );
        assert!(
            output.contains("WHERE"),
            "Display output '{}' should contain WHERE",
            output
        );
        assert!(
            output.contains("RETURN"),
            "Display output '{}' should contain RETURN",
            output
        );
        assert!(
            output.contains("LIMIT"),
            "Display output '{}' should contain LIMIT",
            output
        );
    }

    #[test]
    fn display_match_no_where_no_limit() {
        let q = parse("MATCH (n)-[e]->(m) RETURN n, e").unwrap();
        let out = q.to_string();
        assert!(out.starts_with("MATCH"));
        assert!(out.ends_with("RETURN n, e"));
    }
}
