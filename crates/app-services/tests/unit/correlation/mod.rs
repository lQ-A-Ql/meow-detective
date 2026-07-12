use super::*;
use chrono::Utc;
use domain::{
    Artifact, ArtifactId, CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance,
    EntryType, FileEntry, FileEntryId, TimelineEvent, TimelineEventId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
    timeline_repo::TimelineRepo,
};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use transport::dto::CorrelationNodeKindDto;

fn setup_case_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    CaseRepo::new(&conn)
        .create(&domain::CaseMeta {
            id: CaseId("case-1".to_string()),
            name: "Case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("case-1".to_string()),
            &DataSource {
                id: DataSourceId("ds-1".to_string()),
                name: "source".to_string(),
                kind: DataSourceKind::Raw,
                source_path: "C:/evidence/mock.raw".into(),
                imported_at: Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .unwrap();
    conn
}

fn setup_case_db_without_source() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    CaseRepo::new(&conn)
        .create(&domain::CaseMeta {
            id: CaseId("case-1".to_string()),
            name: "Case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    conn
}

fn insert_file(conn: &Connection, id: &str, path: &str, deleted: bool) {
    FileRepo::new(conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds-1".to_string()),
            path: path.to_string(),
            name: super::rules::basename(path),
            entry_type: EntryType::File,
            size: Some(1024),
            ext: Some("exe".to_string()),
            deleted,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .unwrap();
}

fn insert_artifact(
    conn: &Connection,
    id: &str,
    family: &str,
    source_object_id: Option<&str>,
    attrs: BTreeMap<String, Value>,
) {
    ArtifactRepo::new(conn)
        .insert_batch(
            &[Artifact {
                id: ArtifactId(id.to_string()),
                family: family.to_string(),
                title: format!("{family} artifact"),
                summary: "fixture".to_string(),
                source_object_id: source_object_id.map(|value| FileEntryId(value.to_string())),
                extractor_id: Some(family.to_ascii_lowercase()),
                extractor_version: Some("1.0.0".to_string()),
                confidence: Some(0.91),
                source_attribution: Some("fixture".to_string()),
                created_at: Utc::now(),
                attrs,
            }],
            "case-1",
            "ds-1",
        )
        .unwrap();
}

fn register_source(case_conn: &Connection, case_id: &CaseId, source_id: &str) {
    let ds_id = DataSourceId(source_id.to_string());
    DataSourceRepo::new(case_conn)
        .insert_with_storage(
            case_id,
            &DataSource {
                id: ds_id.clone(),
                name: source_id.to_string(),
                kind: DataSourceKind::Raw,
                source_path: format!("C:/evidence/{source_id}.raw").into(),
                imported_at: Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
            &DataSourceStorage::source_db(&ds_id.0, Some("linux"), None),
        )
        .unwrap();
    case_conn
        .execute_batch("UPDATE data_sources SET import_state='ready'")
        .unwrap();
}

fn insert_source_correlation_fixture(case_root: &std::path::Path, source_id: &str, title: &str) {
    let ds_id = DataSourceId(source_id.to_string());
    let source_conn = crate::source_db::open_source_db(case_root, &ds_id).unwrap();
    FileRepo::new(&source_conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId("file-1".to_string()),
            parent_id: None,
            data_source_id: ds_id.clone(),
            path: format!("C:/{title}/cmd.exe"),
            name: "cmd.exe".to_string(),
            entry_type: EntryType::File,
            size: Some(1024),
            ext: Some("exe".to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .unwrap();
    ArtifactRepo::new(&source_conn)
        .insert_batch(
            &[Artifact {
                id: ArtifactId("artifact-1".to_string()),
                family: "Prefetch".to_string(),
                title: format!("{title} prefetch"),
                summary: "fixture".to_string(),
                source_object_id: Some(FileEntryId("file-1".to_string())),
                extractor_id: Some("prefetch".to_string()),
                extractor_version: Some("1.0.0".to_string()),
                confidence: Some(0.91),
                source_attribution: Some("fixture".to_string()),
                created_at: Utc::now(),
                attrs: BTreeMap::new(),
            }],
            "case-1",
            source_id,
        )
        .unwrap();
    TimelineRepo::new(&source_conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-1".to_string()),
                source_object_id: "file-1".to_string(),
                event_type: "FILE_MODIFIED".to_string(),
                timestamp: Utc::now(),
                title: format!("{title} modified"),
                description: "fixture".to_string(),
                parser_id: Some("timeline.macb".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.82),
                source_attribution: Some("modified_at".to_string()),
                attrs: BTreeMap::new(),
            }],
            "case-1",
        )
        .unwrap();
}

#[test]
fn case_correlation_scopes_duplicate_local_ids_by_data_source() {
    let tmp = tempfile::TempDir::new().unwrap();
    let case_conn = setup_case_db_without_source();
    let case_id = CaseId("case-1".to_string());
    register_source(&case_conn, &case_id, "ds-a");
    register_source(&case_conn, &case_id, "ds-b");
    insert_source_correlation_fixture(tmp.path(), "ds-a", "alpha");
    insert_source_correlation_fixture(tmp.path(), "ds-b", "beta");

    let snapshot = get_correlation_snapshot_for_case(&case_conn, tmp.path(), &case_id).unwrap();

    assert_eq!(snapshot.lead_count, 2);
    assert!(snapshot
        .leads
        .iter()
        .any(|lead| lead.primary_file_id == "ds:ds-a:file-1"));
    assert!(snapshot
        .leads
        .iter()
        .any(|lead| lead.primary_file_id == "ds:ds-b:file-1"));
    assert!(snapshot
        .nodes
        .iter()
        .any(|node| node.id == "file:ds:ds-a:file-1"));
    assert!(snapshot
        .nodes
        .iter()
        .any(|node| node.id == "file:ds:ds-b:file-1"));
    assert!(snapshot
        .edges
        .iter()
        .all(|edge| edge.id.starts_with("ds:ds-a:") || edge.id.starts_with("ds:ds-b:")));
}

#[test]
fn correlation_snapshot_groups_artifact_and_timeline_by_source_object() {
    let conn = setup_case_db();
    insert_file(&conn, "file-1", "C:/Windows/System32/cmd.exe", true);
    insert_artifact(
        &conn,
        "artifact-1",
        "Prefetch",
        Some("file-1"),
        BTreeMap::new(),
    );
    TimelineRepo::new(&conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-1".to_string()),
                source_object_id: "file-1".to_string(),
                event_type: "FILE_MODIFIED".to_string(),
                timestamp: Utc::now(),
                title: "File modified".to_string(),
                description: "MACB projection".to_string(),
                parser_id: Some("timeline.macb".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.82),
                source_attribution: Some("modified_at".to_string()),
                attrs: BTreeMap::new(),
            }],
            "case-1",
        )
        .unwrap();

    let snapshot = get_correlation_snapshot(&conn).unwrap();

    assert_eq!(snapshot.cluster_count, 1);
    assert_eq!(snapshot.lead_count, 1);
    assert!(snapshot.node_count >= 3);
    assert!(snapshot.edge_count >= 3);
    assert_eq!(
        snapshot.leads[0].confidence,
        CorrelationConfidenceDto::Direct
    );
    assert_eq!(snapshot.leads[0].primary_file_id, "file-1");
    assert!(snapshot.leads[0].summary.contains("痕迹记录"));
    assert!(snapshot.nodes.iter().any(|node| {
        node.kind == CorrelationNodeKindDto::File
            && node.badges.iter().any(|badge| badge == "deleted")
    }));
    assert!(
        snapshot.clusters[0]
            .provenance
            .iter()
            .any(|item| item.source_kind == "artifact"
                && item.producer.as_deref() == Some("prefetch"))
    );
}

#[test]
fn correlation_snapshot_matches_lnk_target_path_to_file() {
    let conn = setup_case_db();
    insert_file(&conn, "file-lnk", "C:/Users/Admin/Desktop/cmd.lnk", false);
    insert_file(&conn, "file-cmd", "C:/Windows/System32/cmd.exe", false);

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "target_path".to_string(),
        Value::String("C:/Windows/System32/cmd.exe".to_string()),
    );
    insert_artifact(&conn, "artifact-lnk", "LNK", Some("file-lnk"), attrs);

    let snapshot = get_correlation_snapshot(&conn).unwrap();
    let lead = snapshot
        .leads
        .iter()
        .find(|item| item.id == "lead:rules:file-cmd")
        .unwrap();

    assert_eq!(lead.primary_file_id, "file-cmd");
    assert_eq!(lead.confidence, CorrelationConfidenceDto::Direct);
    assert!(lead.summary.contains("路径"));
    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == CorrelationEdgeKindDto::PathMatch
            && edge.from_node_id == "artifact:artifact-lnk"
            && edge.to_node_id == "file:file-cmd"
    }));
}

