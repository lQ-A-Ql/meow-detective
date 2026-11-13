pub(crate) fn seal_source(
    connection: &rusqlite::Connection,
    source_id: &domain::DataSourceId,
) -> persistence_sqlite::DbResult<()> {
    use persistence_source::fjall::{KeyPart, KeyspaceName, PersistMode, VersionedEnvelope};
    let source = app_services::source_db::open_fjall_source_for_build(connection, source_id)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let key =
        persistence_source::fjall::encode_key("projection", &[KeyPart::Text(source_id.0.clone())])
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", source_id.0),
        &source_id.0,
        1,
        source_id.0.as_bytes().to_vec(),
    );
    let receipt = source
        .write_envelope(KeyspaceName::Meta, key, &marker, PersistMode::SyncAll)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    persistence_source::fjall::seal_source_generation_with_control(
        source.source_store(),
        &source_id.0,
        "desktop-fixture",
        &hex::encode(receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .map(|_| ())
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
}
