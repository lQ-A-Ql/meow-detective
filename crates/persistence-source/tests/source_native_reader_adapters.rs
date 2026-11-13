use chrono::Utc;
use domain::{
    Artifact, ArtifactId, DataSourceId, EntryType, FileEntry, FileEntryId, TimelineEvent,
};
use persistence_source::repositories::{
    artifact_repo::ArtifactRepo, content_index_repo::ContentIndexRepo, correlation_repo,
    file_repo::FileRepo, infrastructure_network_fact_repo::InfrastructureNetworkFactRepo,
    systemd_unit_repo::SystemdUnitRepo, timeline_repo::TimelineRepo,
};
use rusqlite::Connection;
use std::collections::BTreeMap;

const SOURCE: &str = "source-native-readers";

fn setup() -> (tempfile::TempDir, Connection) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.db");
    let connection =
        persistence_source::control::opening::open_source_database(&path, SOURCE).unwrap();
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, 'case-1', 'Native readers', 'linux', '/evidence', ?2)",
            rusqlite::params![SOURCE, Utc::now().to_rfc3339()],
        )
        .unwrap();
    (root, connection)
}

fn file(id: &str, path: &str, name: &str) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.into()),
        parent_id: None,
        data_source_id: DataSourceId(SOURCE.into()),
        path: path.into(),
        name: name.into(),
        entry_type: EntryType::File,
        size: Some(10),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: Some(0o644),
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn artifact(id: &str, object: &str, path: &str, line: &str) -> Artifact {
    Artifact {
        id: ArtifactId(id.into()),
        family: "LinuxSystemConfig".into(),
        title: format!("title {id}"),
        summary: format!("summary {id}"),
        source_object_id: Some(FileEntryId(object.into())),
        extractor_id: Some("linux.system_config".into()),
        extractor_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: Some(path.into()),
        created_at: Utc::now(),
        attrs: BTreeMap::from([
            ("sourcePath".into(), path.into()),
            ("line".into(), line.into()),
            ("lineNumber".into(), 7.into()),
            ("configKind".into(), "textConfig".into()),
        ]),
    }
}

fn timeline(id: &str, object: &str) -> TimelineEvent {
    TimelineEvent {
        id: domain::TimelineEventId(id.into()),
        source_object_id: object.into(),
        event_type: "FILE_MODIFIED".into(),
        timestamp: Utc::now(),
        title: "Modified".into(),
        description: "Modified file".into(),
        parser_id: Some("timeline.file_modified".into()),
        parser_version: Some("2".into()),
        confidence: Some(1.0),
        source_attribution: Some("filesystem".into()),
        attrs: BTreeMap::new(),
    }
}

#[test]
fn native_reader_adapters_use_source_catalog_and_native_details() {
    let (_root, connection) = setup();
    FileRepo::new(&connection)
        .insert_batch(&[
            file(
                "systemd",
                "/etc/systemd/system/demo.service",
                "demo.service",
            ),
            file("hosts", "/etc/hosts", "hosts"),
            file("unrelated", "/var/log/app.log", "app.log"),
        ])
        .unwrap();
    ArtifactRepo::new(&connection)
        .insert_batch(
            &[
                artifact("hosts-artifact", "hosts", "/etc/hosts", "127.0.0.1 local"),
                artifact(
                    "other-artifact",
                    "unrelated",
                    "/var/log/app.log",
                    "not network config",
                ),
            ],
            "case-1",
            SOURCE,
        )
        .unwrap();
    TimelineRepo::new(&connection)
        .insert_batch_with_case(&[timeline("event", "hosts")], "case-1")
        .unwrap();

    let systemd = SystemdUnitRepo::new(&connection)
        .catalog_page(SOURCE, "")
        .unwrap();
    assert_eq!(
        systemd
            .iter()
            .map(|row| row.file_id.as_str())
            .collect::<Vec<_>>(),
        vec!["systemd"]
    );

    let text = ContentIndexRepo::new(&connection)
        .artifact_text(SOURCE, "hosts", "ds:source-native-readers:hosts")
        .unwrap();
    assert!(text.contains("title hosts-artifact"));
    assert!(text.contains("127.0.0.1 local"));

    let rows = InfrastructureNetworkFactRepo::list_source_config_rows(&connection, 8).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].artifact_id, "hosts-artifact");
    assert_eq!(rows[0].file_id, "hosts");
    assert_eq!(rows[0].source_path, "/etc/hosts");
    assert_eq!(rows[0].line_number, 7);
    assert_eq!(rows[0].line, "127.0.0.1 local");

    assert_eq!(
        correlation_repo::collect_artifact_ids(&connection).unwrap(),
        vec!["hosts-artifact", "other-artifact"]
    );
    assert_eq!(
        correlation_repo::resolve_case_id(&connection).unwrap(),
        Some("case-1".into())
    );
    // Artifact writes bump the artifact revision counter; reading it back also
    // proves the correlation read path observes native storage read-only.
    assert!(ArtifactRepo::new(&connection).cursor_revision().unwrap() > 0);
}