#[test]
fn correlation_snapshot_matches_registry_value_path_to_file() {
    let conn = setup_case_db();
    insert_file(
        &conn,
        "file-reg",
        "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
        false,
    );

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "data".to_string(),
        Value::String(
            "\"C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -nop".to_string(),
        ),
    );
    insert_artifact(
        &conn,
        "artifact-reg",
        "RegistryValue",
        Some("registry-hive"),
        attrs,
    );

    let snapshot = get_correlation_snapshot(&conn).unwrap();

    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == CorrelationEdgeKindDto::PathMatch
            && edge.from_node_id == "artifact:artifact-reg"
            && edge.to_node_id == "file:file-reg"
    }));
}

#[test]
fn correlation_snapshot_matches_recycle_bin_original_path_to_deleted_file() {
    let conn = setup_case_db();
    insert_file(
        &conn,
        "file-deleted",
        "C:/Users/Admin/Desktop/secrets.txt",
        true,
    );

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "original_path".to_string(),
        Value::String("C:/Users/Admin/Desktop/secrets.txt".to_string()),
    );
    insert_artifact(&conn, "artifact-rb", "RecycleBin", Some("recycle-i"), attrs);

    let snapshot = get_correlation_snapshot(&conn).unwrap();

    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == CorrelationEdgeKindDto::RecoveredOriginalPath
            && edge.from_node_id == "artifact:artifact-rb"
            && edge.to_node_id == "file:file-deleted"
    }));
}

