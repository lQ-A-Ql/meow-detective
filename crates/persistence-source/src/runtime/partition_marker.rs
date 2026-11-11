//! Partition projection completion marker stored in the source Fjall metadata keyspace.

use persistence_fjall::{KeyPart, KeyspaceName, VersionedEnvelope};

use super::StorageRuntimeError;

pub fn partition_projection_marker(
    source_id: &str,
) -> Result<(KeyspaceName, Vec<u8>, VersionedEnvelope), StorageRuntimeError> {
    let key = persistence_fjall::encode_key(
        "partition_projection",
        &[KeyPart::Text(source_id.to_owned())],
    )?;
    let marker = VersionedEnvelope::new(
        "partition_projection_complete",
        1,
        format!("partition-projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    Ok((KeyspaceName::Meta, key, marker))
}
