use std::collections::BTreeMap;

use chrono::Utc;
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use rusqlite::Connection;
use serde_json::Value;

use super::super::error::RulePackError;
use super::super::parser::{NodeType, RuleDefinition, RulePack};
use super::{loading, matching, projection};

pub(super) fn execute_rule(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
    rule: &RuleDefinition,
) -> Result<u64, RulePackError> {
    match rule.source_type {
        NodeType::Artifact => execute_artifact_rule(conn, case_id, pack, rule),
        NodeType::TimelineEvent
        | NodeType::File
        | NodeType::Entity
        | NodeType::Lead
        | NodeType::NotebookEntry => Ok(0),
    }
}

fn execute_artifact_rule(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
    rule: &RuleDefinition,
) -> Result<u64, RulePackError> {
    let artifacts = loading::artifacts_by_family(conn, &rule.source_family)?;
    if artifacts.is_empty() {
        return Ok(0);
    }
    let target_type: domain::NodeType = rule.target_type.clone().into();
    let targets = loading::nodes_by_type(conn, case_id, &target_type)?;
    if targets.is_empty() {
        return Ok(0);
    }
    let files = if target_type == domain::NodeType::File {
        loading::file_entries(conn)?
    } else {
        Vec::new()
    };
    projection::ensure_artifact_nodes(conn, case_id, &rule.source_family, &artifacts);
    persist_matches(conn, case_id, pack, rule, &artifacts, &targets, &files)
}

fn persist_matches(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
    rule: &RuleDefinition,
    artifacts: &[loading::ArtifactRow],
    targets: &[loading::NodeRow],
    files: &[loading::FileEntryRow],
) -> Result<u64, RulePackError> {
    let now = Utc::now().to_rfc3339();
    let mut edges = Vec::new();
    for artifact in artifacts {
        let attrs: BTreeMap<String, Value> =
            serde_json::from_str(&artifact.attrs).unwrap_or_default();
        for target in targets {
            if matching::conditions_match(&attrs, target, files, &rule.conditions) {
                edges.push(projection::project_edge(
                    case_id,
                    pack,
                    rule,
                    &artifact.id,
                    &target.id,
                    &now,
                ));
            }
        }
    }
    edges.sort_by(|left, right| left.id.cmp(&right.id));
    if edges.is_empty() {
        return Ok(0);
    }
    let count = edges.len() as u64;
    GraphRepo::new(conn)
        .insert_edges_batch(&edges)
        .map_err(|error| {
            RulePackError::Other(format!("insert rule pack edges for '{}': {error}", rule.id))
        })?;
    Ok(count)
}
