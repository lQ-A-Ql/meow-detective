pub fn seal(connection: &rusqlite::Connection, source_id: &domain::DataSourceId) {
    let handle =
        app_services::source_db::open_fjall_source_for_build(connection, source_id).unwrap();
    let (keyspace, key, marker) =
        persistence_source::runtime::partition_marker::partition_projection_marker(&source_id.0)
            .unwrap();
    let receipt = handle
        .write_envelope(
            keyspace,
            key,
            &marker,
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        handle.source_store(),
        &source_id.0,
        "test-source",
        &hex::encode(receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
}