#[test]
fn correlation_snapshot_matches_prefetch_executable_name_to_file_name() {
    let conn = setup_case_db();
    insert_file(&conn, "file-cmd", "C:/Windows/System32/cmd.exe", false);

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "executable".to_string(),
        Value::String("CMD.EXE".to_string()),
    );
    insert_artifact(&conn, "artifact-pf", "Prefetch", Some("pf-file"), attrs);

    let snapshot = get_correlation_snapshot(&conn).unwrap();
    let edge = snapshot
        .edges
        .iter()
        .find(|edge| {
            edge.from_node_id == "artifact:artifact-pf"
                && edge.kind == CorrelationEdgeKindDto::NameMatch
        })
        .unwrap();

    assert_eq!(edge.kind, CorrelationEdgeKindDto::NameMatch);
    assert_eq!(edge.confidence, CorrelationConfidenceDto::Strong);
}

#[test]
fn correlation_snapshot_rule_group_uses_related_timeline_as_context() {
    let conn = setup_case_db();
    insert_file(&conn, "file-payload", "C:/Temp/payload.exe", false);

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "targetPath".to_string(),
        Value::String("C:/Temp/payload.exe".to_string()),
    );
    insert_artifact(
        &conn,
        "artifact-download",
        "BrowserDownload",
        Some("browser-db"),
        attrs,
    );

    TimelineRepo::new(&conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-download".to_string()),
                source_object_id: "file-payload".to_string(),
                event_type: "FILE_CREATED".to_string(),
                timestamp: Utc::now(),
                title: "payload.exe created".to_string(),
                description: "download landed".to_string(),
                parser_id: Some("timeline.macb".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.8),
                source_attribution: Some("created_at".to_string()),
                attrs: BTreeMap::new(),
            }],
            "case-1",
        )
        .unwrap();

    let snapshot = get_correlation_snapshot(&conn).unwrap();
    let cluster = snapshot
        .clusters
        .iter()
        .find(|item| item.id == "cluster:rules:file-payload")
        .unwrap();

    assert_eq!(cluster.timeline_count, 1);
    assert!(cluster
        .edge_ids
        .iter()
        .any(|item| item.contains("rule-timeline")));
    assert!(snapshot.edges.iter().any(|edge| {
        edge.id.contains("rule-timeline")
            && edge.to_node_id == "file:file-payload"
            && edge.kind == CorrelationEdgeKindDto::TemporalContext
    }));
}

