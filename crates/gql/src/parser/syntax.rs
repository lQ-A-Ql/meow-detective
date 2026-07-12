use super::lexer::{Lexer, Token};
use super::validation::{parse_error, ParseError};
use super::{
    ComparisonOp, LogicalConnector, MatchClause, MatchDirection, Predicate, Query, ReturnClause,
    ReturnItem, Value, WhereClause,
};

pub(super) struct Parser {
    lexer: Lexer,
    current: Token,
}

impl Parser {
    pub(super) fn new(input: &str) -> Result<Self, ParseError> {
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
            _ => Err(parse_error(
                format!("Expected keyword '{}', got {:?}", kw, self.current),
                self.lexer.pos_before(),
            )),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.advance()? {
            Token::Identifier(s) => Ok(s),
            tok => Err(parse_error(
                format!("Expected identifier, got {:?}", tok),
                self.lexer.pos_before(),
            )),
        }
    }

    fn expect_punct(&mut self, tok: Token) -> Result<(), ParseError> {
        let expected_variant = std::mem::discriminant(&tok);
        let current_variant = std::mem::discriminant(&self.current);
        if expected_variant == current_variant {
            self.advance()?;
            Ok(())
        } else {
            Err(parse_error(
                format!("Expected {:?}, got {:?}", tok, self.current),
                self.lexer.pos_before(),
            ))
        }
    }

    pub(super) fn parse_query(&mut self) -> Result<Query, ParseError> {
        let match_clause = self.parse_match()?;
        let where_clause = self.parse_where()?;
        let return_clause = self.parse_return()?;
        let limit = self.parse_limit()?;

        if self.current != Token::Eof {
            return Err(parse_error(
                format!("Unexpected token after query: {:?}", self.current),
                self.lexer.pos_before(),
            ));
        }

        Ok(Query {
            match_clause,
            where_clause,
            return_clause,
            limit,
        })
    }

    fn parse_match(&mut self) -> Result<MatchClause, ParseError> {
        self.expect_keyword("MATCH")?;
        let (source_var, source_type) = self.parse_node()?;

        match &self.current {
            Token::Hyphen | Token::Arrow => self.parse_left_to_right_match(source_var, source_type),
            Token::RevArrow => self.parse_right_to_left_match(source_var, source_type),
            _ => Err(parse_error(
                format!("Expected edge pattern, got {:?}", self.current),
                self.lexer.pos_before(),
            )),
        }
    }

    fn parse_left_to_right_match(
        &mut self,
        source_var: String,
        source_type: Option<String>,
    ) -> Result<MatchClause, ParseError> {
        if self.current == Token::Arrow {
            self.advance()?;
            return self.bare_match(source_var, source_type, MatchDirection::LeftToRight);
        }

        self.expect_punct(Token::Hyphen)?;
        if self.current == Token::Arrow {
            self.advance()?;
            return self.bare_match(source_var, source_type, MatchDirection::LeftToRight);
        }

        self.expect_punct(Token::LBracket)?;
        let edge_var = self.expect_ident()?;
        let edge_type = self.parse_optional_type()?;
        self.expect_punct(Token::RBracket)?;
        self.expect_punct(Token::Arrow)?;
        let (target_var, target_type) = self.parse_node()?;

        Ok(MatchClause {
            source_var,
            source_type,
            edge_var,
            edge_type,
            target_var,
            target_type,
            direction: MatchDirection::LeftToRight,
        })
    }

    fn parse_right_to_left_match(
        &mut self,
        source_var: String,
        source_type: Option<String>,
    ) -> Result<MatchClause, ParseError> {
        self.expect_punct(Token::RevArrow)?;
        let (edge_var, edge_type) = if self.current == Token::LBracket {
            self.advance()?;
            let edge_var = self.expect_ident()?;
            let edge_type = self.parse_optional_type()?;
            self.expect_punct(Token::RBracket)?;
            (edge_var, edge_type)
        } else {
            ("_".to_string(), None)
        };
        self.expect_punct(Token::Hyphen)?;
        let (target_var, target_type) = self.parse_node()?;

        Ok(MatchClause {
            source_var,
            source_type,
            edge_var,
            edge_type,
            target_var,
            target_type,
            direction: MatchDirection::RightToLeft,
        })
    }

    fn bare_match(
        &mut self,
        source_var: String,
        source_type: Option<String>,
        direction: MatchDirection,
    ) -> Result<MatchClause, ParseError> {
        let (target_var, target_type) = self.parse_node()?;
        Ok(MatchClause {
            source_var,
            source_type,
            edge_var: "_".to_string(),
            edge_type: None,
            target_var,
            target_type,
            direction,
        })
    }

    fn parse_node(&mut self) -> Result<(String, Option<String>), ParseError> {
        self.expect_punct(Token::LParen)?;
        let node_var = self.expect_ident()?;
        let node_type = self.parse_optional_type()?;
        self.expect_punct(Token::RParen)?;
        Ok((node_var, node_type))
    }

    fn parse_optional_type(&mut self) -> Result<Option<String>, ParseError> {
        if self.current == Token::Colon {
            self.advance()?;
            Ok(Some(self.expect_ident()?))
        } else {
            Ok(None)
        }
    }

    fn parse_where(&mut self) -> Result<Option<WhereClause>, ParseError> {
        if !self.current_keyword_is("WHERE") {
            return Ok(None);
        }

        self.expect_keyword("WHERE")?;
        let mut predicates = vec![self.parse_predicate()?];
        let mut connector = LogicalConnector::And;

        while let Token::Keyword(ref k) = self.current {
            let upper = k.to_uppercase();
            if upper != "AND" && upper != "OR" {
                break;
            }
            if upper == "OR" {
                connector = LogicalConnector::Or;
            }
            self.advance()?;
            predicates.push(self.parse_predicate()?);
        }

        Ok(Some(WhereClause {
            predicates,
            connector,
        }))
    }

