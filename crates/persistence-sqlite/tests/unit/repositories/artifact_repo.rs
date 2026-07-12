use super::*;
use std::collections::BTreeMap;

fn setup_db() -> rusqlite::Connection {
    let conn = crate::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE artifacts (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL DEFAULT '',
            data_source_id TEXT NOT NULL DEFAULT '',
            artifact_type TEXT NOT NULL,
            source_object_id TEXT,
            extractor_id TEXT,
            extractor_version TEXT,
            confidence REAL,
            source_attribution TEXT,
            title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            attrs TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    conn
}

fn setup_legacy_db() -> rusqlite::Connection {
    let conn = crate::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE artifacts (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL DEFAULT '',
            data_source_id TEXT NOT NULL DEFAULT '',
            artifact_type TEXT NOT NULL,
            source_object_id TEXT,
            title TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            attrs TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ALTER TABLE artifacts ADD COLUMN extractor_id TEXT;
        ALTER TABLE artifacts ADD COLUMN extractor_version TEXT;
        ALTER TABLE artifacts ADD COLUMN confidence REAL;
        ALTER TABLE artifacts ADD COLUMN source_attribution TEXT;",
    )
    .unwrap();
    conn
}

fn make_artifact(id: &str, family: &str) -> Artifact {
    Artifact {
        id: ArtifactId(id.to_string()),
        family: family.to_string(),
        title: format!("Title {}", id),
        summary: format!("Summary {}", id),
        source_object_id: None,
        extractor_id: None,
        extractor_version: None,
        confidence: None,
        source_attribution: None,
        created_at: chrono::Utc::now(),
        attrs: BTreeMap::new(),
    }
}

#[test]
fn insert_batch_then_count_returns_correct_number() {
    let conn = setup_db();
    let repo = ArtifactRepo::new(&conn);
    let artifacts = vec![
        make_artifact("a1", "evtx"),
        make_artifact("a2", "prefetch"),
        make_artifact("a3", "evtx"),
    ];
    repo.insert_batch(&artifacts, "case-1", "ds-1").unwrap();

    assert_eq!(repo.count().unwrap(), 3);
}

#[test]
fn families_returns_distinct_types() {
    let conn = setup_db();
    let repo = ArtifactRepo::new(&conn);
    let artifacts = vec![
        make_artifact("a1", "evtx"),
        make_artifact("a2", "prefetch"),
        make_artifact("a3", "evtx"),
    ];
    repo.insert_batch(&artifacts, "case-1", "ds-1").unwrap();

    let families = repo.families().unwrap();
    assert_eq!(families, vec!["evtx", "prefetch"]);
}

#[test]
fn count_by_family_returns_grouped_counts() {
    let conn = setup_db();
    let repo = ArtifactRepo::new(&conn);
    let artifacts = vec![
        make_artifact("a1", "evtx"),
        make_artifact("a2", "prefetch"),
        make_artifact("a3", "evtx"),
    ];
    repo.insert_batch(&artifacts, "case-1", "ds-1").unwrap();

    let counts = repo.count_by_family().unwrap();
    assert_eq!(
        counts,
        vec![("evtx".to_string(), 2), ("prefetch".to_string(), 1)]
    );
}

#[test]
fn list_by_family_filters_correctly() {
    let conn = setup_db();
    let repo = ArtifactRepo::new(&conn);
    let artifacts = vec![
        make_artifact("a1", "evtx"),
        make_artifact("a2", "prefetch"),
        make_artifact("a3", "evtx"),
    ];
    repo.insert_batch(&artifacts, "case-1", "ds-1").unwrap();

    let evtx = repo.list_by_family(Some("evtx")).unwrap();
    assert_eq!(evtx.len(), 2);
    assert!(evtx.iter().all(|a| a.family == "evtx"));

    let all = repo.list_by_family(None).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn artifact_provenance_round_trips() {
    let conn = setup_db();
    let repo = ArtifactRepo::new(&conn);
    let mut artifact = make_artifact("a1", "prefetch");
    artifact.extractor_id = Some("prefetch".to_string());
    artifact.extractor_version = Some("1.2.3".to_string());
    artifact.confidence = Some(0.93);
    artifact.source_attribution = Some("Windows/Prefetch/CMD.EXE.pf".to_string());

    repo.insert_batch(&[artifact], "case-1", "ds-1").unwrap();

    let rows = repo.list_by_family(Some("prefetch")).unwrap();
    assert_eq!(rows[0].extractor_id.as_deref(), Some("prefetch"));
    assert_eq!(rows[0].extractor_version.as_deref(), Some("1.2.3"));
    assert_eq!(rows[0].confidence, Some(0.93));
    assert_eq!(
        rows[0].source_attribution.as_deref(),
        Some("Windows/Prefetch/CMD.EXE.pf")
    );
}

#[test]
fn artifact_null_provenance_loads_as_missing() {
    let conn = setup_legacy_db();
    conn.execute(
        "INSERT INTO artifacts (id, artifact_type, title, summary, attrs, created_at)
         VALUES ('a1', 'legacy', 'Legacy', '', '{}', '2026-06-04T00:00:00Z')",
        [],
    )
    .unwrap();
    let repo = ArtifactRepo::new(&conn);

    let rows = repo.list_by_family(Some("legacy")).unwrap();

    assert_eq!(rows.len(), 1);
    assert!(rows[0].extractor_id.is_none());
    assert!(rows[0].extractor_version.is_none());
    assert!(rows[0].confidence.is_none());
    assert!(rows[0].source_attribution.is_none());
}
