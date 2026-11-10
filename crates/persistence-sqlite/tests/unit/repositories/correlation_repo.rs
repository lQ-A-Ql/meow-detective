use super::*;
use domain::{Artifact, ArtifactId};
use std::collections::BTreeMap;

fn setup_db() -> (tempfile::TempDir, Connection) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.db");
    let conn = crate::open_or_create_source(&path).unwrap();
    crate::source_fjall::configure_source_identity(&conn, "source-1").unwrap();
    crate::source_fjall::open_store_for_write(&conn)
        .unwrap()
        .unwrap();
    conn.execute(
        "INSERT INTO data_sources(id,case_id,name,kind,source_path,imported_at)
         VALUES('source-1','case-1','source','raw','source.raw','2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    (root, conn)
}

fn artifact(id: &str) -> Artifact {
    Artifact {
        id: ArtifactId(id.into()),
        family: "Correlation".into(),
        source_object_id: None,
        title: id.into(),
        summary: id.into(),
        extractor_id: None,
        extractor_version: None,
        confidence: None,
        source_attribution: None,
        created_at: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        attrs: BTreeMap::new(),
    }
}

#[test]
fn store_and_get_snapshot_cache() {
    let (_root, conn) = setup_db();

    let row = get_correlation_snapshot_cache(&conn, "case-1").unwrap();
    assert!(row.is_none());

    store_correlation_snapshot_cache(
        &conn,
        "case-1",
        r#"{"nodes":[]}"#,
        "2026-01-01T00:00:00Z",
        "abc123",
        r#"["a1"]"#,
    )
    .unwrap();

    let row = get_correlation_snapshot_cache(&conn, "case-1")
        .unwrap()
        .expect("should exist");
    assert_eq!(row.snapshot_json, r#"{"nodes":[]}"#);
    assert_eq!(row.artifact_hash, "abc123");
    assert_eq!(row.artifact_ids_json, r#"["a1"]"#);
}

#[test]
fn store_cache_overwrites_previous() {
    let (_root, conn) = setup_db();

    store_correlation_snapshot_cache(
        &conn,
        "case-1",
        "old",
        "2026-01-01T00:00:00Z",
        "hash1",
        "[]",
    )
    .unwrap();
    store_correlation_snapshot_cache(
        &conn,
        "case-1",
        "new",
        "2026-01-02T00:00:00Z",
        "hash2",
        r#"["a2"]"#,
    )
    .unwrap();

    let row = get_correlation_snapshot_cache(&conn, "case-1")
        .unwrap()
        .unwrap();
    assert_eq!(row.snapshot_json, "new");
    assert_eq!(row.artifact_hash, "hash2");
}

#[test]
fn clear_cache_removes_both_tables() {
    let (_root, conn) = setup_db();

    store_correlation_snapshot_cache(
        &conn,
        "case-1",
        "test",
        "2026-01-01T00:00:00Z",
        "hash",
        "[]",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO correlation_edges_cache (edge_id, case_id, source_id, target_id)
         VALUES ('e1', 'case-1', 'source', 'target')",
        [],
    )
    .unwrap();

    clear_correlation_cache(&conn, "case-1").unwrap();

    assert!(get_correlation_snapshot_cache(&conn, "case-1")
        .unwrap()
        .is_none());

    let edge_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM correlation_edges_cache WHERE case_id = 'case-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(edge_count, 0);
}

#[test]
fn compute_artifact_hash_is_stable() {
    let (_root, conn) = setup_db();
    crate::repositories::artifact_repo::ArtifactRepo::new(&conn)
        .insert_batch(&[artifact("a1"), artifact("a2")], "case-1", "source-1")
        .unwrap();

    let h1 = compute_artifact_hash_hex(&conn).unwrap();
    let h2 = compute_artifact_hash_hex(&conn).unwrap();
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64); // SHA-256 hex is 64 chars
}

#[test]
fn compute_artifact_hash_changes_when_data_changes() {
    let (_root, conn) = setup_db();
    crate::repositories::artifact_repo::ArtifactRepo::new(&conn)
        .insert_batch(&[artifact("a1")], "case-1", "source-1")
        .unwrap();

    let h1 = compute_artifact_hash_hex(&conn).unwrap();

    crate::repositories::artifact_repo::ArtifactRepo::new(&conn)
        .insert_batch(&[artifact("a2")], "case-1", "source-1")
        .unwrap();

    let h2 = compute_artifact_hash_hex(&conn).unwrap();
    assert_ne!(h1, h2);
}

#[test]
fn resolve_case_id_returns_first_distinct() {
    let (_root, conn) = setup_db();
    crate::repositories::artifact_repo::ArtifactRepo::new(&conn)
        .insert_batch(&[artifact("a1"), artifact("a2")], "case-1", "source-1")
        .unwrap();

    let case_id = resolve_case_id(&conn).unwrap();
    assert_eq!(case_id.as_deref(), Some("case-1"));
}

#[test]
fn resolve_case_id_returns_none_when_empty() {
    let (_root, conn) = setup_db();
    let case_id = resolve_case_id(&conn).unwrap();
    assert!(case_id.is_none());
}

#[test]
fn collect_artifact_ids_returns_sorted() {
    let (_root, conn) = setup_db();
    crate::repositories::artifact_repo::ArtifactRepo::new(&conn)
        .insert_batch(&[artifact("a2"), artifact("a1")], "case-1", "source-1")
        .unwrap();

    let ids = collect_artifact_ids(&conn).unwrap();
    assert_eq!(ids, vec!["a1", "a2"]);
}
