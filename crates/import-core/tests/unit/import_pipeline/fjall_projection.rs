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

    let unchanged =
        mirror_analysis_outputs_from_connections(&case, root.path(), "case", &source, &source_id.0)
            .unwrap();
    assert_eq!(unchanged, expected);
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
