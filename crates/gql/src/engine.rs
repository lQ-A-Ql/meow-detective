//! GQL query engine — translates AST into executable graph queries against
//! the persistence layer.

use crate::parser::*;
use domain::{EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::connection::DbResult;
use persistence_sqlite::repositories::graph_repo::{Direction, GraphRepo};
use rusqlite::{params, Connection};
use std::collections::HashMap;

// ── Query result ──

/// The result of executing a GQL query.
///
/// Contains matched node-edge-node triples and optional aggregate values.
#[derive(Debug, Clone)]
pub struct GqlQueryResult {
    /// Matched source nodes.
    pub source_nodes: Vec<GraphNode>,
    /// Matched edges.
    pub edges: Vec<GraphEdge>,
    /// Matched target nodes.
    pub target_nodes: Vec<GraphNode>,
    /// Aggregate results keyed by their expression (e.g. "count(*)" -> value).
    pub aggregates: HashMap<String, f64>,
    /// Total number of matched triples before LIMIT (for pagination).
    pub total_matched: u64,
}

/// Result of matching a single triple from the graph.
#[derive(Debug, Clone)]
struct MatchedTriple {
    source: GraphNode,
    edge: GraphEdge,
    target: GraphNode,
}

// ── Engine ──

/// The GQL query engine, backed by a SQLite connection.
pub struct GqlEngine<'a> {
    conn: &'a Connection,
}

impl<'a> GqlEngine<'a> {
    /// Create a new engine from a database connection.
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Execute a parsed GQL query against the graph.
    pub fn execute(&self, case_id: &str, query: &Query) -> DbResult<GqlQueryResult> {
        let repo = GraphRepo::new(self.conn);

        // Step 1: Collect matched triples from the graph pattern
        let triples = self.match_pattern(case_id, &query.match_clause, &repo)?;

        // Step 2: Apply WHERE clause filters
        let filtered: Vec<MatchedTriple> = if let Some(ref where_clause) = query.where_clause {
            triples
                .into_iter()
                .filter(|t| self.evaluate_where(where_clause, t))
                .collect()
        } else {
            triples
        };

        let total_matched = filtered.len() as u64;

        // Step 3: Apply LIMIT
        let limited: Vec<MatchedTriple> = if let Some(limit) = query.limit {
            filtered.into_iter().take(limit as usize).collect()
        } else {
            filtered
        };

        // Step 4: Project RETURN items
        let (source_nodes, edges, target_nodes) = self.project_triples(&limited);

        // Step 5: Compute aggregates
        let aggregates = self.compute_aggregates(&query.return_clause, &limited);

        Ok(GqlQueryResult {
            source_nodes,
            edges,
            target_nodes,
            aggregates,
            total_matched,
        })
    }

    // ── Pattern matching ──

    fn match_pattern(
        &self,
        case_id: &str,
        mc: &MatchClause,
        repo: &GraphRepo,
    ) -> DbResult<Vec<MatchedTriple>> {
        let edge_type_filter: Option<EdgeType> = mc.edge_type.as_deref().map(parse_edge_type);

        // Determine which nodes to start from based on type annotations
        let effective_direction = mc.direction;

        // Get all nodes matching the source type filter
        let source_nodes = self.get_nodes_by_type(case_id, mc.source_type.as_deref())?;

        let mut results = Vec::new();

        for source_node in &source_nodes {
            let neighbors = repo.get_neighbors(
                &source_node.id,
                &edge_type_filter
                    .clone()
                    .map(|et| vec![et])
                    .unwrap_or_default(),
                match effective_direction {
                    crate::parser::MatchDirection::LeftToRight => Direction::Outgoing,
                    crate::parser::MatchDirection::RightToLeft => Direction::Incoming,
                },
            )?;

            for (edge, neighbor_node) in neighbors {
                // Filter target by type if specified
                if let Some(ref target_type) = mc.target_type {
                    let expected = node_type_str(parse_node_type(target_type));
                    let actual = node_type_str(neighbor_node.node_type.clone());
                    if actual != expected {
                        continue;
                    }
                }

                // Build triple based on direction.
                // source always corresponds to the left-hand variable in MATCH,
                // target always corresponds to the right-hand variable.
                // The direction arrow tells us how to traverse the graph,
                // but does not change which variable is source vs target.
                let triple = match effective_direction {
                    crate::parser::MatchDirection::LeftToRight => MatchedTriple {
                        source: source_node.clone(),
                        edge,
                        target: neighbor_node,
                    },
                    crate::parser::MatchDirection::RightToLeft => {
                        // Edge goes from neighbor to source_node, but source
                        // (left-hand var) is still source_node.
                        MatchedTriple {
                            source: source_node.clone(),
                            edge,
                            target: neighbor_node,
                        }
                    }
                };

                results.push(triple);
            }
        }

        Ok(results)
    }

