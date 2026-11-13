use super::*;
use domain::FileEntryId;
use persistence_source::repositories::artifact_repo::ArtifactRepo;

struct Source {
    connection: rusqlite::Connection,
    _root: tempfile::TempDir,
}

fn source() -> Source {
    let workspace =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.codex-tmp/evtx-stream-tests");
    std::fs::create_dir_all(&workspace).unwrap();
    let root = tempfile::tempdir_in(workspace).unwrap();
    let path = root.path().join("case/sources/evtx-source/source.db");
    let connection =
        persistence_source::control::opening::open_source_database(&path, "evtx-source").unwrap();
    connection.execute("INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES ('evtx-source', 'case', 'EVTX', 'logical_directory', 'evidence', '2026-01-01T00:00:00Z')", []).unwrap();
    let store = persistence_source::detail::open_store_for_write(&connection)
        .unwrap()
        .unwrap();
    persistence_source::fjall::replace_source_generation(
        &store,
        "evtx-source",
        "fixture",
        &"a".repeat(64),
    )
    .unwrap();
    let key = persistence_source::fjall::encode_key(
        "projection",
        &[persistence_source::fjall::KeyPart::Text(
            "evtx-source".into(),
        )],
    )
    .unwrap();
    let marker = persistence_source::fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        "projection:evtx-source",
        "evtx-source",
        1,
        b"evtx-source".to_vec(),
    );
    store
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .unwrap();
    drop(store);
    Source {
        connection,
        _root: root,
    }
}

fn candidate() -> EvidenceCandidate {
    let size = std::fs::metadata(testing::fixtures::tiny_system_evtx())
        .unwrap()
        .len();
    EvidenceCandidate {
        file_id: FileEntryId("evtx-input".into()),
        data_source_id: "evtx-source".into(),
        partition_index: Some(0),
        path: "Windows/System32/winevt/Logs/System.evtx".into(),
        size,
        encrypted: false,
        content_identity: "public-evtx-fixture".into(),
        companions: Vec::new(),
        evidence_kind: "event_log".into(),
        parser: "evtx.structured".into(),
        category: "EventLogs".into(),
        modified_at: None,
    }
}

#[test]
fn file_backed_evtx_stream_keeps_outputs_staged_until_publication() {
    let fixture = source();
    let cancel = AtomicBool::new(false);
    let generation = crate::analysis_service::extraction::native_output::begin_streaming(
        &fixture.connection,
        "evtx-source",
        &[],
        &cancel,
        |_| {},
        |_| {},
    )
    .unwrap();
    let reader = std::fs::File::open(testing::fixtures::tiny_system_evtx()).unwrap();
    let parsed = persist_reader(&generation, &candidate(), Box::new(reader), &cancel).unwrap();
    assert!(parsed.artifact_count > 0);
    assert!(!parsed.output_digest.is_empty());
    assert_eq!(ArtifactRepo::new(&fixture.connection).count().unwrap(), 0);
    drop(generation);
    assert_eq!(ArtifactRepo::new(&fixture.connection).count().unwrap(), 0);
}

#[test]
fn file_backed_evtx_stream_honors_cancellation_before_reading() {
    let fixture = source();
    let cancel = AtomicBool::new(false);
    let generation = crate::analysis_service::extraction::native_output::begin_streaming(
        &fixture.connection,
        "evtx-source",
        &[],
        &cancel,
        |_| {},
        |_| {},
    )
    .unwrap();
    cancel.store(true, Ordering::Release);
    let reader = std::fs::File::open(testing::fixtures::tiny_system_evtx()).unwrap();
    assert!(matches!(
        persist_reader(&generation, &candidate(), Box::new(reader), &cancel),
        Err(AnalysisServiceError::Cancelled)
    ));
    assert_eq!(ArtifactRepo::new(&fixture.connection).count().unwrap(), 0);
}
