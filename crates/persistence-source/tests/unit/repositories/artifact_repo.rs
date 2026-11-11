use super::*;
use domain::ArtifactId;
use std::collections::BTreeMap;

fn setup_db() -> (tempfile::TempDir, rusqlite::Connection) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.db");
    let conn = crate::control::opening::open_source_database(&path, "ds-1").unwrap();
    crate::detail::open_store_for_write(&conn).unwrap().unwrap();
    (root, conn)
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

fn make_artifact_at(id: &str, family: &str, created_at: &str) -> Artifact {
    let mut artifact = make_artifact(id, family);
    artifact.created_at = chrono::DateTime::parse_from_rfc3339(created_at)
        .unwrap()
        .with_timezone(&chrono::Utc);
    artifact
}

#[test]
fn insert_batch_then_count_returns_correct_number() {
    let (_root, conn) = setup_db();
    let repo = ArtifactRepo::new(&conn);
    repo.insert_batch(
        &[
            make_artifact("a1", "evtx"),
            make_artifact("a2", "prefetch"),
            make_artifact("a3", "evtx"),
        ],
        "case-1",
        "ds-1",
    )
    .unwrap();
    assert_eq!(repo.count().unwrap(), 3);
}

#[test]
fn families_returns_distinct_types() {
    let (_root, conn) = setup_db();
    let repo = ArtifactRepo::new(&conn);
    repo.insert_batch(
        &[
            make_artifact("a1", "evtx"),
            make_artifact("a2", "prefetch"),
            make_artifact("a3", "evtx"),
        ],
        "case-1",
        "ds-1",
    )
    .unwrap();
    assert_eq!(repo.families().unwrap(), ["evtx", "prefetch"]);
}

#[test]
fn count_by_family_returns_grouped_counts() {
    let (_root, conn) = setup_db();
    let repo = ArtifactRepo::new(&conn);
    repo.insert_batch(
        &[
            make_artifact("a1", "evtx"),
            make_artifact("a2", "prefetch"),
            make_artifact("a3", "evtx"),
        ],
        "case-1",
        "ds-1",
    )
    .unwrap();
    assert_eq!(
        repo.count_by_family().unwrap(),
        vec![("evtx".to_string(), 2), ("prefetch".to_string(), 1)]
    );
}

#[test]
fn list_by_family_filters_correctly() {
    let (_root, conn) = setup_db();
    let repo = ArtifactRepo::new(&conn);
    repo.insert_batch(
        &[
            make_artifact("a1", "evtx"),
            make_artifact("a2", "prefetch"),
            make_artifact("a3", "evtx"),
        ],
        "case-1",
        "ds-1",
    )
    .unwrap();
    let evtx = repo.list_by_family(Some("evtx")).unwrap();
    assert_eq!(evtx.len(), 2);
    assert!(evtx.iter().all(|artifact| artifact.family == "evtx"));
    assert_eq!(repo.list_by_family(None).unwrap().len(), 3);
}

#[test]
fn list_by_family_keyset_is_stable_and_count_matches_filter() {
    let (_root, conn) = setup_db();
    let repo = ArtifactRepo::new(&conn);
    repo.insert_batch(
        &[
            make_artifact_at("a-latest-b", "evtx", "2026-06-03T00:00:00Z"),
            make_artifact_at("other", "prefetch", "2026-06-04T00:00:00Z"),
            make_artifact_at("a-old", "evtx", "2026-06-01T00:00:00Z"),
            make_artifact_at("a-latest-a", "evtx", "2026-06-03T00:00:00Z"),
            make_artifact_at("a-middle", "evtx", "2026-06-02T00:00:00Z"),
        ],
        "case-1",
        "ds-1",
    )
    .unwrap();
    let first = repo.list_by_family_after(None, None, 2).unwrap();
    let second = repo
        .list_by_family_after(Some("evtx"), Some(&first.last().unwrap().sort_key), 2)
        .unwrap();
    assert_eq!(repo.count_for_family(Some("evtx")).unwrap(), 4);
    assert_eq!(repo.count_for_family(None).unwrap(), 5);
    assert_eq!(
        first
            .iter()
            .map(|row| row.artifact.id.0.as_str())
            .collect::<Vec<_>>(),
        ["other", "a-latest-a"]
    );
    assert_eq!(
        second
            .iter()
            .map(|row| row.artifact.id.0.as_str())
            .collect::<Vec<_>>(),
        ["a-latest-b", "a-middle"]
    );
}

#[test]
fn artifact_page_rejects_unbounded_limit() {
    let (_root, conn) = setup_db();
    let repo = ArtifactRepo::new(&conn);
    repo.insert_batch(&[make_artifact("a1", "evtx")], "case-1", "ds-1")
        .unwrap();
    let error = repo.list_by_family_after(None, None, 1_025).unwrap_err();
    assert!(error.to_string().contains("bounded page limit"));
}

#[test]
fn artifact_provenance_round_trips() {
    let (_root, conn) = setup_db();
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
fn artifact_missing_provenance_round_trips_as_none() {
    let (_root, conn) = setup_db();
    let repo = ArtifactRepo::new(&conn);
    repo.insert_batch(&[make_artifact("a1", "legacy")], "case-1", "ds-1")
        .unwrap();
    let rows = repo.list_by_family(Some("legacy")).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].extractor_id.is_none());
    assert!(rows[0].extractor_version.is_none());
    assert!(rows[0].confidence.is_none());
    assert!(rows[0].source_attribution.is_none());
}
