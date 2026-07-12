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
mod platform_coverage;

use rusqlite::Connection;
use std::path::Path;

use transport::dto::{
    BatchStatusDto, GraphStatsDto, NotebookStatsDto, RulePackInfoDto, RulePackStatusDto,
    V3GovernanceSnapshotDto,
};

pub use error::V3GovernanceError;
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

    let v2_toml = parser::V2_STANDARD_TOML;

    let mut loaded_packs = Vec::new();

    // Parse the built-in V2 standard rule pack
    match parser::parse_rule_pack(v2_toml) {
        Ok(pack) => {
            let rule_count = pack.rules.len() as u32;
            loaded_packs.push(RulePackInfoDto {
                name: pack.manifest.name.clone(),
                version: pack.manifest.version.clone(),
                author: pack.manifest.author.clone(),
                rule_count,
                scope: pack.manifest.scope.clone(),
            });
        }
        Err(_) => {
            // If parsing fails, report an empty pack entry so the UI can show
            // that the built-in pack is unavailable.
            loaded_packs.push(RulePackInfoDto {
                name: "v2-standard (parse error)".to_string(),
                version: "0.0.0".to_string(),
                author: "Forensics Workbench".to_string(),
                rule_count: 0,
                scope: vec!["correlation".to_string()],
            });
        }
    }

    let total_rule_count = loaded_packs.iter().map(|p| p.rule_count).sum::<u32>();

    // Execution status: the V2 standard pack is parsed at compile time and
    // executed during the correlation pipeline at import time. Since we can only
    // report on pack definition (not per-case execution state in the V3 MVP),
    // we report "loaded" here.
    let execution_status = if total_rule_count > 0 {
        "loaded"
    } else {
        "unavailable"
    };

    RulePackStatusDto {
        loaded_packs,
        total_rule_count,
        execution_status: execution_status.to_string(),
    }
}

// ── Batch Status ───────────────────────────────────────────────────────────

fn build_batch_status(
    conn: &Connection,
    case_id: &str,
) -> Result<BatchStatusDto, V3GovernanceError> {
    use crate::batch_service;

    let jobs = batch_service::list_batch_jobs(conn, case_id).unwrap_or_default();

    let active_jobs = jobs
        .iter()
        .filter(|job| job.status == "running" || job.status == "starting")
        .count() as u32;
    let completed_jobs = jobs.iter().filter(|job| job.status == "completed").count() as u32;
    let failed_jobs = jobs.iter().filter(|job| job.status == "failed").count() as u32;
    let queued_jobs = jobs.iter().filter(|job| job.status == "queued").count() as u32;
    let total_jobs = jobs.len() as u32;

    Ok(BatchStatusDto {
        active_jobs,
        completed_jobs,
        failed_jobs,
        queued_jobs,
        total_jobs,
    })
}

// ── Notebook Stats ─────────────────────────────────────────────────────────

fn build_notebook_stats(
    conn: &Connection,
    case_id: &str,
) -> Result<NotebookStatsDto, V3GovernanceError> {
    let entry_count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM notebook_entries WHERE case_id = ?1 AND status != 'deleted'",
            rusqlite::params![case_id],
            |row| row.get::<_, i64>(0).map(|v| v as u32),
        )
        .unwrap_or(0);

    let citation_count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM evidence_citations
             WHERE entry_id IN (SELECT id FROM notebook_entries WHERE case_id = ?1 AND status != 'deleted')",
            rusqlite::params![case_id],
            |row| row.get::<_, i64>(0).map(|v| v as u32),
        )
        .unwrap_or(0);

    Ok(NotebookStatsDto {
        entry_count,
        citation_count,
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../tests/unit/v3_governance_service.rs"]
mod tests;
