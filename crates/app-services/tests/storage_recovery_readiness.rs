use app_services::active_case::ActiveCase;
use app_services::storage_recovery_service::{scan_case, RecoveryDisposition};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::control::import_state::{record_import_target, ImportedSourceState};
use persistence_source::fjall::{
    encode_key, KeyPart, KeyspaceName, PersistMode, SourceBuildLayout, SourceStore,
    VersionedEnvelope, WriteAction,
};
use persistence_source::runtime::search_marker::SearchProjectionMarker;
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use persistence_sqlite::repositories::storage_control_repo::{
    StorageControlRepo, StorageOperationRecord, StorageOperationState,
};

struct Fixture {
    _root: tempfile::TempDir,
    case: ActiveCase,
    source: DataSourceId,
    digest: String,
}

fn fixture(initial: &str, target: ImportedSourceState, published: bool) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let case =
        app_services::case_service::create_case(root.path(), "Import recovery", None).unwrap();
    let source = DataSourceId("readiness-source".into());
    let data_source = DataSource {
        id: source.clone(),
        name: "Recovery source".into(),
        kind: DataSourceKind::E01,
        source_path: root.path().join("source.E01"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source.0, Some("linux"), None);
    storage.import_state = initial.into();
    case.with_conn(|conn| {
        DataSourceRepo::new(conn).insert_with_storage(&case.meta.id, &data_source, &storage)
    })
    .unwrap();
    let control = app_services::source_db::open_source_db(&case.case_root, &source).unwrap();
    record_import_target(&control, target).unwrap();
    let input_revision =
        persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(&control)
            .read_revision(
                persistence_sqlite::repositories::source_meta_repo::SEARCH_INPUT_REVISION_KEY,
            )
            .unwrap();
    drop(control);

    let layout =
        SourceBuildLayout::for_case_source(&case.case_root, &source.0, "recovery-attempt").unwrap();
    let store = SourceStore::open(layout.build_path(), Default::default()).unwrap();
    let index_path = case
        .with_conn(|conn| {
            app_services::source_db::registered_source_index_dir(conn, &case.case_root, &source)
        })
        .unwrap();
    let index = search::SearchIndex::create(&index_path).unwrap();
    let marker = SearchProjectionMarker {
        source_id: source.0.clone(),
        input_revision: input_revision.to_string(),
        generation: index.generation().into(),
        schema_version: index.schema_version(),
        index_path: index_path.to_string_lossy().into_owned(),
    };
    let marker = VersionedEnvelope::from_serializable(
        "search_projection_generation",
        1,
        "search",
        &source.0,
        1,
        &marker,
    )
    .unwrap();
    store
        .write_envelope(
            KeyspaceName::Meta,
            encode_key("search", &[KeyPart::Text(source.0.clone())]).unwrap(),
            &marker,
            PersistMode::SyncAll,
        )
        .unwrap();
    let receipt = store
        .database()
        .write_batch(
            &[WriteAction::Put {
                keyspace: KeyspaceName::Meta,
                key: b"import-complete".to_vec(),
                value: b"ready".to_vec(),
            }],
            PersistMode::SyncAll,
        )
        .unwrap();
    let digest = hex::encode(receipt.batch_digest);
    persistence_source::fjall::seal_source_generation_with_control(
        &store,
        &source.0,
        "recovery-attempt",
        &digest,
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    drop(store);
    if published {
        persistence_source::fjall::publish_source_generation(
            layout.build_path(),
            layout.published_path(),
        )
        .unwrap();
    }
    case.with_conn(|conn| {
        StorageControlRepo::new(conn).insert_operation(&StorageOperationRecord {
            operation_id: "source-import:recovery-attempt".into(),
            case_id: case.meta.id.0.clone(),
            source_id: Some(source.0.clone()),
            attempt_id: "recovery-attempt".into(),
            operation_kind: "source_import".into(),
            state: StorageOperationState::Committed,
            input_digest: None,
            expected_batch_digest: Some(digest.clone()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            last_error_code: None,
        })
    })
    .unwrap();
    Fixture {
        _root: root,
        case,
        source,
        digest,
    }
}

fn recover(fixture: &Fixture) -> RecoveryDisposition {
    let report = fixture
        .case
        .with_conn(|conn| {
            scan_case(conn, &fixture.case.case_root, &fixture.case.meta.id.0)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert_eq!(report.disposition, RecoveryDisposition::Ready, "{report:?}");
    report.disposition
}

fn assert_published(fixture: &Fixture, readiness: &str) {
    fixture
        .case
        .with_conn(|conn| {
            let operation = StorageControlRepo::new(conn)
                .find_operation("source-import:recovery-attempt")?
                .unwrap();
            assert_eq!(operation.state, StorageOperationState::Published);
            assert_eq!(
                operation.expected_batch_digest.as_deref(),
                Some(fixture.digest.as_str())
            );
            assert_eq!(
                DataSourceRepo::new(conn)
                    .find_storage(&fixture.source)?
                    .unwrap()
                    .import_state,
                readiness
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn committed_import_recovers_readiness_and_publication_together() {
    let fixture = fixture("importing", ImportedSourceState::Ready, true);
    assert_eq!(recover(&fixture), RecoveryDisposition::Ready);
    assert_published(&fixture, "ready");
    assert_eq!(recover(&fixture), RecoveryDisposition::Ready);
}

#[test]
fn committed_ready_window_recovers_without_losing_the_receipt() {
    let fixture = fixture("ready", ImportedSourceState::Ready, true);
    assert_eq!(recover(&fixture), RecoveryDisposition::Ready);
    assert_published(&fixture, "ready");
}

#[test]
fn sealed_metadata_import_recovers_the_metadata_only_state() {
    let fixture = fixture("importing", ImportedSourceState::ReadyMetadata, false);
    assert_eq!(recover(&fixture), RecoveryDisposition::Ready);
    assert_published(&fixture, "ready_metadata");
}

#[test]
fn publication_failure_rolls_back_the_source_readiness_update() {
    let fixture = fixture("importing", ImportedSourceState::Ready, true);
    fixture
        .case
        .with_conn(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER reject_publication BEFORE UPDATE OF state ON storage_operations
            WHEN NEW.state='published' BEGIN SELECT RAISE(ABORT, 'publication rejected'); END;",
            )?;
            assert!(scan_case(conn, &fixture.case.case_root, &fixture.case.meta.id.0).is_err());
            assert_eq!(
                DataSourceRepo::new(conn)
                    .find_storage(&fixture.source)?
                    .unwrap()
                    .import_state,
                "importing"
            );
            assert_eq!(
                StorageControlRepo::new(conn)
                    .find_operation("source-import:recovery-attempt")?
                    .unwrap()
                    .state,
                StorageOperationState::Committed
            );
            Ok(())
        })
        .unwrap();
}
