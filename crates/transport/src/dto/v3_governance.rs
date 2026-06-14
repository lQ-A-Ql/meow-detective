//! V3 governance DTOs shared across the Tauri boundary.
//!
//! V3 extends V2 governance with graph statistics, platform coverage,
//! rule pack status, batch job status, and notebook metadata.

use serde::{Deserialize, Serialize};

use super::V2GovernanceSnapshotDto;

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
    /// Artifact family coverage by platform (Windows / Linux / macOS).
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
    /// Number of artifact families targeting macOS.
    pub macos_artifact_families: u32,
    /// Total number of distinct artifact families.
    pub total_families: u32,
    /// List of Windows artifact family names.
    pub windows_families: Vec<String>,
    /// List of Linux artifact family names.
    pub linux_families: Vec<String>,
    /// List of macOS artifact family names.
    pub macos_families: Vec<String>,
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
mod tests {
    use super::*;
    use crate::dto::{
        BenchmarkSummaryDto, GovernanceFactSourceDto, GovernanceRuntimeResultsDto,
        GovernanceRuntimeSignalsDto, ParserSupportMatrixSummaryDto, ReleaseScorecardDto,
        SecurityAuditSummaryDto,
    };

    fn make_v2_snapshot() -> V2GovernanceSnapshotDto {
        V2GovernanceSnapshotDto {
            generated_at: "2026-06-14T00:00:00Z".to_string(),
            fact_sources: vec![GovernanceFactSourceDto {
                area: "verification".to_string(),
                fact_file: "v2-verification-catalog.json".to_string(),
                fact_kind: "catalog".to_string(),
                derived_outputs: vec!["verificationChains".to_string()],
                last_verified_at: "2026-06-12T00:00:00Z".to_string(),
            }],
            runtime_results: GovernanceRuntimeResultsDto {
                checked_at: "2026-06-14T00:00:00Z".to_string(),
                checks: vec![],
            },
            verification_chains: vec![],
            support_matrix: ParserSupportMatrixSummaryDto {
                ga_count: 5,
                beta_count: 3,
                experimental_count: 2,
                unsupported_count: 0,
                documented_limit_count: 18,
            },
            support_matrix_entries: vec![],
            known_limitations: vec![],
            benchmark: BenchmarkSummaryDto {
                host_profile: "test".to_string(),
                baseline_version: "1.0".to_string(),
                last_verified_at: "2026-06-14T00:00:00Z".to_string(),
                scenarios: vec![],
                required_checks: vec![],
                covered_required_count: 0,
                missing_required_count: 0,
                exceeded_required_count: 0,
            },
            security: SecurityAuditSummaryDto {
                export_overwrite_default: false,
                export_path_guard_enabled: true,
                stdio_command_whitelist_enforced: true,
                sse_https_only: true,
                embedded_credentials_blocked: true,
                media_handle_scoped: true,
                error_redaction_enabled: true,
                audit_log_required: true,
                audit_event_count: 10,
                sensitive_audit_event_count: 2,
                recent_audit_entries: vec![],
                notes: vec![],
            },
            error_taxonomy_entries: vec![],
            release_gates: vec![],
            release_scorecard: ReleaseScorecardDto {
                total_score: 85,
                grade: "B".to_string(),
                verification_score: 20,
                correlation_score: 20,
                performance_score: 20,
                security_score: 25,
                breakdown: vec![],
                blockers: vec![],
                residual_risks: vec![],
            },
            runtime_signals: GovernanceRuntimeSignalsDto {
                data_source_count: 1,
                hashed_data_source_count: 1,
                pending_hash_data_source_count: 0,
                warning_data_source_count: 0,
                running_job_count: 0,
                partial_job_count: 0,
                failed_job_count: 0,
                report_count: 0,
                correlation_snapshot_available: false,
                correlation_lead_count: 0,
                correlation_high_confidence_lead_count: 0,
                correlation_review_lead_count: 0,
                correlation_cluster_count: 0,
                correlation_rule_family_count: 8,
                correlation_covered_family_count: 0,
                correlation_high_confidence_family_count: 0,
                correlation_family_coverage: vec![],
            },
        }
    }

