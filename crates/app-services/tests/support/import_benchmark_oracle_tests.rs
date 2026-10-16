use super::*;
use evidence_core::FileSystemReader;

#[test]
fn catalog_sidecar_localizes_each_business_field_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fixture.raw");
    std::fs::write(&path, fixture::image(32)).unwrap();
    with_import(&path, true, 1, |source, case, id, _| {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("ATTACH ?1 AS original", [source.path().unwrap()])
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE file_entries AS SELECT * FROM original.file_entries; DETACH original",
        )
        .unwrap();
        let normalizer = streaming::configure(&conn, canonical_replacements(&conn, case, id));
        let before = semantics::catalog(&conn, &normalizer);
        let target: String = conn
            .query_row(
                "SELECT id FROM file_entries WHERE name='file-16.txt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        for (column, value) in [
            ("path", "'changed/path'"),
            ("parent_id", "'mft:0:999'"),
            ("size", "987"),
            ("modified_at", "'2026-10-16T12:00:00Z'"),
            ("hidden", "1"),
        ] {
            conn.execute_batch("SAVEPOINT mutation").unwrap();
            assert_eq!(
                conn.execute(
                    &format!("UPDATE file_entries SET {column}={value} WHERE id=?1"),
                    [&target]
                )
                .unwrap(),
                1
            );
            let after = semantics::catalog(&conn, &normalizer);
            let changed: Vec<_> = before["columns"]
                .as_object()
                .unwrap()
                .iter()
                .filter_map(|(name, hash)| {
                    (hash != &after["columns"][name]).then_some(name.as_str())
                })
                .collect();
            assert_eq!(changed, [column], "field localization");
            assert_ne!(before["idHashBuckets"], after["idHashBuckets"]);
            conn.execute_batch("ROLLBACK TO mutation; RELEASE mutation")
                .unwrap();
            assert_eq!(before, semantics::catalog(&conn, &normalizer));
        }
    });
}

#[test]
fn indexed_fragmented_scenario_is_reproducible_and_preserves_catalog_and_bytes() {
    let bytes = scenarios::image(128, "indexed-fragmented");
    assert_eq!(bytes, scenarios::image(128, "indexed-fragmented"));
    let temp = tempfile::tempdir().unwrap();
    let raw = temp.path().join("rich.raw");
    let e01 = temp.path().join("rich.E01");
    std::fs::write(&raw, &bytes).unwrap();
    fixture::write_e01(&e01, &bytes).unwrap();
    let ntfs = fs_ntfs::NtfsReader::open(
        Box::new(evidence_core::RawImageReader::open(&raw).unwrap()),
        0,
    )
    .unwrap();
    let listing = ntfs.list_root_directory_entries().unwrap();
    assert!(listing.iter().any(|e| e.name == "indexed-17.txt"));
    let mut logical = Vec::new();
    ntfs.open_file("mft:17")
        .unwrap()
        .read_to_end(&mut logical)
        .unwrap();
    assert_eq!(
        logical,
        [vec![b'A'; 1024], vec![b'B'; 276], vec![0; 500]].concat()
    );
    assert_eq!(
        ntfs.file_size_by_inode(18).unwrap(),
        Some(fixture::FILE_CONTENT.len() as u64)
    );
    let first = import_with_workers(&raw, true, 1);
    let second = import_with_workers(&e01, true, 4);
    assert_eq!(first.catalog, second.catalog);
    assert_eq!(first.graph, second.graph);
    assert_eq!(first.timeline, second.timeline);
    let a = streaming::profile(&raw, 1);
    let b = streaming::profile(&e01, 4);
    assert_eq!(a["semanticOracle"], b["semanticOracle"]);
    assert!(
        a["semanticOracle"]["catalog"]["directories"]
            .as_u64()
            .unwrap()
            >= 2
    );
    assert!(a["semanticOracle"]["catalog"]["deleted"].as_u64().unwrap() > 0);
}

