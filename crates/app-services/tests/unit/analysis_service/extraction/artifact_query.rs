use super::*;
use chrono::{TimeZone, Utc};
use domain::{Artifact, ArtifactId};
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use serde_json::json;

fn artifact(id: &str, family: &str, attrs: Value) -> Artifact {
    Artifact {
        id: ArtifactId(id.into()),
        family: family.into(),
        title: id.into(),
        summary: "summary".into(),
        source_object_id: None,
        extractor_id: Some("parser.fixture".into()),
        extractor_version: Some("1".into()),
        confidence: Some(0.9),
        source_attribution: Some("fixture-source".into()),
        created_at: Utc.with_ymd_and_hms(2026, 11, 6, 1, 2, 3).unwrap(),
        attrs: serde_json::from_value(attrs).unwrap(),
    }
}

fn source(root: &std::path::Path) -> Connection {
    let conn = persistence_sqlite::open_or_create_source(&root.join("source.db")).unwrap();
    persistence_sqlite::source_fjall::configure_source_identity(&conn, "source").unwrap();
    conn
}

#[test]
fn fjall_queries_read_nested_payload_and_every_page() {
    let root = tempfile::tempdir().unwrap();
    let conn = source(root.path());
    let records = (0..1301)
        .map(|index| {
            artifact(
                &format!("a{index:04}"),
                "BrowserHistory",
                json!({"url": format!("https://fixture.test/{index}")}),
            )
        })
        .collect::<Vec<_>>();
    ArtifactRepo::new(&conn)
        .insert_batch(&records, "case", "source")
        .unwrap();
    assert_eq!(count_analysis_artifacts(&conn).unwrap(), 1301);
    let page = query_artifact_rows(&conn, &["BrowserHistory"], 1024, 277).unwrap();
    assert_eq!(page.len(), 277);
    assert_eq!(page.first().unwrap().id, "a1024");
    assert_eq!(page.last().unwrap().id, "a1300");
    assert_eq!(page[0].attrs["url"], "https://fixture.test/1024");
    assert_eq!(page[0].extractor_id.as_deref(), Some("parser.fixture"));
    let tables: u64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('artifacts','file_entries','timeline_events')", [], |row| row.get(0)).unwrap();
    assert_eq!(tables, 0);
}

#[test]
fn evtx_pagination_matches_sqlite_timestamp_sequence_oracle() {
    let root = tempfile::tempdir().unwrap();
    let conn = source(root.path());
    let records = vec![
        artifact(
            "early",
            "EvtxSecurityEvent",
            json!({"kind":"logon", "timestamp":"2026-11-06T00:00:00Z","recordId":3}),
        ),
        artifact(
            "tie-a",
            "EvtxSecurityEvent",
            json!({"kind":"logon", "timestamp":"2026-11-06T01:00:00+01:00","recordId":9}),
        ),
        artifact(
            "tie-b",
            "EvtxSecurityEvent",
            json!({"kind":"logon", "timestamp":"2026-11-06T00:00:00Z","recordId":9}),
        ),
        artifact(
            "late",
            "EvtxSecurityEvent",
            json!({"kind":"other", "timestamp":"2026-11-07T00:00:00Z","recordId":1}),
        ),
        artifact(
            "invalid",
            "EvtxSecurityEvent",
            json!({"kind":"logon", "timestamp":"invalid","recordId":12}),
        ),
        artifact(
            "missing",
            "EvtxSecurityEvent",
            json!({"kind":"logon", "recordId":8}),
        ),
    ];
    ArtifactRepo::new(&conn)
        .insert_batch(&records, "case", "source")
        .unwrap();
    let oracle = Connection::open_in_memory().unwrap();
    oracle
        .execute_batch("CREATE TABLE oracle(id TEXT PRIMARY KEY,attrs TEXT NOT NULL)")
        .unwrap();
    for record in &records {
        oracle
            .execute(
                "INSERT INTO oracle VALUES (?1,?2)",
                rusqlite::params![record.id.0, serde_json::to_string(&record.attrs).unwrap()],
            )
            .unwrap();
    }
    let expected = oracle.prepare("SELECT id FROM oracle WHERE json_extract(attrs,'$.kind')='logon' ORDER BY julianday(json_extract(attrs,'$.timestamp')) IS NULL ASC,julianday(json_extract(attrs,'$.timestamp')) DESC,CAST(json_extract(attrs,'$.recordId') AS INTEGER) DESC,id ASC").unwrap().query_map([], |row| row.get::<_,String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    let mut actual = Vec::new();
    for offset in [0, 2, 4] {
        actual.extend(
            query_evtx_artifact_rows_by_kinds(&conn, "EvtxSecurityEvent", &["logon"], offset, 2)
                .unwrap()
                .into_iter()
                .map(|row| row.id),
        );
    }
    assert_eq!(actual, expected);
}

#[test]
fn linux_summary_preserves_host_filtering_numbers_and_line_order() {
    let root = tempfile::tempdir().unwrap();
    let conn = source(root.path());
    let records = vec![
        artifact(
            "user",
            "LinuxSystemConfig",
            json!({"configKind":"passwdAccount","uid":"1001","overlayContext":null}),
        ),
        artifact(
            "container",
            "LinuxSystemConfig",
            json!({"configKind":"passwdAccount","uid":1002,"overlayContext":{}}),
        ),
        artifact(
            "root",
            "LinuxSystemConfig",
            json!({"configKind":"passwdAccount","uid":0}),
        ),
        artifact(
            "locked-bool",
            "LinuxSystemConfig",
            json!({"configKind":"shadowAccount","locked":true}),
        ),
        artifact(
            "locked-int",
            "LinuxSystemConfig",
            json!({"configKind":"shadowAccount","locked":1}),
        ),
        artifact(
            "line-2",
            "LinuxSystemConfig",
            json!({"sourcePath":"/etc/hostname","lineNumber":2}),
        ),
        artifact(
            "line-1",
            "LinuxSystemConfig",
            json!({"sourcePath":"/etc/hostname","lineNumber":1}),
        ),
    ];
    ArtifactRepo::new(&conn)
        .insert_batch(&records, "case", "source")
        .unwrap();
    assert_eq!(count_linux_user_accounts(&conn).unwrap(), 1);
    assert_eq!(count_linux_locked_shadow_accounts(&conn).unwrap(), 2);
    assert_eq!(query_linux_account_rows(&conn).unwrap().len(), 4);
    assert_eq!(query_linux_hostname_rows(&conn, 1).unwrap()[0].id, "line-1");
    assert_eq!(
        query_linux_system_config_by_kind_paged(&conn, "passwdAccount", 1, 1).unwrap()[0].id,
        "user"
    );
}
