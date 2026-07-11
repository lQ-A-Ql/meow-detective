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

use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

use transport::dto::{
    BatchStatusDto, GraphStatsDto, NotebookStatsDto, PlatformCoverageDto, RulePackInfoDto,
    RulePackStatusDto, V3GovernanceSnapshotDto,
};

#[derive(Debug, Error)]
pub enum V3GovernanceError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for V3GovernanceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }
}

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
    let v2 = crate::v2_governance_service::get_v2_governance_snapshot(conn, case_id)
        .map_err(|e| V3GovernanceError::Other(e.to_string()))?;

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
    let v2 =
        crate::v2_governance_service::get_v2_governance_snapshot_for_case(conn, case_root, case_id)
            .map_err(|e| V3GovernanceError::Other(e.to_string()))?;

    let graph_statistics = build_graph_stats_for_case(conn, case_root, case_id)?;
    let platform_coverage =
        build_platform_coverage_for_case(conn, case_root, &domain::CaseId(case_id.to_string()))?;
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
    let graph_snapshot = crate::graph_service::get_graph_snapshot(conn, case_id)
        .map_err(|e| V3GovernanceError::Other(e.to_string()))?;
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
        crate::graph_service::get_graph_snapshot_for_case(conn, case_root, case_id)
            .map_err(|e| V3GovernanceError::Other(e.to_string()))?;
    Ok(GraphStatsDto {
        node_count_by_type: graph_snapshot.node_count_by_type,
        edge_count_by_type: graph_snapshot.edge_count_by_type,
        total_nodes: graph_snapshot.total_nodes,
        total_edges: graph_snapshot.total_edges,
        density: graph_snapshot.density,
        largest_component_size: graph_snapshot.largest_component_size,
    })
}

// ── Platform Coverage ──────────────────────────────────────────────────────

/// Map artifact family names to target platforms.
///
/// Unknown families default to Windows since this is a Windows-first product.
fn get_platform(family: &str) -> &'static str {
    // Windows artifact families (Windows-only forensics artifacts)
    let windows_families: &[&str] = &[
        "LNK",
        "Prefetch",
        "JumpList",
        "RegistryValue",
        "Registry",
        "RecycleBin",
        "EVTX",
        "SRU",
        "Thumbcache",
        "ShellBag",
        "AmCache",
        "USBDevice",
        "EventLog",
        "AppCompatCache",
        "MUICache",
        "UserAssist",
        "BAM",
        "ShimCache",
        "NetworkList",
        "ScheduledTask",
        "Service",
        "Startup",
        "MRU",
        "RunMRU",
        "LastVisitedMRU",
        "OfficeMRU",
        "TypedPaths",
        "RecentDocs",
        "ComDlg32",
        "WordWheelQuery",
        "BagMRU",
        "MFT",
        "UsnJrnl",
        "LogonSession",
        "RDPClient",
        "PowerShellHistory",
        "CmdHistory",
    ];
    // Cross-platform artifact families (browsers, email)
    let cross_platform_families: &[&str] = &[
        "BrowserDownload",
        "BrowserHistory",
        "BrowserVisit",
        "BrowserCookie",
        "BrowserSessionTab",
        "EmailMessage",
    ];
    let linux_families: &[&str] = &[
        "BashHistory",
        "AuthLog",
        "Syslog",
        "Journald",
        "CronLog",
        "PacmanLog",
        "AptHistory",
        "DpkgLog",
    ];
    if windows_families
        .iter()
        .any(|f| f.eq_ignore_ascii_case(family))
    {
        "windows"
    } else if cross_platform_families
        .iter()
        .any(|f| f.eq_ignore_ascii_case(family))
    {
        "cross-platform"
    } else if linux_families
        .iter()
        .any(|f| f.eq_ignore_ascii_case(family))
    {
        "linux"
    } else {
        // Default to Windows for unknown families (Windows-first product)
        "windows"
    }
}

fn build_platform_coverage(conn: &Connection) -> Result<PlatformCoverageDto, V3GovernanceError> {
    use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;

    let family_counts = ArtifactRepo::new(conn)
        .count_by_family()
        .map_err(|e| V3GovernanceError::Other(e.to_string()))?;

    let mut windows_families: Vec<String> = Vec::new();
    let mut linux_families: Vec<String> = Vec::new();
    let mut cross_platform_families: Vec<String> = Vec::new();

    for (family, _count) in &family_counts {
        match get_platform(family) {
            "windows" => windows_families.push(family.clone()),
            "linux" => linux_families.push(family.clone()),
            "cross-platform" => cross_platform_families.push(family.clone()),
            _ => windows_families.push(family.clone()),
        }
    }

    Ok(PlatformCoverageDto {
        windows_artifact_families: windows_families.len() as u32,
        linux_artifact_families: linux_families.len() as u32,
        cross_platform_artifact_families: cross_platform_families.len() as u32,
        total_families: family_counts.len() as u32,
        windows_families,
        linux_families,
        cross_platform_families,
    })
}

