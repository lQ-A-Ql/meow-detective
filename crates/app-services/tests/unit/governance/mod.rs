use chrono::Utc;
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceKind, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use persistence_sqlite::repositories::{
    audit_repo::{AuditAction, AuditRepo},
    datasource_repo::DataSourceRepo,
    job_repo::JobRepo,
    report_repo::{ReportRecord, ReportRepo},
};
use transport::dto::{
    BenchmarkRequirementStatusDto, CorrelationCoverageStatusDto, ReleaseGateStatusDto,
};

use crate::governance::get_v2_governance_snapshot;

#[test]
fn governance_snapshot_aggregates_runtime_signals() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '001', 'qa')",
        [],
    )
    .unwrap();

    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("case-1".to_string()),
            &DataSource {
                id: domain::DataSourceId("ds-1".to_string()),
                name: "source-1".to_string(),
                kind: DataSourceKind::Raw,
                source_path: std::path::PathBuf::from("C:/evidence/source-1.raw"),
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
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("case-1".to_string()),
            &DataSource {
                id: domain::DataSourceId("ds-2".to_string()),
                name: "source-2".to_string(),
                kind: DataSourceKind::E01,
                source_path: std::path::PathBuf::from("C:/evidence/source-2.E01"),
                imported_at: Utc::now(),
                provenance: DataSourceProvenance {
                    source_hash_sha256: None,
                    hash_status: DataSourceHashStatus::Pending,
                    canonical_source_path: None,
                    evidence_size: Some(2048),
                    reader_kind: Some("e01".to_string()),
                    provenance_status: DataSourceProvenanceStatus::Recorded,
                    warnings: vec!["pending".to_string()],
                },
            },
        )
        .unwrap();

    let job_id = JobRepo::new(&conn).create("case-1", "Import").unwrap();
    JobRepo::new(&conn)
        .update_outcome_counts(&job_id, 1, 0, 0, true)
        .unwrap();
    JobRepo::new(&conn).complete(&job_id, "partial").unwrap();
    ReportRepo::new(&conn)
        .insert(&ReportRecord {
            id: "report-1".to_string(),
            case_id: "case-1".to_string(),
            template_id: "summary".to_string(),
            file_name: "report.json".to_string(),
            created_by: "qa".to_string(),
            status: "completed".to_string(),
            progress: Some(100),
            created_at: Utc::now().to_rfc3339(),
        })
        .unwrap();
    AuditRepo::new(&conn)
        .log(
            Some("case-1"),
            "system",
            &AuditAction::McpToolCall,
            Some("fixture-catalog"),
            r#"{"status":"ok","toolName":"query_fixture_catalog"}"#,
        )
        .unwrap();
    AuditRepo::new(&conn)
        .log(
            Some("case-1"),
            "system",
            &AuditAction::FileExtract,
            Some("file-cmd-exe"),
            r#"{"status":"ok","destinationFileName":"cmd.exe"}"#,
        )
        .unwrap();

    let snapshot = get_v2_governance_snapshot(&conn, "case-1").unwrap();

    assert_eq!(snapshot.runtime_signals.data_source_count, 2);
    assert_eq!(snapshot.runtime_signals.hashed_data_source_count, 1);
    assert_eq!(snapshot.runtime_signals.pending_hash_data_source_count, 1);
    assert_eq!(snapshot.runtime_signals.warning_data_source_count, 1);
    assert_eq!(snapshot.runtime_signals.partial_job_count, 1);
    assert_eq!(snapshot.runtime_signals.report_count, 1);
    assert!(snapshot.runtime_signals.correlation_snapshot_available);
    assert_eq!(snapshot.runtime_signals.correlation_lead_count, 0);
    assert_eq!(
        snapshot
            .runtime_signals
            .correlation_high_confidence_lead_count,
        0
    );
    assert_eq!(snapshot.runtime_signals.correlation_review_lead_count, 0);
    assert_eq!(snapshot.runtime_signals.correlation_cluster_count, 0);
    assert_eq!(snapshot.runtime_signals.correlation_rule_family_count, 8);
    assert_eq!(snapshot.runtime_signals.correlation_covered_family_count, 0);
    assert_eq!(
        snapshot
            .runtime_signals
            .correlation_high_confidence_family_count,
        0
    );
    assert_eq!(
        snapshot.runtime_signals.correlation_family_coverage.len(),
        8
    );
    assert!(snapshot
        .runtime_signals
        .correlation_family_coverage
        .iter()
        .all(|item| item.status == CorrelationCoverageStatusDto::Missing));
    assert_eq!(snapshot.benchmark.required_checks.len(), 18);
    assert_eq!(snapshot.benchmark.covered_required_count, 18);
    assert_eq!(snapshot.benchmark.missing_required_count, 0);
    assert_eq!(snapshot.benchmark.exceeded_required_count, 0);
    assert_eq!(
        snapshot.benchmark.required_checks[0].status,
        BenchmarkRequirementStatusDto::Covered
    );
    assert_eq!(
        snapshot.benchmark.required_checks[2].status,
        BenchmarkRequirementStatusDto::Covered
    );
    assert_eq!(snapshot.security.audit_event_count, 2);
    assert_eq!(snapshot.security.sensitive_audit_event_count, 2);
    assert_eq!(snapshot.security.recent_audit_entries.len(), 2);
    let audit_actions = snapshot
        .security
        .recent_audit_entries
        .iter()
        .map(|entry| entry.action.as_str())
        .collect::<Vec<_>>();
    assert!(audit_actions.contains(&"file.extract"));
    assert!(audit_actions.contains(&"mcp.tool.call"));
    assert!(!snapshot.verification_chains.is_empty());
    assert!(!snapshot.support_matrix_entries.is_empty());
    assert_eq!(snapshot.known_limitations.len(), 54);
    assert_eq!(
        snapshot.support_matrix.documented_limit_count,
        snapshot.known_limitations.len() as u32
    );
    assert!(snapshot
        .known_limitations
        .iter()
        .any(|item| item.category == "Recycle Bin"
            && item.affected_chains.contains(&"RecycleBin".to_string())));
    assert!(snapshot
        .known_limitations
        .iter()
        .any(|item| item.category == "Browser"
            && item.affected_chains.contains(&"ChromeHistory".to_string())));
    assert!(snapshot
        .fact_sources
        .iter()
        .any(|item| item.fact_file == "testdata/governance/v2-known-limitations.json"));
    assert!(!snapshot.error_taxonomy_entries.is_empty());
    assert_eq!(snapshot.release_gates.len(), 7);
    assert_eq!(
        snapshot
            .release_gates
            .iter()
            .find(|gate| gate.gate_id == "core-fixture-regression")
            .map(|gate| gate.status.clone()),
        Some(ReleaseGateStatusDto::Warning)
    );
    assert_eq!(
        snapshot
            .release_gates
            .iter()
            .find(|gate| gate.gate_id == "benchmark-thresholds")
            .map(|gate| gate.status.clone()),
        Some(ReleaseGateStatusDto::Warning)
    );
    assert_eq!(
        snapshot
            .release_gates
            .iter()
            .find(|gate| gate.gate_id == "security-baseline")
            .map(|gate| gate.status.clone()),
        Some(ReleaseGateStatusDto::Passed)
    );
    assert_eq!(
        snapshot
            .release_gates
            .iter()
            .find(|gate| gate.gate_id == "correlation-family-coverage")
            .map(|gate| gate.status.clone()),
        Some(ReleaseGateStatusDto::Blocked)
    );
    assert_eq!(snapshot.release_scorecard.total_score, 70);
    assert_eq!(snapshot.release_scorecard.grade, "C");
    assert!(snapshot
        .release_scorecard
        .residual_risks
        .iter()
        .any(|item| item.contains("关联分析快照没有 lead")));
    assert!(snapshot
        .release_scorecard
        .breakdown
        .iter()
        .find(|entry| entry.dimension == "correlation")
        .map(|entry| entry
            .deductions
            .iter()
            .any(|item| item.contains("关联快照无 lead")))
        .unwrap_or(false));
}
