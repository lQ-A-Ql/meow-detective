//! GQL parser - converts query strings into an AST.
//!
//! Supports a Cypher-inspired syntax:
//!
//! ```text
//! MATCH (n:NodeType)-[e:EdgeType]->(m:NodeType)
//! WHERE n.property = 'value' AND e.confidence > 0.7
//! RETURN n, e, m, count(*)
//! LIMIT 50
//! ```

mod ast;
mod lexer;
mod syntax;
mod validation;

pub use ast::{
    ComparisonOp, Expr, LogicalConnector, MatchClause, MatchDirection, Predicate, Query,
    ReturnClause, ReturnItem, Value, WhereClause,
};
pub use validation::ParseError;

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
    let mut parser = syntax::Parser::new(input)?;
    parser.parse_query()
}
