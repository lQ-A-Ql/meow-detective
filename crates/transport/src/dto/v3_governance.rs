//! V3 governance DTOs shared across the Tauri boundary.
//!
//! V3 extends V2 governance with graph statistics, platform coverage,
//! rule pack status, batch job status, and notebook metadata.

use serde::{Deserialize, Serialize};

use super::{
    CorrelationFamilyCoverageDto, DataSourceSummaryDto, FamilyCountDto, V2GovernanceSnapshotDto,
};

/// Purpose-built snapshot for the investigator overview screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseOverviewSnapshotDto {
    pub generated_at: String,
    pub data_sources: Vec<DataSourceSummaryDto>,
    pub timeline_event_count: u64,
    pub artifact_family_counts: Vec<FamilyCountDto>,
    pub correlation_statistics: CorrelationOverviewDto,
    pub platform_coverage: PlatformCoverageDto,
    pub rule_pack_coverage: RulePackStatusDto,
    pub batch_status: BatchStatusDto,
}

/// Correlation totals required by the overview without transferring graph rows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationOverviewDto {
    pub node_count: u32,
    pub edge_count: u32,
    pub cluster_count: u32,
    pub lead_count: u32,
    pub family_coverage: Vec<CorrelationFamilyCoverageDto>,
}

// ── V3 Governance Snapshot ────────────────────────────────────────────────

/// V3 governance snapshot: extends V2 with graph, platform, rule pack,
/// batch, and notebook fields for a richer governance dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V3GovernanceSnapshotDto {
    // ── V2 fields (backward compat) ──
    #[serde(flatten)]
    pub v2: V2GovernanceSnapshotDto,

    // ── V3 extensions ──
    /// Graph statistics from the investigative graph snapshot.
    pub graph_statistics: GraphStatsDto,
    /// Artifact family coverage by platform (Windows / Linux).
    pub platform_coverage: PlatformCoverageDto,
    /// Loaded rule packs, rule counts, and execution status.
    pub rule_pack_coverage: RulePackStatusDto,
    /// Batch job status breakdown.
    pub batch_status: BatchStatusDto,
    /// Notebook entry and citation statistics.
    pub notebook_stats: NotebookStatsDto,
}

// ── Graph Statistics ──────────────────────────────────────────────────────

/// Aggregate graph statistics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphStatsDto {
    /// Count of nodes grouped by node type (e.g. "file", "artifact").
    pub node_count_by_type: std::collections::HashMap<String, u64>,
    /// Count of edges grouped by edge type (e.g. "references", "correlates_with").
    pub edge_count_by_type: std::collections::HashMap<String, u64>,
    /// Total number of nodes in the graph.
    pub total_nodes: u64,
    /// Total number of edges in the graph.
    pub total_edges: u64,
    /// Graph density: (2 * total_edges) / (total_nodes * (total_nodes - 1)) for total_nodes > 1, else 0.
    pub density: f64,
    /// Size of the largest connected component.
    pub largest_component_size: u64,
}

// ── Platform Coverage ─────────────────────────────────────────────────────

/// Artifact family coverage broken down by target platform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCoverageDto {
    /// Number of artifact families targeting Windows.
    pub windows_artifact_families: u32,
    /// Number of artifact families targeting Linux.
    pub linux_artifact_families: u32,
    /// Number of artifact families that are cross-platform.
    pub cross_platform_artifact_families: u32,
    /// Number of families that have no production platform classification.
    pub unknown_artifact_families: u32,
    /// Total number of distinct artifact families.
    pub total_families: u32,
    /// List of Windows artifact family names.
    pub windows_families: Vec<String>,
    /// List of Linux artifact family names.
    pub linux_families: Vec<String>,
    /// List of cross-platform artifact family names.
    pub cross_platform_families: Vec<String>,
    /// List of artifact family names that require platform classification review.
    pub unknown_families: Vec<String>,
}

// ── Rule Pack Status ──────────────────────────────────────────────────────

/// Status of a loaded rule pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RulePackInfoDto {
    /// Rule pack name (from manifest).
    pub name: String,
    /// Semantic version of the rule pack.
    pub version: String,
    /// Pack author or organisation.
    pub author: String,
    /// Number of rules defined in this pack.
    pub rule_count: u32,
    /// Usage scopes (e.g. "correlation", "investigation").
    pub scope: Vec<String>,
}

/// Aggregated rule pack coverage status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RulePackStatusDto {
    /// Individual rule pack info entries.
    pub loaded_packs: Vec<RulePackInfoDto>,
    /// Total number of rules across all loaded packs.
    pub total_rule_count: u32,
    /// Status of parsing and loading the rule-pack definition.
    pub load_status: String,
    /// Overall execution status: "executed", "not_executed", "partial".
    pub execution_status: String,
}

// ── Batch Status ──────────────────────────────────────────────────────────

/// Batch job status breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BatchStatusDto {
    /// Number of currently active (running) batch jobs.
    pub active_jobs: u32,
    /// Number of successfully completed batch jobs.
    pub completed_jobs: u32,
    /// Number of failed batch jobs.
    pub failed_jobs: u32,
    /// Number of queued (not yet started) batch jobs.
    pub queued_jobs: u32,
    /// Total number of batch jobs in the system.
    pub total_jobs: u32,
}

// ── Notebook Stats ────────────────────────────────────────────────────────

/// Notebook entry and citation statistics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotebookStatsDto {
    /// Total number of notebook entries for the case.
    pub entry_count: u32,
    /// Total number of evidence citations across all entries.
    pub citation_count: u32,
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../tests/unit/dto/v3_governance.rs"]
mod tests;