    fn parse_predicate(&mut self) -> Result<Predicate, ParseError> {
        let variable = self.expect_ident()?;
        self.expect_punct(Token::Dot)?;
        let property = self.expect_ident()?;
        let operator = self.parse_operator()?;
        let value = self.parse_value()?;

        Ok(Predicate {
            variable,
            property,
            operator,
            value,
        })
    }

    fn parse_operator(&mut self) -> Result<ComparisonOp, ParseError> {
        let negated = self.consume_keyword("NOT")?;
        match &self.current {
            Token::Eq => {
                self.advance()?;
                Ok(if negated {
                    ComparisonOp::Neq
                } else {
                    ComparisonOp::Eq
                })
            }
            Token::Neq => {
                self.advance()?;
                Ok(ComparisonOp::Neq)
            }
            Token::Gt => {
                self.advance()?;
                Ok(ComparisonOp::Gt)
            }
            Token::Gte => {
                self.advance()?;
                Ok(ComparisonOp::Gte)
            }
            Token::Lt => {
                self.advance()?;
                Ok(ComparisonOp::Lt)
            }
            Token::Lte => {
                self.advance()?;
                Ok(ComparisonOp::Lte)
            }
            Token::Keyword(k) => self.parse_keyword_operator(k.clone()),
            _ => Err(parse_error(
                format!("Expected comparison operator, got {:?}", self.current),
                self.lexer.pos_before(),
            )),
        }
    }

    fn parse_keyword_operator(&mut self, keyword: String) -> Result<ComparisonOp, ParseError> {
        match keyword.to_uppercase().as_str() {
            "LIKE" => {
                self.advance()?;
                Ok(ComparisonOp::Like)
            }
            "CONTAINS" => {
                self.advance()?;
                Ok(ComparisonOp::Contains)
            }
            _ => Err(parse_error(
                format!("Expected comparison operator, got keyword '{}'", keyword),
                self.lexer.pos_before(),
            )),
        }
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        match &self.current {
            Token::StringLiteral(s) => {
                let val = Value::String(s.clone());
                self.advance()?;
                Ok(val)
            }
            Token::NumberLiteral(s) => {
                let n: f64 = s.parse().map_err(|_| {
                    parse_error(format!("Invalid number: {}", s), self.lexer.pos_before())
                })?;
                self.advance()?;
                Ok(Value::Number(n))
            }
            Token::Keyword(k) => self.parse_keyword_value(k.clone()),
            _ => Err(parse_error(
                format!("Expected literal value, got {:?}", self.current),
                self.lexer.pos_before(),
            )),
        }
    }

    fn parse_keyword_value(&mut self, keyword: String) -> Result<Value, ParseError> {
        match keyword.to_uppercase().as_str() {
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
            _ => Err(parse_error(
                format!("Expected value, got keyword '{}'", keyword),
                self.lexer.pos_before(),
            )),
        }
    }

    fn parse_return(&mut self) -> Result<ReturnClause, ParseError> {
        self.expect_keyword("RETURN")?;
        let mut items = vec![self.parse_return_item()?];
        while self.current == Token::Comma {
            self.advance()?;
            items.push(self.parse_return_item()?);
        }
        Ok(ReturnClause { items })
    }

    fn parse_return_item(&mut self) -> Result<ReturnItem, ParseError> {
        if let Token::Keyword(k) = &self.current {
            let upper = k.to_uppercase();
            if upper == "COUNT" {
                return self.parse_count_return();
            }
            if matches!(upper.as_str(), "MIN" | "MAX" | "AVG" | "SUM") {
                return self.parse_aggregate_return(upper.to_lowercase());
            }
        }

        Ok(ReturnItem::Variable(self.expect_ident()?))
    }

    fn parse_count_return(&mut self) -> Result<ReturnItem, ParseError> {
        self.advance()?;
        self.expect_punct(Token::LParen)?;
        if self.current == Token::Star {
            self.advance()?;
            self.expect_punct(Token::RParen)?;
            return Ok(ReturnItem::CountStar);
        }
        let var = self.expect_ident()?;
        self.expect_punct(Token::RParen)?;
        Ok(ReturnItem::Count(var))
    }

    fn parse_aggregate_return(&mut self, func: String) -> Result<ReturnItem, ParseError> {
        self.advance()?;
        self.expect_punct(Token::LParen)?;
        let first = self.expect_ident()?;
        let variable = if self.current == Token::Dot {
            self.advance()?;
            format!("{}.{}", first, self.expect_ident()?)
        } else {
            first
        };
        self.expect_punct(Token::RParen)?;
        Ok(ReturnItem::Aggregate { func, variable })
    }

    fn parse_limit(&mut self) -> Result<Option<u32>, ParseError> {
        if !self.current_keyword_is("LIMIT") {
            return Ok(None);
        }
        self.advance()?;
        if let Token::NumberLiteral(ref s) = self.current {
            let n: u32 = s.parse().map_err(|_| {
                parse_error(format!("Invalid limit: {}", s), self.lexer.pos_before())
            })?;
            self.advance()?;
            return Ok(Some(n));
        }
        Err(parse_error(
            "Expected number after LIMIT",
            self.lexer.pos_before(),
        ))
    }

    fn consume_keyword(&mut self, keyword: &str) -> Result<bool, ParseError> {
        if self.current_keyword_is(keyword) {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn current_keyword_is(&self, keyword: &str) -> bool {
        matches!(&self.current, Token::Keyword(k) if k.to_uppercase() == keyword)
    }
}