    // ── WHERE evaluation ──

    fn evaluate_where(&self, wc: &WhereClause, triple: &MatchedTriple) -> bool {
        let results: Vec<bool> = wc
            .predicates
            .iter()
            .map(|p| self.evaluate_predicate(p, triple))
            .collect();

        match wc.connector {
            LogicalConnector::And => results.iter().all(|&b| b),
            LogicalConnector::Or => results.iter().any(|&b| b),
        }
    }

    fn evaluate_predicate(&self, p: &Predicate, triple: &MatchedTriple) -> bool {
        // Resolve which object the variable refers to and extract the property value
        let prop_value = match p.variable.as_str() {
            "e" | "edge" => self.get_edge_property_from_triple(triple, &p.property),
            var if var == triple.edge.id.as_str() => {
                self.get_edge_property_from_triple(triple, &p.property)
            }
            "n" | "source" => self.get_node_property_direct(&triple.source, &p.property),
            "m" | "target" => self.get_node_property_direct(&triple.target, &p.property),
            var if var == triple.source.id.as_str() => {
                self.get_node_property_direct(&triple.source, &p.property)
            }
            var if var == triple.target.id.as_str() => {
                self.get_node_property_direct(&triple.target, &p.property)
            }
            _ => None, // unknown variable
        };

        match prop_value {
            None => {
                // Property doesn't exist: only NULL = NULL is true
                matches!(p.operator, ComparisonOp::Eq) && matches!(p.value, Value::Null)
            }
            Some(actual) => self.compare_values(&actual, &p.operator, &p.value),
        }
    }

    /// Extract a property value from the edge within a triple.
    fn get_edge_property_from_triple(&self, triple: &MatchedTriple, prop: &str) -> Option<String> {
        match prop {
            "id" => Some(triple.edge.id.clone()),
            "sourceId" | "source_id" => Some(triple.edge.source_id.clone()),
            "targetId" | "target_id" => Some(triple.edge.target_id.clone()),
            "edgeType" | "edge_type" => {
                Some(edge_type_str_upper(&triple.edge.edge_type).to_string())
            }
            "confidence" => triple.edge.confidence.map(|c| c.to_string()),
            "provenance" => triple.edge.provenance.clone(),
            "createdAt" | "created_at" => Some(triple.edge.created_at.clone()),
            _ => None,
        }
    }

    /// Extract a property value from a node.
    fn get_node_property_direct(&self, node: &GraphNode, prop: &str) -> Option<String> {
        match prop {
            "id" => Some(node.id.clone()),
            "label" => Some(node.label.clone()),
            "summary" => Some(node.summary.clone()),
            "nodeType" | "node_type" => Some(node_type_str(node.node_type.clone()).to_string()),
            "createdAt" | "created_at" => Some(node.created_at.clone()),
            "tags" => {
                if node.tags.is_empty() {
                    None
                } else {
                    Some(node.tags.join(","))
                }
            }
            _ => None,
        }
    }

