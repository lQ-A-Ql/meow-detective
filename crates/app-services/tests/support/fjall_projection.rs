use app_services::storage_runtime::ReadWriteSourceStorageHandle;
use persistence_source::fjall::{
    encode_key, KeyPart, KeyspaceName, PersistMode, VersionedEnvelope,
};

pub fn mark_source_projection_complete(source: &ReadWriteSourceStorageHandle, source_id: &str) {
    let key = encode_key("projection", &[KeyPart::Text(source_id.to_owned())]).unwrap();
    let marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    source
        .write_envelope(KeyspaceName::Meta, key, &marker, PersistMode::SyncAll)
        .unwrap();
}
