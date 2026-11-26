use persistence_source::fjall::{
    timeline_projection_marker_envelope, KeyspaceName, PersistMode, SourceStore,
    TimelineProjectionState, TimelineProjectionStatePayload,
};

pub fn mark_complete(store: &SourceStore, source_id: &str, event_count: u64) {
    let payload = TimelineProjectionStatePayload::new(
        TimelineProjectionState::Complete,
        format!("fixture:{source_id}"),
        event_count,
        0,
        0,
        None,
    );
    let (key, envelope) = timeline_projection_marker_envelope(source_id, &payload).unwrap();
    store
        .write_envelope(KeyspaceName::Meta, key, &envelope, PersistMode::SyncAll)
        .unwrap();
}