    /// Compare an actual string value against an expected value with an operator.
    fn compare_values(&self, actual: &str, op: &ComparisonOp, expected: &Value) -> bool {
        match expected {
            Value::String(s) => match op {
                ComparisonOp::Eq => actual == s.as_str(),
                ComparisonOp::Neq => actual != s.as_str(),
                ComparisonOp::Like => self.like_match(actual, s),
                ComparisonOp::Contains => actual.contains(s.as_str()),
                ComparisonOp::Gt => actual > s.as_str(),
                ComparisonOp::Gte => actual >= s.as_str(),
                ComparisonOp::Lt => actual < s.as_str(),
                ComparisonOp::Lte => actual <= s.as_str(),
            },
            Value::Number(n) => {
                let actual_num: f64 = match actual.parse() {
                    Ok(v) => v,
                    Err(_) => return false,
                };
                match op {
                    ComparisonOp::Eq => (actual_num - n).abs() < f64::EPSILON,
                    ComparisonOp::Neq => (actual_num - n).abs() >= f64::EPSILON,
                    ComparisonOp::Gt => actual_num > *n,
                    ComparisonOp::Gte => actual_num >= *n,
                    ComparisonOp::Lt => actual_num < *n,
                    ComparisonOp::Lte => actual_num <= *n,
                    _ => false, // LIKE/CONTAINS not applicable to numbers
                }
            }
            Value::Bool(b) => {
                let actual_bool = actual.eq_ignore_ascii_case("true");
                match op {
                    ComparisonOp::Eq => actual_bool == *b,
                    ComparisonOp::Neq => actual_bool != *b,
                    _ => false,
                }
            }
            Value::Null => match op {
                ComparisonOp::Eq => actual.is_empty(),
                ComparisonOp::Neq => !actual.is_empty(),
                _ => false,
            },
        }
    }

    /// Simple LIKE pattern matching (supports % and _ wildcards).
    fn like_match(&self, actual: &str, pattern: &str) -> bool {
        // Simple wildcard matching without regex dependency.
        // Supports % (any sequence) and _ (single character).
        if !pattern.contains('%') && !pattern.contains('_') {
            return actual == pattern;
        }
        // For simple patterns, use substring matching
        if pattern == "%" {
            return true;
        }
        // `%word%` → contains
        if let Some(inner) = pattern.strip_prefix('%').and_then(|s| s.strip_suffix('%')) {
            if !inner.contains('%') && !inner.contains('_') {
                return actual.contains(inner);
            }
        }
        // `word%` → starts with
        if let Some(prefix) = pattern.strip_suffix('%') {
            if !prefix.contains('%') && !prefix.contains('_') {
                return actual.starts_with(prefix);
            }
        }
        // `%word` → ends with
        if let Some(suffix) = pattern.strip_prefix('%') {
            if !suffix.contains('%') && !suffix.contains('_') {
                return actual.ends_with(suffix);
            }
        }

        // Fallback: character-by-character matching with wildcards
        self.wildcard_match(actual, pattern)
    }

    /// Simple wildcard matcher for % and _.
    fn wildcard_match(&self, text: &str, pattern: &str) -> bool {
        let text_chars: Vec<char> = text.chars().collect();
        let pat_chars: Vec<char> = pattern.chars().collect();
        let mut memo = vec![vec![false; pat_chars.len() + 1]; text_chars.len() + 1];
        memo[0][0] = true;

        // Handle leading wildcards
        for j in 1..=pat_chars.len() {
            if pat_chars[j - 1] == '%' {
                memo[0][j] = memo[0][j - 1];
            } else {
                break;
            }
        }

        for i in 1..=text_chars.len() {
            for j in 1..=pat_chars.len() {
                if pat_chars[j - 1] == '%' {
                    memo[i][j] = memo[i - 1][j] || memo[i][j - 1];
                } else if pat_chars[j - 1] == '_' || pat_chars[j - 1] == text_chars[i - 1] {
                    memo[i][j] = memo[i - 1][j - 1];
                }
            }
        }

        memo[text_chars.len()][pat_chars.len()]
    }

