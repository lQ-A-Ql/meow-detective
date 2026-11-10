use super::*;
use evidence_core::FileSystemReader;

#[test]
fn search_oracle_canonicalizes_generated_id_ties_without_changing_membership() {
    fn capture(reverse: bool) -> serde_json::Value {
        let temp = tempfile::tempdir().unwrap();
        let index = search::SearchIndex::create(temp.path()).unwrap();
        let mut writer = index.metadata_writer().unwrap();
        let low = "00000000-0000-4000-8000-000000000001";
        let high = "ffffffff-ffff-4fff-8fff-ffffffffffff";
        let raw = if reverse { [high, low] } else { [low, high] };
        let documents: Vec<_> = raw
            .iter()
            .zip(["Same.txt", "same.txt"])
            .map(|(id, path)| search::SearchFileDocument {
                file_id: (*id).into(),
                path: path.into(),
                name: path.into(),
                extension: "txt".into(),
                entry_type: "file".into(),
                size: Some(1),
                modified_at: None,
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
            })
            .collect();
        writer.add_documents(&documents).unwrap();
        writer.commit().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        let normalizer = streaming::configure(
            &conn,
            vec![
                (raw[0].into(), "__file_A__".into()),
                (raw[1].into(), "__file_B__".into()),
            ],
        );
        semantics::search_ids(&index, "same", &normalizer)
    }
    let forward = capture(false);
    let reversed = capture(true);
    assert_eq!(forward["documents"], 2);
    assert_eq!(forward, reversed);
}

#[test]
fn catalog_sidecar_localizes_each_business_field_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("fixture.raw");
    std::fs::write(&path, fixture::image(32)).unwrap();
    with_import(&path, true, 1, |source, case, id, _| {
        let replacements = canonical_replacements(source, case, id);
        let original = native_catalog_snapshot(source, &replacements);
        let normalizer = streaming::configure(source, replacements);
        let before = semantics::catalog_from_rows(&original);
        let target = original
            .iter()
            .find(|row| matches!(&row[3], Value::Text(name) if name == "file-16.txt"))
            .map(|row| match &row[0] {
                Value::Text(id) => id.clone(),
                _ => panic!("catalog id is not text"),
            })
            .unwrap();
        for (column, index, value) in [
            ("path", 2, Value::Text("changed/path".into())),
            ("parent_id", 1, Value::Text("mft:0:999".into())),
            ("size", 5, Value::Integer(987)),
            (
                "modified_at",
                15,
                Value::Text("2026-10-16T12:00:00Z".into()),
            ),
            ("hidden", 8, Value::Integer(1)),
        ] {
            let mut changed_rows = original.clone();
            let row = changed_rows
                .iter_mut()
                .find(|row| matches!(&row[0], Value::Text(id) if id == &target))
                .unwrap();
            row[index] = value;
            let after = semantics::catalog_from_rows(&changed_rows);
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
            assert_eq!(before, semantics::catalog_from_rows(&original));
        }
        drop(normalizer);
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
                persistence_sqlite::repositories::timeline_repo::TimelineRepo::new(conn)
                    .count()
                    .unwrap()
            );
            let writable = source_db::open_source_db(root, &DataSourceId(source.into())).unwrap();
            let revision = SourceMetaRepo::new(&writable)
                .read(TIMELINE_CURSOR_REVISION_KEY)
                .unwrap();
            let mut before_rows = native_timeline_snapshot(conn, &normalizer.replacements());
            before_rows.extend(snapshot(conn, queries::NODES, &normalizer.replacements()));
            before_rows.extend(snapshot(conn, queries::EDGES, &normalizer.replacements()));
            let before = streaming::fingerprint_rows(before_rows.clone(), &normalizer);
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
            assert_eq!(before, {
                let mut after_rows = native_timeline_snapshot(conn, &normalizer.replacements());
                after_rows.extend(snapshot(conn, queries::NODES, &normalizer.replacements()));
                after_rows.extend(snapshot(conn, queries::EDGES, &normalizer.replacements()));
                streaming::fingerprint_rows(after_rows, &normalizer)
            });
            let fjall = app_services::source_db::open_registered_fjall_source_read_write(
                root,
                &DataSourceId(source.into()),
            )
            .unwrap();
            let invalidation_event = persistence_fjall::TimelineRecord {
                id: "__cursor-invalidation__".into(),
                data_source_id: source.into(),
                source_object_id: "__probe__".into(),
                event_type: "Fixture".into(),
                timestamp: chrono::Utc::now(),
                title: "cursor invalidation".into(),
                description: String::new(),
                parser_id: Some("fixture".into()),
                parser_version: Some("1".into()),
                confidence: Some(1.0),
                source_attribution: None,
                attrs: Default::default(),
            };
            fjall
                .timeline_upsert(&invalidation_event, persistence_fjall::PersistMode::SyncAll)
                .unwrap();
            fjall
                .timeline_delete(
                    source,
                    &invalidation_event.id,
                    persistence_fjall::PersistMode::SyncAll,
                )
                .unwrap();
            drop(fjall);
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
            let id: String = native_catalog_snapshot(conn, &normalizer.replacements())
                .iter()
                .find(|row| matches!(&row[3], Value::Text(name) if name == "file-16.txt"))
                .and_then(|row| match &row[0] {
                    Value::Text(id) => Some(id.clone()),
                    _ => None,
                })
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