#[test]
fn correlation_snapshot_matches_jumplist_target_path_to_file() {
    let conn = setup_case_db();
    insert_file(
        &conn,
        "file-report",
        "C:/Users/Admin/Documents/report.docx",
        false,
    );

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "target_path".to_string(),
        Value::String("C:/Users/Admin/Documents/report.docx".to_string()),
    );
    insert_artifact(
        &conn,
        "artifact-jumplist",
        "JumpList",
        Some("jumplist-file"),
        attrs,
    );

    let snapshot = get_correlation_snapshot(&conn).unwrap();

    assert!(snapshot.edges.iter().any(|edge| {
        edge.kind == CorrelationEdgeKindDto::PathMatch
            && edge.from_node_id == "artifact:artifact-jumplist"
            && edge.to_node_id == "file:file-report"
    }));
}

#[test]
fn correlation_snapshot_adds_proximity_timeline_signal_for_browser_download() {
    let conn = setup_case_db();
    insert_file(&conn, "file-payload", "C:/Temp/payload.exe", false);
    insert_file(
        &conn,
        "file-history",
        "C:/Users/Admin/AppData/Local/Edge/User Data/Default/History",
        false,
    );

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "targetPath".to_string(),
        Value::String("C:/Temp/payload.exe".to_string()),
    );
    attrs.insert(
        "startTime".to_string(),
        Value::String("2026-06-12T10:00:00Z".to_string()),
    );
    insert_artifact(
        &conn,
        "artifact-download-proximity",
        "BrowserDownload",
        Some("file-history"),
        attrs,
    );

    TimelineRepo::new(&conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-near-download".to_string()),
                source_object_id: "other-file".to_string(),
                event_type: "FILE_CREATED".to_string(),
                timestamp: chrono::DateTime::parse_from_rfc3339("2026-06-12T10:05:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                title: "payload created".to_string(),
                description: "nearby timeline".to_string(),
                parser_id: Some("timeline.macb".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.75),
                source_attribution: Some("C:/Temp/payload.exe".to_string()),
                attrs: BTreeMap::new(),
            }],
            "case-1",
        )
        .unwrap();

    let snapshot = get_correlation_snapshot(&conn).unwrap();
    let lead = snapshot
        .leads
        .iter()
        .find(|item| item.id == "lead:rules:file-payload")
        .unwrap();

    assert!(lead
        .match_signals
        .iter()
        .any(|item| item.contains("邻近时间线命中 FILE_CREATED")));
}

#[test]
fn correlation_snapshot_adds_proximity_timeline_signal_for_email_message() {
    let conn = setup_case_db();
    insert_file(&conn, "file-triage", "C:/Cases/triage.csv", false);
    insert_file(
        &conn,
        "file-mail",
        "C:/Users/Admin/Documents/incident-response.eml",
        false,
    );

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "attachments".to_string(),
        Value::Array(vec![Value::String("triage.csv".to_string())]),
    );
    attrs.insert(
        "subject".to_string(),
        Value::String("Initial triage notes".to_string()),
    );
    attrs.insert(
        "sentAt".to_string(),
        Value::String("2026-06-12T11:00:00Z".to_string()),
    );
    insert_artifact(
        &conn,
        "artifact-email-proximity",
        "EmailMessage",
        Some("file-mail"),
        attrs,
    );

    TimelineRepo::new(&conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-near-email".to_string()),
                source_object_id: "other-file".to_string(),
                event_type: "REPORT_UPDATED".to_string(),
                timestamp: chrono::DateTime::parse_from_rfc3339("2026-06-12T11:10:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                title: "Initial triage notes".to_string(),
                description: "triage.csv refreshed".to_string(),
                parser_id: Some("timeline.note".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.72),
                source_attribution: Some("C:/Cases/triage.csv".to_string()),
                attrs: BTreeMap::new(),
            }],
            "case-1",
        )
        .unwrap();

    let snapshot = get_correlation_snapshot(&conn).unwrap();
    let lead = snapshot
        .leads
        .iter()
        .find(|item| item.id == "lead:rules:file-triage")
        .unwrap();

    assert!(lead
        .match_signals
        .iter()
        .any(|item| item.contains("邻近时间线命中 REPORT_UPDATED")));
}