    // ── Projection ──

    fn project_triples(
        &self,
        triples: &[MatchedTriple],
    ) -> (Vec<GraphNode>, Vec<GraphEdge>, Vec<GraphNode>) {
        let mut sources = Vec::with_capacity(triples.len());
        let mut edges = Vec::with_capacity(triples.len());
        let mut targets = Vec::with_capacity(triples.len());

        for t in triples {
            sources.push(t.source.clone());
            edges.push(t.edge.clone());
            targets.push(t.target.clone());
        }

        (sources, edges, targets)
    }

    // ── Aggregation ──

    fn compute_aggregates(
        &self,
        rc: &ReturnClause,
        triples: &[MatchedTriple],
    ) -> HashMap<String, f64> {
        let mut agg = HashMap::new();

        for item in &rc.items {
            match item {
                ReturnItem::CountStar => {
                    agg.insert("count(*)".to_string(), triples.len() as f64);
                }
                ReturnItem::Count(var) => {
                    agg.insert(format!("count({})", var), triples.len() as f64);
                }
                ReturnItem::Aggregate { func, variable } => {
                    let values: Vec<f64> = triples
                        .iter()
                        .filter_map(|t| self.get_numeric_aggregate_value(variable, t))
                        .collect();

                    if values.is_empty() {
                        agg.insert(format!("{}({})", func, variable), 0.0);
                        continue;
                    }

                    let result = match func.as_str() {
                        "min" => values.iter().cloned().fold(f64::INFINITY, f64::min),
                        "max" => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                        "avg" => {
                            let sum: f64 = values.iter().sum();
                            sum / values.len() as f64
                        }
                        "sum" => values.iter().sum(),
                        _ => 0.0,
                    };

                    agg.insert(format!("{}({})", func, variable), result);
                }
                _ => {}
            }
        }

        agg
    }

    /// Extract a numeric value for aggregation from a triple variable.property.
    fn get_numeric_aggregate_value(&self, variable: &str, triple: &MatchedTriple) -> Option<f64> {
        let parts: Vec<&str> = variable.split('.').collect();
        let (var, prop) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            return None;
        };

        if var == "e" {
            match prop {
                "confidence" => triple.edge.confidence,
                _ => None,
            }
        } else {
            None
        }
    }

    // ── Helpers ──

    /// Get all nodes of a given type from the graph.
    fn get_nodes_by_type(
        &self,
        case_id: &str,
        node_type: Option<&str>,
    ) -> DbResult<Vec<GraphNode>> {
        let nt_str = node_type.map(|s| s.to_lowercase());

        if let Some(ref nt) = nt_str {
            let sql = "SELECT id, case_id, node_type, label, summary, tags, created_at FROM graph_nodes WHERE case_id = ?1 AND node_type = ?2";
            let mut stmt = self.conn.prepare(sql)?;
            let rows = stmt.query_map(params![case_id, nt], |row| {
                let tags_str: String = row.get(5)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                Ok(GraphNode {
                    id: row.get(0)?,
                    case_id: row.get(1)?,
                    node_type: parse_node_type(&row.get::<_, String>(2)?),
                    label: row.get(3)?,
                    summary: row.get(4)?,
                    tags,
                    created_at: row.get(6)?,
                })
            })?;
            let mut nodes = Vec::new();
            for row in rows {
                nodes.push(row?);
            }
            Ok(nodes)
        } else {
            // No type filter — get all nodes, but this could be expensive.
            // By default, only fetch a limited number or return empty.
            let sql = "SELECT id, case_id, node_type, label, summary, tags, created_at FROM graph_nodes WHERE case_id = ?1 LIMIT 500";
            let mut stmt = self.conn.prepare(sql)?;
            let rows = stmt.query_map(params![case_id], |row| {
                let tags_str: String = row.get(5)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                Ok(GraphNode {
                    id: row.get(0)?,
                    case_id: row.get(1)?,
                    node_type: parse_node_type(&row.get::<_, String>(2)?),
                    label: row.get(3)?,
                    summary: row.get(4)?,
                    tags,
                    created_at: row.get(6)?,
                })
            })?;
            let mut nodes = Vec::new();
            for row in rows {
                nodes.push(row?);
            }
            Ok(nodes)
        }
    }
}

