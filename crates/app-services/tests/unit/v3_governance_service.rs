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
fn build_rule_pack_status_parses_builtin_pack() {
    let status = build_rule_pack_status();
    assert_eq!(status.loaded_packs.len(), 1);
    assert!(status.loaded_packs[0].rule_count > 0);
    assert!(status.loaded_packs[0]
        .scope
        .contains(&"correlation".to_string()));
}
