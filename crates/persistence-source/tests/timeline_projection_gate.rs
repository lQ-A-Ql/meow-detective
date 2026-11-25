use persistence_fjall::{
    timeline_projection_marker_envelope, KeyspaceName, PersistMode, TimelineProjectionState,
    TimelineProjectionStatePayload,
};
use persistence_source::runtime::{
    ReadOnlyStorageRuntime, ReadWriteStorageRuntime, StorageRuntimeError,
};

#[test]
fn missing_marker_and_deferred_state_both_block_reads() {
    let root = tempfile::tempdir().unwrap();
    {
        let rw = ReadWriteStorageRuntime::open(root.path()).unwrap();
        rw.source().snapshot();
    }
    let ro = ReadOnlyStorageRuntime::open(root.path()).unwrap();
    let reader = ro.source();
    let snapshot = reader.snapshot();
    assert!(reader
        .timeline_projection_state(&snapshot, "source-01")
        .unwrap()
        .is_none());
    assert!(reader
        .require_timeline_projection_complete(&snapshot, "source-01")
        .is_err());

    {
        let rw = ReadWriteStorageRuntime::open(root.path()).unwrap();
        let payload = TimelineProjectionStatePayload::new(
            TimelineProjectionState::Deferred,
            "identity-1",
            0,
            1_200_000_000,
            900_000_000,
            Some("TIMELINE_WORKSPACE_LIMIT".to_owned()),
        );
        rw.source()
            .timeline_projection_mark_deferred("source-01", &payload, PersistMode::SyncAll)
            .unwrap();
    }
    let ro = ReadOnlyStorageRuntime::open(root.path()).unwrap();
    let reader = ro.source();
    let snapshot = reader.snapshot();
    let state = reader
        .timeline_projection_state(&snapshot, "source-01")
        .unwrap()
        .unwrap();
    assert_eq!(state.status, TimelineProjectionState::Deferred);
    assert_eq!(state.reason.as_deref(), Some("TIMELINE_WORKSPACE_LIMIT"));
    assert_eq!(state.required_bytes, 1_200_000_000);
    assert_eq!(state.available_bytes, 900_000_000);

    let err = reader
        .require_timeline_projection_complete(&snapshot, "source-01")
        .unwrap_err();
    match err {
        StorageRuntimeError::TimelineProjectionNotReady {
            source_id,
            status,
            reason,
        } => {
            assert_eq!(source_id, "source-01");
            assert_eq!(status, "deferred");
            assert_eq!(reason.as_deref(), Some("TIMELINE_WORKSPACE_LIMIT"));
        }
        other => panic!("expected TimelineProjectionNotReady, got {other:?}"),
    }
}

#[test]
fn building_marker_blocks_reads_with_typed_not_ready() {
    let root = tempfile::tempdir().unwrap();
    {
        let rw = ReadWriteStorageRuntime::open(root.path()).unwrap();
        let payload = TimelineProjectionStatePayload::new(
            TimelineProjectionState::Building,
            "identity-2",
            0,
            0,
            0,
            None,
        );
        let (key, envelope) = timeline_projection_marker_envelope("source-02", &payload).unwrap();
        rw.source()
            .write_envelope(KeyspaceName::Meta, key, &envelope, PersistMode::SyncAll)
            .unwrap();
    }
    let ro = ReadOnlyStorageRuntime::open(root.path()).unwrap();
    let reader = ro.source();
    let snapshot = reader.snapshot();
    let state = reader
        .timeline_projection_state(&snapshot, "source-02")
        .unwrap()
        .unwrap();
    assert_eq!(state.status, TimelineProjectionState::Building);
    let err = reader
        .require_timeline_projection_complete(&snapshot, "source-02")
        .unwrap_err();
    match err {
        StorageRuntimeError::TimelineProjectionNotReady { status, .. } => {
            assert_eq!(status, "building");
        }
        other => panic!("expected TimelineProjectionNotReady, got {other:?}"),
    }
}