// ── Type string conversions (mirrors graph_repo helpers) ──

fn node_type_str(nt: NodeType) -> &'static str {
    match nt {
        NodeType::File => "file",
        NodeType::Artifact => "artifact",
        NodeType::TimelineEvent => "timeline_event",
        NodeType::Entity => "entity",
        NodeType::Lead => "lead",
        NodeType::NotebookEntry => "notebook_entry",
    }
}

fn parse_node_type(s: &str) -> NodeType {
    match s.to_lowercase().as_str() {
        "file" => NodeType::File,
        "artifact" => NodeType::Artifact,
        "timeline_event" | "timelineevent" => NodeType::TimelineEvent,
        "entity" => NodeType::Entity,
        "lead" => NodeType::Lead,
        "notebook_entry" | "notebookentry" => NodeType::NotebookEntry,
        _ => NodeType::Entity,
    }
}

fn parse_edge_type(s: &str) -> EdgeType {
    match s.to_lowercase().as_str() {
        "contains" => EdgeType::Contains,
        "references" => EdgeType::References,
        "correlates_with" | "correlateswith" => EdgeType::CorrelatesWith,
        "derives_from" | "derivesfrom" => EdgeType::DerivesFrom,
        "precedes" => EdgeType::Precedes,
        "cites" => EdgeType::Cites,
        "annotates" => EdgeType::Annotates,
        _ => EdgeType::References,
    }
}

