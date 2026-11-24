use super::{generation, mirror_analysis_outputs_from_connections, projection_state};
use domain::DataSourceId;
use persistence_source::fjall::{KeyspaceName, PersistMode, TimelineRecord, VersionedEnvelope};

#[test]
fn both_analysis_projection_branches_return_the_published_receipt() {
    let root = tempfile::tempdir().unwrap();
    let source_id = DataSourceId("receipt-source".into());
    let source = source_kernel::source_db::open_source_db(root.path(), &source_id).unwrap();
    let case = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
    persistence_sqlite::runner::run_all(&case).unwrap();
    let now = chrono::Utc::now();
    persistence_sqlite::repositories::case_repo::CaseRepo::new(&case)
        .create(&domain::CaseMeta {
            id: domain::CaseId("case".into()),
            name: "Projection receipt".into(),
            number: None,
            examiner: None,
            notes: None,
            created_at: now,
            updated_at: now,
        })
        .unwrap();
    let native =
        source_kernel::source_db::open_fjall_source_for_build(&source, &source_id).unwrap();
    native
        .timeline_upsert(
            &TimelineRecord {
                id: "event".into(),
                data_source_id: source_id.0.clone(),
                source_object_id: "file".into(),
                event_type: "FILE_MODIFIED".into(),
                timestamp: chrono::Utc::now(),
                title: "Modified file".into(),
                description: "receipt regression".into(),
                parser_id: Some("file".into()),
                parser_version: Some("1".into()),
                confidence: None,
                source_attribution: None,
                attrs: Default::default(),
            },
            PersistMode::SyncAll,
        )
        .unwrap();
    native
        .timeline_rebuild_count_indexes_with_control(
            &source_id.0,
            PersistMode::SyncAll,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let initial = native
        .write_envelope(
            KeyspaceName::Meta,
            b"initial-publication".to_vec(),
            &VersionedEnvelope::new("publication", 1, "receipt", &source_id.0, 1, vec![]),
            PersistMode::SyncAll,
        )
        .unwrap();
    let expected = hex::encode(initial.batch_digest);
    persistence_source::fjall::seal_source_generation_with_control(
        native.source_store(),
        &source_id.0,
        "receipt-attempt",
        &expected,
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert!(
        projection_state::analysis_projection_marker(&native, &source_id.0)
            .unwrap()
            .is_none()
    );
    drop(native);

    let normal =
        mirror_analysis_outputs_from_connections(&case, root.path(), "case", &source, &source_id.0)
            .unwrap();
    assert_eq!(normal, expected);
    let current =
        source_kernel::source_db::open_registered_fjall_source_read_write(root.path(), &source_id)
            .unwrap();
    let before = projection_state::analysis_projection_marker(&current, &source_id.0)
        .unwrap()
        .unwrap();
    assert!(current
        .timeline_counts_ready(&current.snapshot(), &source_id.0)
        .unwrap());
    drop(current);

    let mut staged = analysis_core::analysis_service::stage_source_generation(
        &source,
        &source_id.0,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let bound_generation = staged.name.clone();
    staged.bind(&source).unwrap();
    staged.commit();
    let expected = generation::publication_receipt(&source, &source_id.0).unwrap();
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&case)
        .insert(
            &domain::CaseId("case".into()),
            &domain::DataSource {
                id: source_id.clone(),
                name: "source".into(),
                kind: domain::DataSourceKind::Raw,
                source_path: root.path().join("source.raw"),
                imported_at: now,
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )
        .unwrap();
    persistence_sqlite::repositories::storage_control_repo::StorageControlRepo::new(&case)
        .insert_operation(&persistence_sqlite::repositories::storage_control_repo::StorageOperationRecord {
            operation_id:"source-import:original".into(), case_id:"case".into(),
            source_id:Some(source_id.0.clone()), attempt_id:"receipt-attempt".into(),
            operation_kind:"source_import".into(),
            state:persistence_sqlite::repositories::storage_control_repo::StorageOperationState::Running,
            input_digest:None, expected_batch_digest:None, created_at:now.to_rfc3339(),
            updated_at:now.to_rfc3339(), last_error_code:None,
        }).unwrap();
    let unchanged =
        mirror_analysis_outputs_from_connections(&case, root.path(), "case", &source, &source_id.0)
            .unwrap();
    assert_eq!(unchanged, expected);
    let actual_binding: String = source
        .query_row(
            "SELECT value FROM source_meta WHERE key='fjall_root'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(actual_binding, bound_generation);
    let current =
        source_kernel::source_db::open_registered_fjall_source_read_write(root.path(), &source_id)
            .unwrap();
    assert_eq!(
        projection_state::analysis_projection_marker(&current, &source_id.0).unwrap(),
        Some(before)
    );
    assert_eq!(
        generation::publication_receipt(&source, &source_id.0).unwrap(),
        expected
    );
}

#[test]
fn a_manifest_without_its_physical_batch_receipt_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let source_id = DataSourceId("missing-receipt".into());
    let source = source_kernel::source_db::open_source_db(root.path(), &source_id).unwrap();
    let native =
        source_kernel::source_db::open_fjall_source_for_build(&source, &source_id).unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        native.source_store(),
        &source_id.0,
        "receipt-attempt",
        &"ab".repeat(32),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert!(generation::publication_receipt(&source, &source_id.0).is_err());
}