    #[test]
    fn v3_snapshot_serializes_camel_case() {
        let mut node_count_by_type = std::collections::HashMap::new();
        node_count_by_type.insert("file".to_string(), 42);

        let mut edge_count_by_type = std::collections::HashMap::new();
        edge_count_by_type.insert("references".to_string(), 30);

        let v3 = V3GovernanceSnapshotDto {
            v2: make_v2_snapshot(),
            graph_statistics: GraphStatsDto {
                node_count_by_type,
                edge_count_by_type,
                total_nodes: 57,
                total_edges: 50,
                density: 0.0313,
                largest_component_size: 40,
            },
            platform_coverage: PlatformCoverageDto {
                windows_artifact_families: 8,
                linux_artifact_families: 0,
                macos_artifact_families: 0,
                total_families: 8,
                windows_families: vec!["LNK".to_string(), "Prefetch".to_string()],
                linux_families: vec![],
                macos_families: vec![],
            },
            rule_pack_coverage: RulePackStatusDto {
                loaded_packs: vec![RulePackInfoDto {
                    name: "v2-standard".to_string(),
                    version: "1.0.0".to_string(),
                    author: "Forensics Workbench".to_string(),
                    rule_count: 10,
                    scope: vec!["correlation".to_string()],
                }],
                total_rule_count: 10,
                execution_status: "executed".to_string(),
            },
            batch_status: BatchStatusDto {
                active_jobs: 1,
                completed_jobs: 3,
                failed_jobs: 0,
                queued_jobs: 0,
                total_jobs: 4,
            },
            notebook_stats: NotebookStatsDto {
                entry_count: 5,
                citation_count: 12,
            },
        };

        let json = serde_json::to_value(&v3).unwrap();

        // V2 fields flattened
        assert_eq!(json["generatedAt"], "2026-06-14T00:00:00Z");
        assert_eq!(json["factSources"][0]["area"], "verification");

        // V3 extensions
        assert_eq!(json["graphStatistics"]["totalNodes"], 57);
        assert_eq!(json["graphStatistics"]["density"], 0.0313);
        assert_eq!(json["graphStatistics"]["nodeCountByType"]["file"], 42);

        assert_eq!(json["platformCoverage"]["windowsArtifactFamilies"], 8);
        assert_eq!(json["platformCoverage"]["windowsFamilies"][0], "LNK");

        assert_eq!(json["rulePackCoverage"]["totalRuleCount"], 10);
        assert_eq!(
            json["rulePackCoverage"]["loadedPacks"][0]["name"],
            "v2-standard"
        );

        assert_eq!(json["batchStatus"]["activeJobs"], 1);
        assert_eq!(json["batchStatus"]["totalJobs"], 4);

        assert_eq!(json["notebookStats"]["entryCount"], 5);
        assert_eq!(json["notebookStats"]["citationCount"], 12);
    }

    #[test]
    fn empty_v3_snapshot_serializes_without_errors() {
        let v3 = V3GovernanceSnapshotDto {
            v2: make_v2_snapshot(),
            graph_statistics: GraphStatsDto {
                node_count_by_type: std::collections::HashMap::new(),
                edge_count_by_type: std::collections::HashMap::new(),
                total_nodes: 0,
                total_edges: 0,
                density: 0.0,
                largest_component_size: 0,
            },
            platform_coverage: PlatformCoverageDto {
                windows_artifact_families: 0,
                linux_artifact_families: 0,
                macos_artifact_families: 0,
                total_families: 0,
                windows_families: vec![],
                linux_families: vec![],
                macos_families: vec![],
            },
            rule_pack_coverage: RulePackStatusDto {
                loaded_packs: vec![],
                total_rule_count: 0,
                execution_status: "not_executed".to_string(),
            },
            batch_status: BatchStatusDto {
                active_jobs: 0,
                completed_jobs: 0,
                failed_jobs: 0,
                queued_jobs: 0,
                total_jobs: 0,
            },
            notebook_stats: NotebookStatsDto {
                entry_count: 0,
                citation_count: 0,
            },
        };

        let json = serde_json::to_value(&v3).unwrap();
        assert_eq!(json["graphStatistics"]["totalNodes"], 0);
        assert_eq!(json["platformCoverage"]["totalFamilies"], 0);
        assert_eq!(json["notebookStats"]["entryCount"], 0);
    }
}
