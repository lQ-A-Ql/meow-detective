use serde::{Deserialize, Serialize};
use std::fmt;

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