fn edge_type_str_upper(et: &EdgeType) -> &'static str {
    match et {
        EdgeType::Contains => "Contains",
        EdgeType::References => "References",
        EdgeType::CorrelatesWith => "CorrelatesWith",
        EdgeType::DerivesFrom => "DerivesFrom",
        EdgeType::Precedes => "Precedes",
        EdgeType::Cites => "Cites",
        EdgeType::Annotates => "Annotates",
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_sqlite::connection::open_in_memory;
    use persistence_sqlite::migrations::runner;

    fn setup() -> (&'static Connection, String) {
        let conn = Box::new(open_in_memory().unwrap());
        let conn_ref: &'static Connection = Box::leak(conn);
        runner::run_all(conn_ref).unwrap();
        let case_id = "case-gql-1".to_string();
        conn_ref
            .execute(
                "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, 'Test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                params![case_id],
            )
            .unwrap();

        // Create nodes
        let repo = GraphRepo::new(conn_ref);
        let nodes = vec![
            GraphNode {
                id: "f1".to_string(),
                case_id: case_id.clone(),
                node_type: NodeType::File,
                label: "cmd.exe".to_string(),
                summary: "Command Prompt".to_string(),
                tags: vec!["executable".to_string()],
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            GraphNode {
                id: "f2".to_string(),
                case_id: case_id.clone(),
                node_type: NodeType::File,
                label: "powershell.exe".to_string(),
                summary: "PowerShell".to_string(),
                tags: vec!["executable".to_string()],
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            GraphNode {
                id: "a1".to_string(),
                case_id: case_id.clone(),
                node_type: NodeType::Artifact,
                label: "LNK-1".to_string(),
                summary: "A shell link file".to_string(),
                tags: vec!["lnk".to_string()],
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            GraphNode {
                id: "a2".to_string(),
                case_id: case_id.clone(),
                node_type: NodeType::Artifact,
                label: "Prefetch-1".to_string(),
                summary: "A prefetch file".to_string(),
                tags: vec!["prefetch".to_string()],
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
        ];
        repo.insert_nodes_batch(&nodes).unwrap();

        let edges = vec![
            GraphEdge {
                id: "e1".to_string(),
                case_id: case_id.clone(),
                source_id: "f1".to_string(),
                target_id: "a1".to_string(),
                edge_type: EdgeType::References,
                confidence: Some(0.95),
                provenance: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            GraphEdge {
                id: "e2".to_string(),
                case_id: case_id.clone(),
                source_id: "f1".to_string(),
                target_id: "a2".to_string(),
                edge_type: EdgeType::References,
                confidence: Some(0.60),
                provenance: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            GraphEdge {
                id: "e3".to_string(),
                case_id: case_id.clone(),
                source_id: "f2".to_string(),
                target_id: "a1".to_string(),
                edge_type: EdgeType::References,
                confidence: Some(0.80),
                provenance: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
        ];
        repo.insert_edges_batch(&edges).unwrap();

        (conn_ref, case_id)
    }

    #[test]
    fn engine_match_file_to_artifact() {
        let (conn, case_id) = setup();
        let engine = GqlEngine::new(conn);
        let q = crate::parser::parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN n, e, m")
            .unwrap();
        let result = engine.execute(&case_id, &q).unwrap();
        // f1->a1, f1->a2, f2->a1 = 3 triples
        assert_eq!(result.source_nodes.len(), 3);
        assert_eq!(result.edges.len(), 3);
        assert_eq!(result.target_nodes.len(), 3);
        assert_eq!(result.total_matched, 3);
    }

    #[test]
    fn engine_where_confidence_filter() {
        let (conn, case_id) = setup();
        let engine = GqlEngine::new(conn);
        let q = crate::parser::parse(
            "MATCH (n:File)-[e:References]->(m:Artifact) WHERE e.confidence > 0.7 RETURN n, e, m",
        )
        .unwrap();
        let result = engine.execute(&case_id, &q).unwrap();
        // f1->a1 (0.95), f2->a1 (0.80) pass; f1->a2 (0.60) filtered out
        assert_eq!(result.total_matched, 2);
    }

    #[test]
    fn engine_where_label_filter() {
        let (conn, case_id) = setup();
        let engine = GqlEngine::new(conn);
        let q = crate::parser::parse(
            "MATCH (n:File)-[e:References]->(m:Artifact) WHERE n.label = 'cmd.exe' RETURN n, e, m",
        )
        .unwrap();
        let result = engine.execute(&case_id, &q).unwrap();
        // Only f1 is cmd.exe, so f1->a1, f1->a2 = 2
        assert_eq!(result.total_matched, 2);
        assert_eq!(result.source_nodes[0].label, "cmd.exe");
    }

    #[test]
    fn engine_limit() {
        let (conn, case_id) = setup();
        let engine = GqlEngine::new(conn);
        let q = crate::parser::parse(
            "MATCH (n:File)-[e:References]->(m:Artifact) RETURN n, e, m LIMIT 1",
        )
        .unwrap();
        let result = engine.execute(&case_id, &q).unwrap();
        assert_eq!(result.source_nodes.len(), 1);
        assert_eq!(result.total_matched, 3); // total before limit
    }

    #[test]
    fn engine_count_aggregate() {
        let (conn, case_id) = setup();
        let engine = GqlEngine::new(conn);
        let q = crate::parser::parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN count(*)")
            .unwrap();
        let result = engine.execute(&case_id, &q).unwrap();
        assert_eq!(result.aggregates.get("count(*)"), Some(&3.0));
    }

    #[test]
    fn engine_reverse_direction() {
        let (conn, case_id) = setup();
        let engine = GqlEngine::new(conn);
        let q = crate::parser::parse("MATCH (a:Artifact)<-[e:References]-(f:File) RETURN a, e, f")
            .unwrap();
        let result = engine.execute(&case_id, &q).unwrap();
        // Same 3 triples, just swapped perspective
        assert_eq!(result.total_matched, 3);
        // Source nodes should be artifacts
        assert!(result
            .source_nodes
            .iter()
            .all(|n| matches!(n.node_type, NodeType::Artifact)));
    }
}
