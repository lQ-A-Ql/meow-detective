use super::*;

pub(super) fn stream_delete(writer: &mut WriteStream<'_>, record: &TimelineRecord) -> Result<()> {
    writer.delete(
        KeyspaceName::TimelineRecords,
        record_key(&record.data_source_id, &record.id)?,
    );
    for index in index_keys(record)? {
        writer.delete(KeyspaceName::TimelineIndexes, index);
    }
    Ok(())
}

pub(super) fn stream_action(writer: &mut WriteStream<'_>, action: WriteAction) {
    match action {
        WriteAction::Put {
            keyspace,
            key,
            value,
        } => writer.put(keyspace, key, value),
        WriteAction::Delete { keyspace, key } => writer.delete(keyspace, key),
    }
}

pub(super) fn record_action(record: &TimelineRecord) -> Result<WriteAction> {
    let payload = serde_json::to_vec(record)
        .map_err(|error| crate::FjallStorageError::Serialization(error.to_string()))?;
    let envelope = VersionedEnvelope::new(
        RECORD_KIND,
        SCHEMA_VERSION,
        &record.id,
        &record.data_source_id,
        1,
        payload,
    );
    Ok(WriteAction::Put {
        keyspace: KeyspaceName::TimelineRecords,
        key: record_key(&record.data_source_id, &record.id)?,
        value: envelope.encode()?,
    })
}