fn build_platform_coverage_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<PlatformCoverageDto, V3GovernanceError> {
    let family_counts =
        crate::artifact_service::get_artifact_family_counts_for_case(conn, case_root, case_id)
            .map_err(|e| V3GovernanceError::Other(e.to_string()))?;

    let mut windows_families: Vec<String> = Vec::new();
    let mut linux_families: Vec<String> = Vec::new();
    let mut cross_platform_families: Vec<String> = Vec::new();

    for family_count in &family_counts {
        match get_platform(&family_count.family) {
            "windows" => windows_families.push(family_count.family.clone()),
            "linux" => linux_families.push(family_count.family.clone()),
            "cross-platform" => cross_platform_families.push(family_count.family.clone()),
            _ => windows_families.push(family_count.family.clone()),
        }
    }

    Ok(PlatformCoverageDto {
        windows_artifact_families: windows_families.len() as u32,
        linux_artifact_families: linux_families.len() as u32,
        cross_platform_artifact_families: cross_platform_families.len() as u32,
        total_families: family_counts.len() as u32,
        windows_families,
        linux_families,
        cross_platform_families,
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
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::{
        CaseId, CaseMeta, DataSource, DataSourceHashStatus, DataSourceKind, DataSourceProvenance,
        DataSourceProvenanceStatus, EntryStatus, EvidenceCitation, NodeType, NotebookEntry,
        NotebookEntryType,
    };
    use persistence_sqlite::repositories::{
        case_repo::CaseRepo, datasource_repo::DataSourceRepo, notebook_repo::NotebookRepo,
    };
    use rusqlite::Connection;

    fn setup_case_db(case_id: &str) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        CaseRepo::new(&conn)
            .create(&CaseMeta {
                id: CaseId(case_id.to_string()),
                name: "V3 Governance Test Case".to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        conn
    }

    fn add_datasource(conn: &Connection, case_id: &str, ds_id: &str, name: &str) {
        DataSourceRepo::new(conn)
            .insert(
                &CaseId(case_id.to_string()),
                &DataSource {
                    id: domain::DataSourceId(ds_id.to_string()),
                    name: name.to_string(),
                    kind: DataSourceKind::Raw,
                    source_path: std::path::PathBuf::from(format!("C:/evidence/{name}")),
                    imported_at: Utc::now(),
                    provenance: DataSourceProvenance {
                        source_hash_sha256: Some("abc".to_string()),
                        hash_status: DataSourceHashStatus::Hashed,
                        canonical_source_path: None,
                        evidence_size: Some(1024),
                        reader_kind: Some("raw".to_string()),
                        provenance_status: DataSourceProvenanceStatus::Recorded,
                        warnings: Vec::new(),
                    },
                },
            )
            .unwrap();
    }

    fn add_notebook_entry(conn: &Connection, case_id: &str, id: &str, title: &str) {
        let repo = NotebookRepo::new(conn);
        repo.create_entry(&NotebookEntry {
            id: id.to_string(),
            case_id: case_id.to_string(),
            parent_id: None,
            author: "investigator".to_string(),
            entry_type: NotebookEntryType::Finding,
            title: title.to_string(),
            body_markdown: "body".to_string(),
            tags: vec![],
            status: EntryStatus::Draft,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();
    }

    fn add_citation(conn: &Connection, entry_id: &str, citation_id: &str, target_node_id: &str) {
        let repo = NotebookRepo::new(conn);
        repo.add_citation(&EvidenceCitation {
            id: citation_id.to_string(),
            entry_id: entry_id.to_string(),
            target_node_type: NodeType::File,
            target_node_id: target_node_id.to_string(),
            display_label: "test citation".to_string(),
            snippet: None,
            cited_at: Utc::now().to_rfc3339(),
        })
        .unwrap();
    }

    #[test]
    fn v3_snapshot_includes_graph_statistics() {
        let conn = setup_case_db("case-graph");
        add_datasource(&conn, "case-graph", "ds-1", "disk.raw");

        let snapshot = get_v3_governance_snapshot(&conn, "case-graph").unwrap();

        // Graph statistics should be present (likely empty for fresh case)
        assert_eq!(snapshot.graph_statistics.total_nodes, 0);
        assert_eq!(snapshot.graph_statistics.total_edges, 0);
        assert_eq!(snapshot.graph_statistics.density, 0.0);
        assert_eq!(snapshot.graph_statistics.largest_component_size, 0);
    }

    #[test]
    fn v3_snapshot_includes_platform_coverage() {
        let conn = setup_case_db("case-platform");
        add_datasource(&conn, "case-platform", "ds-1", "disk.raw");

        let snapshot = get_v3_governance_snapshot(&conn, "case-platform").unwrap();

        // Platform coverage should be present (may be 0 for empty artifacts)
        assert_eq!(snapshot.platform_coverage.total_families, 0);
        // But the field itself must exist
        assert_eq!(snapshot.platform_coverage.linux_artifact_families, 0);
    }

    #[test]
    fn v3_snapshot_includes_rule_pack_status() {
        let conn = setup_case_db("case-rules");
        add_datasource(&conn, "case-rules", "ds-1", "disk.raw");

        let snapshot = get_v3_governance_snapshot(&conn, "case-rules").unwrap();

        // Rule pack should be parsed from the built-in V2_STANDARD_TOML
        assert_eq!(snapshot.rule_pack_coverage.loaded_packs.len(), 1);
        assert_eq!(
            snapshot.rule_pack_coverage.loaded_packs[0].name,
            "v2-standard"
        );
        assert_eq!(snapshot.rule_pack_coverage.loaded_packs[0].rule_count, 10);
        assert_eq!(snapshot.rule_pack_coverage.total_rule_count, 10);
    }

    #[test]
    fn v3_snapshot_includes_batch_status() {
        let conn = setup_case_db("case-batch");
        add_datasource(&conn, "case-batch", "ds-1", "disk.raw");

        let snapshot = get_v3_governance_snapshot(&conn, "case-batch").unwrap();

        // Batch status should be present (empty for fresh case)
        assert_eq!(snapshot.batch_status.total_jobs, 0);
        assert_eq!(snapshot.batch_status.active_jobs, 0);
        assert_eq!(snapshot.batch_status.completed_jobs, 0);
        assert_eq!(snapshot.batch_status.failed_jobs, 0);
        assert_eq!(snapshot.batch_status.queued_jobs, 0);
    }

    #[test]
    fn v3_snapshot_includes_notebook_stats() {
        let conn = setup_case_db("case-notebook");
        add_datasource(&conn, "case-notebook", "ds-1", "disk.raw");

        // Add notebook entries and citations
        add_notebook_entry(&conn, "case-notebook", "entry-1", "Finding 1");
        add_notebook_entry(&conn, "case-notebook", "entry-2", "Finding 2");
        add_notebook_entry(&conn, "case-notebook", "entry-3", "Observation");
        add_citation(&conn, "entry-1", "cite-1", "node-a");
        add_citation(&conn, "entry-1", "cite-2", "node-b");
        add_citation(&conn, "entry-2", "cite-3", "node-c");

        let snapshot = get_v3_governance_snapshot(&conn, "case-notebook").unwrap();

        assert_eq!(snapshot.notebook_stats.entry_count, 3);
        assert_eq!(snapshot.notebook_stats.citation_count, 3);
    }

    #[test]
    fn v3_snapshot_includes_all_v2_fields() {
        let conn = setup_case_db("case-v2-compat");
        add_datasource(&conn, "case-v2-compat", "ds-1", "disk.raw");

        let snapshot = get_v3_governance_snapshot(&conn, "case-v2-compat").unwrap();

        // V2 fields are flattened into the root via serde(flatten)
        assert!(!snapshot.v2.generated_at.is_empty());
        assert!(!snapshot.v2.fact_sources.is_empty());
        assert!(!snapshot.v2.error_taxonomy_entries.is_empty());
        assert_eq!(snapshot.v2.release_gates.len(), 7);
    }

    #[test]
    fn get_platform_maps_known_families() {
        assert_eq!(get_platform("LNK"), "windows");
        assert_eq!(get_platform("Prefetch"), "windows");
        assert_eq!(get_platform("JumpList"), "windows");
        assert_eq!(get_platform("RegistryValue"), "windows");
        assert_eq!(get_platform("RecycleBin"), "windows");
        assert_eq!(get_platform("BrowserHistory"), "cross-platform");
        assert_eq!(get_platform("BrowserCookie"), "cross-platform");
        assert_eq!(get_platform("EmailMessage"), "cross-platform");
        assert_eq!(get_platform("BashHistory"), "linux");
    }

    #[test]
    fn get_platform_defaults_to_windows_for_unknown() {
        assert_eq!(get_platform("UnknownFamily"), "windows");
        assert_eq!(get_platform("CustomArtifact"), "windows");
    }

    #[test]
    fn build_rule_pack_status_parses_builtin_pack() {
        let status = build_rule_pack_status();
        assert_eq!(status.loaded_packs.len(), 1);
        assert!(status.loaded_packs[0].rule_count > 0);
        assert!(status.loaded_packs[0]
            .scope
            .contains(&"correlation".to_string()));
    }
}
