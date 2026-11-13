use super::*;
use persistence_source::fjall::{
    encode_key, instrumentation, KeyPart, KeyspaceName, PersistMode, WriteAction,
};

fn type_count_key(source_id: &str) -> Vec<u8> {
    encode_key(
        "artifact.type-count-schema",
        &[KeyPart::Text(source_id.to_owned())],
    )
    .unwrap()
}

fn case_counts(connection: &rusqlite::Connection, root: &std::path::Path) -> Vec<(String, u64)> {
    get_artifact_family_counts_for_case(connection, root, &domain::CaseId("case-1".into()))
        .unwrap()
        .into_iter()
        .map(|item| (item.family, item.count))
        .collect()
}

#[test]
fn case_family_facets_use_type_counts_and_preserve_source_platforms() {
    let root = tempfile::tempdir().unwrap();
    let case_conn = in_memory_case_db();
    let first = open_fixture_source(root.path(), &DataSourceId("ds-1".into()));
    let second = register_ready_source(&case_conn, root.path(), "ds-2");
    case_conn
        .execute(
            "UPDATE data_sources SET platform='windows' WHERE id='ds-2'",
            [],
        )
        .unwrap();
    let mut expected = BTreeMap::<String, u64>::new();
    for (source_id, connection) in [("ds-1", &first), ("ds-2", &second)] {
        let records = (0..2_048)
            .map(|index| {
                let family = if index % 2 == 0 {
                    "EventLog"
                } else {
                    "Prefetch"
                };
                *expected.entry(family.to_owned()).or_default() += 1;
                let mut artifact = make_artifact(family, "family-count");
                artifact.id = ArtifactId(format!("artifact-{index}"));
                artifact
            })
            .collect::<Vec<_>>();
        store_artifacts(connection, &records, "case-1", source_id).unwrap();
    }
    instrumentation::enable();
    instrumentation::reset();
    let observed = case_counts(&case_conn, root.path());
    let families =
        get_artifact_families_for_case(&case_conn, root.path(), &domain::CaseId("case-1".into()))
            .unwrap();
    let sources = crate::artifact_service::get_source_attributed_artifact_family_counts_for_case(
        &case_conn,
        root.path(),
        &domain::CaseId("case-1".into()),
    )
    .unwrap();
    let counters = instrumentation::snapshot();
    instrumentation::disable();
    assert_eq!(observed, expected.into_iter().collect::<Vec<_>>());
    assert_eq!(families, vec!["EventLog", "Prefetch"]);
    assert_eq!(sources.len(), 4);
    for item in sources {
        assert_eq!(item.count, 1_024);
        let expected_platform = if item.data_source_id.0 == "ds-1" {
            domain::DataSourcePlatform::Linux
        } else {
            assert_eq!(item.data_source_id.0, "ds-2");
            domain::DataSourcePlatform::Windows
        };
        assert_eq!(item.platform, expected_platform);
    }
    assert!(counters.scan_entries < 100, "record scan: {counters:?}");
    assert!(counters.point_reads < 100, "record reads: {counters:?}");
}

#[test]
fn empty_case_family_facets_need_no_type_count_marker() {
    let root = tempfile::tempdir().unwrap();
    let case_conn = in_memory_case_db();
    let _source = open_fixture_source(root.path(), &DataSourceId("ds-1".into()));
    assert!(case_counts(&case_conn, root.path()).is_empty());
    assert!(get_artifact_families_for_case(
        &case_conn,
        root.path(),
        &domain::CaseId("case-1".into()),
    )
    .unwrap()
    .is_empty());
}

#[test]
fn nonempty_legacy_source_requires_type_count_preparation() {
    let root = tempfile::tempdir().unwrap();
    let case_conn = in_memory_case_db();
    let source = open_fixture_source(root.path(), &DataSourceId("ds-1".into()));
    store_artifacts(
        &source,
        &[make_artifact("EventLog", "event")],
        "case-1",
        "ds-1",
    )
    .unwrap();
    let store = persistence_source::detail::require_source_store(&source).unwrap();
    store
        .database()
        .write_batch(
            &[WriteAction::Delete {
                keyspace: KeyspaceName::Meta,
                key: type_count_key("ds-1"),
            }],
            PersistMode::SyncAll,
        )
        .unwrap();
    let error = get_artifact_family_counts_for_case(
        &case_conn,
        root.path(),
        &domain::CaseId("case-1".into()),
    )
    .unwrap_err();
    assert!(matches!(error, ArtifactServiceError::InvalidInput(_)));
    assert!(error.to_string().contains("re-run data-source analysis"));
}

#[test]
fn malformed_type_count_marker_is_reported_without_scanning_records() {
    let root = tempfile::tempdir().unwrap();
    let case_conn = in_memory_case_db();
    let source = open_fixture_source(root.path(), &DataSourceId("ds-1".into()));
    let store = persistence_source::detail::require_source_store(&source).unwrap();
    store
        .database()
        .write_batch(
            &[WriteAction::Put {
                keyspace: KeyspaceName::Meta,
                key: type_count_key("ds-1"),
                value: b"invalid".to_vec(),
            }],
            PersistMode::SyncAll,
        )
        .unwrap();
    let error =
        get_artifact_families_for_case(&case_conn, root.path(), &domain::CaseId("case-1".into()))
            .unwrap_err();
    assert!(matches!(error, ArtifactServiceError::Other(_)));
    assert!(error.to_string().contains("schema marker is malformed"));
}
