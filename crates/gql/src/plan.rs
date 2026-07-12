//! Query plan and cost estimation for GQL queries.
//!
//! Produces an estimated execution plan showing traversal steps,
//! estimated node/edge counts, and index usage before execution.

use crate::parser::*;
use serde::{Deserialize, Serialize};

/// The cost of executing a query step (estimated).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    /// Estimated number of database operations.
    pub estimated_ops: u64,
    /// Estimated number of nodes visited during traversal.
    pub nodes_visited: u64,
    /// Estimated number of edges traversed.
    pub edges_traversed: u64,
}

/// A single step in the query execution plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlanStep {
    /// Step number (1-indexed).
    pub step: u32,
    /// Description of what this step does.
    pub description: String,
    /// Type of operation (scan, filter, project, aggregate, limit).
    pub operation: String,
    /// Estimated cost details for this step.
    pub estimated_cost: Cost,
    /// Whether this step uses an index.
    pub uses_index: bool,
    /// Name of the index used, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_name: Option<String>,
}

/// The full query execution plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlan {
    /// Ordered execution steps.
    pub steps: Vec<QueryPlanStep>,
    /// Total estimated cost across all steps.
    pub total_cost: Cost,
    /// Whether all steps can use indexes.
    pub fully_indexed: bool,
    /// Human-readable summary.
    pub summary: String,
}