#[test]
fn actual_import_search_preview_extraction_and_projection_retry_are_stable() {
    use app_services::{
        file_service,
        timeline_service::{self, TimelineQuery},
    };
    use persistence_sqlite::repositories::source_meta_repo::{
        SourceMetaRepo, TIMELINE_CURSOR_REVISION_KEY,
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("full.raw");
    std::fs::write(&path, fixture::image(32)).unwrap();
    let metadata = streaming::profile(&path, 1);
    with_import_context(
        &path,
        true,
        4,
        ImportAnalysisMode::FullContent,
        |conn, case_conn, root, case, source, _| {
            let normalizer = streaming::configure(conn, canonical_replacements(conn, case, source));
            let oracle = semantics::capture(conn, case_conn, root, case, source, &normalizer);
            assert_eq!(oracle["catalog"], metadata["semanticOracle"]["catalog"]);
            assert_eq!(
                oracle["previewExtraction"],
                metadata["semanticOracle"]["previewExtraction"]
            );
            assert!(
                oracle["contentSearch"][2]["documents"].as_u64().unwrap() > 0,
                "full-content hello query"
            );
            assert_eq!(metadata["semanticOracle"]["search"][2]["documents"], 0);
            let case_id = domain::CaseId(case.into());
            let first = timeline_service::query_timeline_filtered_for_case(
                case_conn,
                root,
                &case_id,
                TimelineQuery::unfiltered(0, 7),
            )
            .unwrap();
            let initial_cursor = first.next_cursor.clone().unwrap();
            let mut ids = std::collections::HashSet::new();
            let mut page = first;
            loop {
                for event in page.items {
                    assert!(event.id.starts_with(&format!("ds:{source}:")));
                    assert!(ids.insert(event.id));
                }
                let Some(cursor) = page.next_cursor else {
                    break;
                };
                page = timeline_service::query_timeline_filtered_for_case(
                    case_conn,
                    root,
                    &case_id,
                    TimelineQuery {
                        cursor: Some(&cursor),
                        ..TimelineQuery::unfiltered(0, 7)
                    },
                )
                .unwrap();
            }
            assert_eq!(
                ids.len() as u64,
                conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r
                    .get::<_, u64>(0))
                    .unwrap()
            );
            let writable = source_db::open_source_db(root, &DataSourceId(source.into())).unwrap();
            let revision = SourceMetaRepo::new(&writable)
                .read(TIMELINE_CURSOR_REVISION_KEY)
                .unwrap();
            let before = streaming::fingerprint(
                conn,
                &[queries::TIMELINE, queries::NODES, queries::EDGES],
                &normalizer,
            );
            let retry = timeline_service::materialize_file_activity(
                &writable,
                DataSourcePlatform::Windows,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(retry.already_projected && retry.graph_complete);
            assert_eq!(
                revision,
                SourceMetaRepo::new(&writable)
                    .read(TIMELINE_CURSOR_REVISION_KEY)
                    .unwrap()
            );
            assert_eq!(
                before,
                streaming::fingerprint(
                    conn,
                    &[queries::TIMELINE, queries::NODES, queries::EDGES],
                    &normalizer
                )
            );
            SourceMetaRepo::new(&writable)
                .write(TIMELINE_CURSOR_REVISION_KEY, "999999999")
                .unwrap();
            assert!(timeline_service::query_timeline_filtered_for_case(
                case_conn,
                root,
                &case_id,
                TimelineQuery {
                    cursor: Some(&initial_cursor),
                    ..TimelineQuery::unfiltered(0, 7)
                }
            )
            .is_err());
            let id: String = conn
                .query_row(
                    "SELECT id FROM file_entries WHERE name='file-16.txt'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let global = format!("ds:{source}:{id}");
            let handle =
                file_service::open_file_handle_for_case(case_conn, root, &case_id, &global)
                    .unwrap();
            let range = file_service::read_file_range_for_source_case(
                case_conn,
                root,
                &case_id,
                &transport::dto::ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: 2,
                    length: 5,
                },
            )
            .unwrap();
            assert_eq!(range.raw_bytes.unwrap(), fixture::FILE_CONTENT[2..7]);
            assert!(file_service::open_file_handle_for_case(
                case_conn,
                root,
                &case_id,
                &format!("ds:absent:{id}")
            )
            .is_err());
        },
    );
}