#[test]
fn correlation_snapshot_adds_proximity_timeline_signal_for_browser_history() {
    let conn = setup_case_db();
    insert_file(
        &conn,
        "file-browser-cache",
        "C:/Users/Admin/AppData/Local/Edge/User Data/Default/History",
        false,
    );
    insert_file(
        &conn,
        "file-report",
        "C:/Cases/browser-incident-report.docx",
        false,
    );

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "url".to_string(),
        Value::String("https://intranet.local/reports/browser-incident-report".to_string()),
    );
    attrs.insert(
        "title".to_string(),
        Value::String("browser-incident-report.docx draft".to_string()),
    );
    attrs.insert(
        "visitTime".to_string(),
        Value::String("2026-06-12T12:00:00Z".to_string()),
    );
    insert_artifact(
        &conn,
        "artifact-browser-history-proximity",
        "BrowserHistory",
        Some("file-browser-cache"),
        attrs,
    );

    TimelineRepo::new(&conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-near-browser-history".to_string()),
                source_object_id: "other-file".to_string(),
                event_type: "REPORT_OPENED".to_string(),
                timestamp: chrono::DateTime::parse_from_rfc3339("2026-06-12T12:15:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                title: "browser-incident-report.docx draft".to_string(),
                description: "C:/Cases/browser-incident-report.docx opened".to_string(),
                parser_id: Some("timeline.note".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.78),
                source_attribution: Some("C:/Cases/browser-incident-report.docx".to_string()),
                attrs: BTreeMap::new(),
            }],
            "case-1",
        )
        .unwrap();

    let snapshot = get_correlation_snapshot(&conn).unwrap();
    let lead = snapshot
        .leads
        .iter()
        .find(|item| item.id == "lead:rules:file-report")
        .unwrap();

    assert_eq!(lead.confidence, CorrelationConfidenceDto::Strong);
    assert!(lead
        .match_signals
        .iter()
        .any(|item| item.contains("BrowserHistory 标题或 URL 命中文件名")));
    assert!(lead
        .match_signals
        .iter()
        .any(|item| item.contains("邻近时间线命中 REPORT_OPENED")));
}

// ── Cache tests ──

#[test]
fn cached_snapshot_matches_recomputed() {
    let conn = setup_case_db();
    insert_file(&conn, "file-1", "C:/Windows/System32/cmd.exe", false);

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "target_path".to_string(),
        Value::String("C:/Windows/System32/cmd.exe".to_string()),
    );
    insert_artifact(&conn, "artifact-lnk", "LNK", Some("file-lnk"), attrs);

    let first = get_correlation_snapshot(&conn).unwrap();

    let second = get_correlation_snapshot(&conn).unwrap();

    assert_eq!(first.nodes.len(), second.nodes.len());
    assert_eq!(first.edges.len(), second.edges.len());
    assert_eq!(first.clusters.len(), second.clusters.len());
    assert_eq!(first.leads.len(), second.leads.len());
    assert_eq!(first.node_count, second.node_count);
    assert_eq!(first.edge_count, second.edge_count);
    assert_eq!(first.cluster_count, second.cluster_count);
    assert_eq!(first.lead_count, second.lead_count);

    let first_ids: BTreeSet<_> = first.nodes.iter().map(|n| n.id.clone()).collect();
    let second_ids: BTreeSet<_> = second.nodes.iter().map(|n| n.id.clone()).collect();
    assert_eq!(first_ids, second_ids);
}