/// Estimate the execution plan for a parsed GQL query.
///
/// This builds a plan based on the MATCH pattern, WHERE filters,
/// RETURN projection, and LIMIT clause. The cost estimates use
/// heuristics based on expected graph sizes and index availability.
pub fn estimate_plan(query: &Query) -> QueryPlan {
    let mut steps = Vec::new();
    let mc = &query.match_clause;

    // ── Step 1: Source node scan ──
    let has_source_type = mc.source_type.is_some();
    let has_source_index = has_source_type;
    let source_scan_cost = if has_source_type {
        // Index on node_type narrows the scan
        Cost {
            estimated_ops: 50,
            nodes_visited: 50,
            edges_traversed: 0,
        }
    } else {
        // Full scan of all nodes
        Cost {
            estimated_ops: 500,
            nodes_visited: 500,
            edges_traversed: 0,
        }
    };

    steps.push(QueryPlanStep {
        step: 1,
        description: format!(
            "Scan graph_nodes for source nodes{}",
            if let Some(ref t) = mc.source_type {
                format!(" WHERE node_type = '{}'", t.to_lowercase())
            } else {
                String::new()
            }
        ),
        operation: "scan".to_string(),
        estimated_cost: source_scan_cost,
        uses_index: has_source_index,
        index_name: if has_source_index {
            Some("idx_graph_nodes_type".to_string())
        } else {
            None
        },
    });

    // ── Step 2: Edge traversal ──
    let has_edge_type = mc.edge_type.is_some();
    let has_edge_index = true; // idx_graph_edges_source and idx_graph_edges_type exist
    let estimated_edges_per_node: u64 = if has_edge_type { 1 } else { 3 };
    let source_nodes = source_scan_cost.nodes_visited;
    let edge_traversal_cost = Cost {
        estimated_ops: source_nodes * 2,
        nodes_visited: 0, // counting edges here
        edges_traversed: source_nodes * estimated_edges_per_node,
    };

    steps.push(QueryPlanStep {
        step: 2,
        description: format!(
            "Traverse edges from source nodes{} direction={}",
            if let Some(ref t) = mc.edge_type {
                format!(" WHERE edge_type = '{}'", t.to_lowercase())
            } else {
                String::new()
            },
            match mc.direction {
                MatchDirection::LeftToRight => "outgoing",
                MatchDirection::RightToLeft => "incoming",
            }
        ),
        operation: "traverse".to_string(),
        estimated_cost: edge_traversal_cost,
        uses_index: has_edge_index,
        index_name: Some(if matches!(mc.direction, MatchDirection::LeftToRight) {
            "idx_graph_edges_source".to_string()
        } else {
            "idx_graph_edges_target".to_string()
        }),
    });

    // ── Step 3: Target node type filter ──
    let mut total_triples = edge_traversal_cost.edges_traversed;
    if let Some(target_type) = &mc.target_type {
        let target_filter_cost = Cost {
            estimated_ops: total_triples,
            nodes_visited: total_triples,
            edges_traversed: 0,
        };
        steps.push(QueryPlanStep {
            step: 3,
            description: format!(
                "Filter target nodes by node_type = '{}'",
                target_type.to_lowercase()
            ),
            operation: "filter".to_string(),
            estimated_cost: target_filter_cost,
            uses_index: false,
            index_name: None,
        });
        // Estimated that 50% of neighbors match the target type
        total_triples /= 2;
    }

    // ── Step 4: WHERE clause filter ──
    let mut step_num = if mc.target_type.is_some() { 4 } else { 3 };
    if let Some(ref wc) = query.where_clause {
        let where_cost = Cost {
            estimated_ops: total_triples,
            nodes_visited: total_triples,
            edges_traversed: 0,
        };
        let pred_descs: Vec<String> = wc
            .predicates
            .iter()
            .map(|p| format!("{}.{} {} {}", p.variable, p.property, p.operator, p.value))
            .collect();

        let filter_selectivity: f64 = match wc.connector {
            LogicalConnector::And => 0.3_f64.powi(wc.predicates.len() as i32),
            LogicalConnector::Or => 0.3 * wc.predicates.len() as f64,
        };
        total_triples = (total_triples as f64 * filter_selectivity).ceil() as u64;

        steps.push(QueryPlanStep {
            step: step_num,
            description: format!(
                "Apply WHERE predicates: {}",
                pred_descs.join(if wc.connector == LogicalConnector::And {
                    " AND "
                } else {
                    " OR "
                })
            ),
            operation: "filter".to_string(),
            estimated_cost: where_cost,
            uses_index: false,
            index_name: None,
        });
        step_num += 1;
    }

    // ── Step 5: Project RETURN items ──
    let project_cost = Cost {
        estimated_ops: total_triples,
        nodes_visited: 0,
        edges_traversed: 0,
    };
    let return_descs: Vec<String> = query
        .return_clause
        .items
        .iter()
        .map(|item| match item {
            ReturnItem::Variable(v) => v.clone(),
            ReturnItem::CountStar => "count(*)".to_string(),
            ReturnItem::Count(v) => format!("count({})", v),
            ReturnItem::Aggregate { func, variable } => format!("{}({})", func, variable),
        })
        .collect();

    steps.push(QueryPlanStep {
        step: step_num,
        description: format!("Project RETURN items: {}", return_descs.join(", ")),
        operation: "project".to_string(),
        estimated_cost: project_cost,
        uses_index: false,
        index_name: None,
    });

    let mut after_project = total_triples;
    // If only aggregates, the result is a single row
    let has_only_aggregates = query
        .return_clause
        .items
        .iter()
        .all(|item| !matches!(item, ReturnItem::Variable(_)));
    if has_only_aggregates {
        after_project = 1;
    }

    // ── Step 6: LIMIT ──
    if let Some(limit) = query.limit {
        step_num += 1;
        let limit_cost = Cost {
            estimated_ops: limit as u64,
            nodes_visited: 0,
            edges_traversed: 0,
        };
        steps.push(QueryPlanStep {
            step: step_num,
            description: format!("Apply LIMIT {}", limit),
            operation: "limit".to_string(),
            estimated_cost: limit_cost,
            uses_index: false,
            index_name: None,
        });
    }

    // ── Total cost ──
    let total_ops: u64 = steps.iter().map(|s| s.estimated_cost.estimated_ops).sum();
    let total_nodes: u64 = steps.iter().map(|s| s.estimated_cost.nodes_visited).sum();
    let total_edges: u64 = steps.iter().map(|s| s.estimated_cost.edges_traversed).sum();
    let total_cost = Cost {
        estimated_ops: total_ops,
        nodes_visited: total_nodes,
        edges_traversed: total_edges,
    };

    let fully_indexed = steps.iter().all(|s| {
        s.uses_index
            || s.operation == "filter"
            || s.operation == "project"
            || s.operation == "limit"
    });

    let summary = format!(
        "Estimated {} ops, visiting ~{} nodes and ~{} edges. {} indexes used. Projecting ~{} result rows.",
        total_ops,
        total_nodes,
        total_edges,
        if fully_indexed { "All" } else { "Partial" },
        after_project,
    );

    QueryPlan {
        steps,
        total_cost,
        fully_indexed,
        summary,
    }
}

// ── Tests ──

#[cfg(test)]
#[path = "../tests/unit/plan.rs"]
mod tests;
