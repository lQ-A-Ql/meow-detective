//! V3 governance snapshot service.
//!
//! Extends V2 governance with graph statistics, platform coverage,
//! rule pack status, batch job status, and notebook metadata.
//!
//! # Design
//!
//! - Reuses `v2_governance_service::get_v2_governance_snapshot` for all V2 fields
//!   (backward compatibility).
//! - Adds V3-specific fields: graph statistics, platform coverage, rule pack status,
//!   batch job breakdown, and notebook statistics.
//! - All queries are read-only and operate against the active case database.

mod artifact_family_platform;
mod error;
mod overview;
mod platform_coverage;

use rusqlite::Connection;
use std::path::Path;

use transport::dto::{
    BatchStatusDto, GraphStatsDto, NotebookStatsDto, RulePackInfoDto, RulePackStatusDto,
    V3GovernanceSnapshotDto,
};

pub use error::V3GovernanceError;
pub use overview::get_case_overview_snapshot_for_case;
use platform_coverage::{
    apply_platform_integrity_gate, build_platform_coverage, build_platform_coverage_for_case,
};

// ── Public API ─────────────────────────────────────────────────────────────

/// Build a V3 governance snapshot for the given case.
///
/// Combines the full V2 governance snapshot with V3-specific extensions:
/// graph statistics, platform coverage, rule pack status, batch job status,
/// and notebook statistics.
pub fn get_v3_governance_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<V3GovernanceSnapshotDto, V3GovernanceError> {
    let v2 = crate::v2_governance_service::get_v2_governance_snapshot(conn, case_id)?;

    let graph_statistics = build_graph_stats(conn, case_id)?;
    let platform_coverage = build_platform_coverage(conn)?;
    let rule_pack_coverage = build_rule_pack_status();
    let batch_status = build_batch_status(conn, case_id)?;
    let notebook_stats = build_notebook_stats(conn, case_id)?;

    Ok(V3GovernanceSnapshotDto {
        v2,
        graph_statistics,
        platform_coverage,
        rule_pack_coverage,
        batch_status,
        notebook_stats,
    })
}

pub fn get_v3_governance_snapshot_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<V3GovernanceSnapshotDto, V3GovernanceError> {
    let mut v2 = crate::v2_governance_service::get_v2_governance_snapshot_for_case(
        conn, case_root, case_id,
    )?;

    let graph_statistics = build_graph_stats_for_case(conn, case_root, case_id)?;
    let platform_assessment =
        build_platform_coverage_for_case(conn, case_root, &domain::CaseId(case_id.to_string()))?;
    apply_platform_integrity_gate(&mut v2, &platform_assessment);
    let platform_coverage = platform_assessment.coverage;
    let rule_pack_coverage = build_rule_pack_status();
    let batch_status = build_batch_status(conn, case_id)?;
    let notebook_stats = build_notebook_stats(conn, case_id)?;

    Ok(V3GovernanceSnapshotDto {
        v2,
        graph_statistics,
        platform_coverage,
        rule_pack_coverage,
        batch_status,
        notebook_stats,
    })
}

// ── Graph Statistics ───────────────────────────────────────────────────────

fn build_graph_stats(conn: &Connection, case_id: &str) -> Result<GraphStatsDto, V3GovernanceError> {
    let graph_snapshot = crate::graph_service::get_graph_snapshot(conn, case_id)?;
    Ok(GraphStatsDto {
        node_count_by_type: graph_snapshot.node_count_by_type,
        edge_count_by_type: graph_snapshot.edge_count_by_type,
        total_nodes: graph_snapshot.total_nodes,
        total_edges: graph_snapshot.total_edges,
        density: graph_snapshot.density,
        largest_component_size: graph_snapshot.largest_component_size,
    })
}

fn build_graph_stats_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<GraphStatsDto, V3GovernanceError> {
    let graph_snapshot =
        crate::graph_service::get_graph_snapshot_for_case(conn, case_root, case_id)?;
    Ok(GraphStatsDto {
        node_count_by_type: graph_snapshot.node_count_by_type,
        edge_count_by_type: graph_snapshot.edge_count_by_type,
        total_nodes: graph_snapshot.total_nodes,
        total_edges: graph_snapshot.total_edges,
        density: graph_snapshot.density,
        largest_component_size: graph_snapshot.largest_component_size,
    })
}

// ── Rule Pack Status ───────────────────────────────────────────────────────

fn build_rule_pack_status() -> RulePackStatusDto {
    use crate::rule_pack::parser;

    build_rule_pack_status_from(parser::V2_STANDARD_TOML)
}

fn build_rule_pack_status_from(source: &str) -> RulePackStatusDto {
    use crate::rule_pack::parser;

    let mut loaded_packs = Vec::new();
    let mut load_status = "unavailable";

    match parser::parse_rule_pack(source) {
        Ok(pack) => {
            load_status = "loaded";
            let rule_count = pack.rules.len() as u32;
            loaded_packs.push(RulePackInfoDto {
                name: pack.manifest.name.clone(),
                version: pack.manifest.version.clone(),
                author: pack.manifest.author.clone(),
                rule_count,
                scope: pack.manifest.scope.clone(),
            });
        }
        Err(error) => {
            tracing::error!(?error, "built-in rule-pack definition could not be loaded");
        }
    }

    let total_rule_count = loaded_packs.iter().map(|p| p.rule_count).sum::<u32>();

    // The built-in pack is currently a definition-only capability. No persisted
    // per-case rule-pack run record exists, so never present a loaded definition
    // as if it had executed for the active case.
    let execution_status = if total_rule_count > 0 {
        "not_executed"
    } else {
        "unavailable"
    };

    RulePackStatusDto {
        loaded_packs,
        total_rule_count,
        load_status: load_status.to_string(),
        execution_status: execution_status.to_string(),
    }
}

// ── Batch Status ───────────────────────────────────────────────────────────

fn build_batch_status(
    conn: &Connection,
    case_id: &str,
) -> Result<BatchStatusDto, V3GovernanceError> {
    let counts = persistence_sqlite::repositories::batch_repo::BatchRepo::new(conn)
        .count_jobs_by_status(case_id)
        .map_err(|error| V3GovernanceError::Other(format!("count batch jobs: {error}")))?;

    Ok(BatchStatusDto {
        active_jobs: counts.active_jobs,
        completed_jobs: counts.completed_jobs,
        failed_jobs: counts.failed_jobs,
        queued_jobs: counts.queued_jobs,
        total_jobs: counts.total_jobs,
    })
}

// ── Notebook Stats ─────────────────────────────────────────────────────────

fn build_notebook_stats(
    conn: &Connection,
    case_id: &str,
) -> Result<NotebookStatsDto, V3GovernanceError> {
    let repo = persistence_sqlite::repositories::notebook_repo::NotebookRepo::new(conn);
    let entry_count = repo
        .count_active_entries_for_case(case_id)
        .map_err(|error| V3GovernanceError::Other(format!("count notebook entries: {error}")))?;
    let citation_count = repo
        .count_citations_for_case(case_id)
        .map_err(|error| V3GovernanceError::Other(format!("count notebook citations: {error}")))?;

    Ok(NotebookStatsDto {
        entry_count,
        citation_count,
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../tests/unit/v3_governance_service.rs"]
mod tests;