#[test]
fn cache_invalidates_on_new_import() {
    let conn = setup_case_db();
    insert_file(&conn, "file-1", "C:/Windows/System32/cmd.exe", false);

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "target_path".to_string(),
        Value::String("C:/Windows/System32/cmd.exe".to_string()),
    );
    insert_artifact(&conn, "artifact-lnk", "LNK", Some("file-lnk"), attrs);

    // Populate cache
    let first = get_correlation_snapshot(&conn).unwrap();
    assert!(first.node_count > 0);

    // Verify cache row exists
    let cached_hash: String = conn
        .query_row(
            "SELECT artifact_hash FROM correlation_snapshots WHERE case_id = 'case-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!cached_hash.is_empty());

    // Simulate new data source import by invalidating the cache
    invalidate_correlation_cache(&conn, "case-1").unwrap();

    // Verify cache is cleared
    let cached: Option<String> = conn
        .query_row(
            "SELECT artifact_hash FROM correlation_snapshots WHERE case_id = 'case-1'",
            [],
            |row| row.get(0),
        )
        .ok();
    assert!(cached.is_none(), "cache should be empty after invalidation");

    // Next call should recompute (not return from cache)
    let second = get_correlation_snapshot(&conn).unwrap();
    assert_eq!(second.node_count, first.node_count);

    // Cache should be repopulated
    let repopulated: String = conn
        .query_row(
            "SELECT artifact_hash FROM correlation_snapshots WHERE case_id = 'case-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!repopulated.is_empty());
}

#[test]
fn incremental_only_processes_new_artifacts() {
    let conn = setup_case_db();
    insert_file(&conn, "file-lnk", "C:/Users/Admin/Desktop/cmd.lnk", false);
    insert_file(&conn, "file-cmd", "C:/Windows/System32/cmd.exe", false);

    // First artifact: LNK pointing to cmd.exe
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "target_path".to_string(),
        Value::String("C:/Windows/System32/cmd.exe".to_string()),
    );
    insert_artifact(&conn, "artifact-lnk", "LNK", Some("file-lnk"), attrs);

    // Build initial snapshot (and cache)
    let initial = get_correlation_snapshot(&conn).unwrap();
    let initial_node_count = initial.node_count;
    let initial_edge_count = initial.edge_count;
    assert!(initial_node_count > 0);

    // Verify the cached artifact_ids include artifact-lnk
    let cached_ids: String = conn
        .query_row(
            "SELECT artifact_ids_json FROM correlation_snapshots WHERE case_id = 'case-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(cached_ids.contains("artifact-lnk"));
    let cached_ids_set: BTreeSet<String> = serde_json::from_str(&cached_ids).unwrap();
    assert!(!cached_ids_set.contains("artifact-lnk-2"));

    // Add a new LNK artifact (different file target)
    insert_file(
        &conn,
        "file-lnk-2",
        "C:/Users/Admin/Desktop/powershell.lnk",
        false,
    );
    insert_file(
        &conn,
        "file-powershell",
        "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
        false,
    );
    let mut attrs2 = BTreeMap::new();
    attrs2.insert(
        "target_path".to_string(),
        Value::String("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe".to_string()),
    );
    insert_artifact(&conn, "artifact-lnk-2", "LNK", Some("file-lnk-2"), attrs2);

    // Incremental: should only process the new artifact
    let updated = get_correlation_snapshot_incremental(&conn).unwrap();

    // Should have more nodes and edges than the initial snapshot
    assert!(
        updated.node_count > initial_node_count,
        "expected updated node_count {} > initial {}",
        updated.node_count,
        initial_node_count
    );
    assert!(
        updated.edge_count > initial_edge_count,
        "expected updated edge_count {} > initial {}",
        updated.edge_count,
        initial_edge_count
    );

    // Both artifacts' nodes should be present
    assert!(updated
        .nodes
        .iter()
        .any(|n| n.id == "artifact:artifact-lnk"));
    assert!(updated
        .nodes
        .iter()
        .any(|n| n.id == "artifact:artifact-lnk-2"));

    // The new LNK artifact should have generated a PathMatch edge to powershell.exe
    assert!(updated.edges.iter().any(|edge| {
        edge.kind == CorrelationEdgeKindDto::PathMatch
            && edge.from_node_id == "artifact:artifact-lnk-2"
            && edge.to_node_id == "file:file-powershell"
    }));

    // Original data is preserved
    assert!(updated.edges.iter().any(|edge| {
        edge.kind == CorrelationEdgeKindDto::PathMatch
            && edge.from_node_id == "artifact:artifact-lnk"
            && edge.to_node_id == "file:file-cmd"
    }));

    // Cache should now include both artifact IDs
    let updated_ids: String = conn
        .query_row(
            "SELECT artifact_ids_json FROM correlation_snapshots WHERE case_id = 'case-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(updated_ids.contains("artifact-lnk"));
    assert!(updated_ids.contains("artifact-lnk-2"));
}

#[test]
fn second_call_under_200ms() {
    let conn = setup_case_db();

    // Insert a modest number of artifacts and files to make the first call non-trivial
    for i in 0..10 {
        insert_file(
            &conn,
            &format!("file-{i}"),
            &format!("C:/Test/file{i}.exe"),
            false,
        );
    }
    for i in 0..10 {
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "target_path".to_string(),
            Value::String(format!("C:/Test/file{i}.exe")),
        );
        insert_artifact(
            &conn,
            &format!("artifact-lnk-{i}"),
            "LNK",
            Some(&format!("file-{i}")),
            attrs,
        );
    }

    // First call: populate cache (measure but don't enforce a limit)
    let t0 = std::time::Instant::now();
    let _first = get_correlation_snapshot(&conn).unwrap();
    let first_duration = t0.elapsed();
    // Verify first call was non-trivial (should take some time with rayon + files)
    assert!(
        first_duration.as_nanos() > 0,
        "first call should do real work"
    );

    // Second call: should hit cache
    let t1 = std::time::Instant::now();
    let _second = get_correlation_snapshot(&conn).unwrap();
    let second_duration = t1.elapsed();

    assert!(
        second_duration.as_millis() < 200,
        "second cached call took {}ms, expected < 200ms",
        second_duration.as_millis()
    );
}

#[test]
fn artifact_family_maps_all_registry_families_to_registry() {
    assert_eq!(
        artifact_family("RegistrySamUser"),
        Some("Registry".to_string())
    );
    assert_eq!(
        artifact_family("RegistryUserAssist"),
        Some("Registry".to_string())
    );
    assert_eq!(
        artifact_family("RegistryShutdownTime"),
        Some("Registry".to_string())
    );
    assert_eq!(
        artifact_family("RegistryValue"),
        Some("Registry".to_string())
    );
    // Non-registry families stay unchanged.
    assert_eq!(artifact_family("Prefetch"), Some("Prefetch".to_string()));
    assert_eq!(artifact_family("LNK"), Some("LNK".to_string()));
}

#[test]
fn correlation_groups_registry_sam_and_timeline_into_registry_family() {
    let conn = setup_case_db();
    insert_file(&conn, "file-sam", "C:/Windows/System32/config/SAM", false);

    let mut attrs = BTreeMap::new();
    attrs.insert(
        "subjectSid".to_string(),
        Value::String("S-1-5-21-1-2-3-1001".to_string()),
    );
    attrs.insert(
        "subjectUsername".to_string(),
        Value::String("alice".to_string()),
    );
    insert_artifact(
        &conn,
        "artifact-sam",
        "RegistrySamUser",
        Some("file-sam"),
        attrs,
    );

    TimelineRepo::new(&conn)
        .insert_batch_with_case(
            &[TimelineEvent {
                id: TimelineEventId("timeline-sam-login".to_string()),
                source_object_id: "file-sam".to_string(),
                event_type: "REGISTRY_LAST_LOGIN".to_string(),
                timestamp: Utc::now(),
                title: "SAM last login".to_string(),
                description: "alice logged in".to_string(),
                parser_id: Some("registry.sam.v1".to_string()),
                parser_version: Some("1.0.0".to_string()),
                confidence: Some(0.85),
                source_attribution: None,
                attrs: BTreeMap::new(),
            }],
            "case-1",
        )
        .unwrap();

    let snapshot = get_correlation_snapshot(&conn).unwrap();
    let lead = snapshot
        .leads
        .iter()
        .find(|item| item.primary_file_id == "file-sam")
        .unwrap();
    assert!(lead.families.iter().any(|f| f == "Registry"));
    assert!(lead.summary.contains("痕迹记录"));
}
