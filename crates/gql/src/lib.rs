//! GQL (Graph Query Language) engine for the forensics knowledge graph.
//!
//! Provides a Cypher-inspired query language for traversing and filtering
//! the investigative graph of nodes (Files, Artifacts, TimelineEvents, etc.)
//! and edges (References, Contains, CorrelatesWith, etc.).
//!
//! ## Usage
//!
//! ```ignore
//! use gql::{parse, GqlEngine};
//!
//! let query = parse(r#"
//!     MATCH (n:File)-[e:References]->(m:Artifact)
//!     WHERE e.confidence > 0.7
//!     RETURN n, e, m
//!     LIMIT 50
//! "#).unwrap();
//!
//! let engine = GqlEngine::new(&conn);
//! let result = engine.execute("case-1", &query)?;
//! ```

pub mod engine;
pub mod parser;
pub mod plan;

pub use engine::{GqlEngine, GqlQueryResult};
pub use parser::{parse, Expr, MatchClause, Predicate, Query, ReturnItem, WhereClause};
pub use plan::{QueryPlan, QueryPlanStep};