#[test]
fn native_artifact_cursor_and_family_count_read_created_indexes() {
    let (_root, connection) = setup();
    let mut newest = artifact("newest", "hosts", "/etc/hosts", "2026-01-03T00:00:00Z");
    newest.created_at = "2026-01-03T00:00:00Z".parse().unwrap();
    let mut middle = artifact("middle", "hosts", "/etc/hosts", "2026-01-02T00:00:00Z");
    middle.created_at = "2026-01-02T00:00:00Z".parse().unwrap();
    let mut oldest = artifact("oldest", "hosts", "/etc/hosts", "2026-01-01T00:00:00Z");
    oldest.created_at = "2026-01-01T00:00:00Z".parse().unwrap();
    let repository = ArtifactRepo::new(&connection);
    repository
        .insert_batch(&[oldest, middle, newest], "case-1", SOURCE)
        .unwrap();

    let snapshot = repository.snapshot_high_water().unwrap();
    assert_eq!(
        repository
            .count_for_family_at_snapshot(Some("LinuxSystemConfig"), snapshot)
            .unwrap(),
        3
    );
    let first = repository
        .list_by_family_after_at_snapshot(Some("LinuxSystemConfig"), None, snapshot, 2)
        .unwrap();
    assert_eq!(
        first
            .iter()
            .map(|row| row.sort_key.artifact_id.as_str())
            .collect::<Vec<_>>(),
        vec!["newest", "middle"]
    );
    let second = repository
        .list_by_family_after_at_snapshot(
            Some("LinuxSystemConfig"),
            Some(&first.last().unwrap().sort_key),
            snapshot,
            2,
        )
        .unwrap();
    assert_eq!(
        second
            .iter()
            .map(|row| row.sort_key.artifact_id.as_str())
            .collect::<Vec<_>>(),
        vec!["oldest"]
    );
}

#[test]
fn native_timeline_queries_stream_time_and_type_indexes() {
    let (_root, connection) = setup();
    let mut newest = timeline("newest", "object");
    newest.timestamp = "2026-01-03T00:00:00Z".parse().unwrap();
    let mut middle = timeline("middle", "object");
    middle.timestamp = "2026-01-02T00:00:00Z".parse().unwrap();
    middle.event_type = "LOGON".into();
    let mut oldest = timeline("oldest", "object");
    oldest.timestamp = "2026-01-01T00:00:00Z".parse().unwrap();
    TimelineRepo::new(&connection)
        .insert_batch_with_case(&[oldest, middle, newest], "case-1")
        .unwrap();
    let repository = TimelineRepo::new(&connection);

    assert_eq!(
        repository
            .query_filtered(
                0,
                2,
                Some("2026-01-01T00:00:00Z"),
                Some("2026-01-03T00:00:00Z"),
                None,
            )
            .unwrap()
            .into_iter()
            .map(|event| event.id.0)
            .collect::<Vec<_>>(),
        vec!["newest", "middle"]
    );
    assert_eq!(
        repository
            .query_filtered(0, 10, None, None, Some("FILE_MODIFIED"),)
            .unwrap()
            .into_iter()
            .map(|event| event.id.0)
            .collect::<Vec<_>>(),
        vec!["newest", "oldest"]
    );
    assert_eq!(
        repository
            .count_filtered(
                Some("2026-01-02T00:00:00Z"),
                Some("2026-01-03T00:00:00Z"),
                None,
            )
            .unwrap(),
        2
    );
    assert_eq!(
        repository.find_by_id("middle").unwrap().unwrap().id.0,
        "middle"
    );
}
