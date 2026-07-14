use crate::{
    bluestore_semantic::{
        collection::decode_collection,
        object_value::decode_object,
        shared_blob::decode_shared_blob,
        super_value::decode_super,
        types::{BlueStoreDecodedRecord, BlueStoreKeySpace, BlueStoreSemanticLimits},
    },
    error::{CephWireError, Result},
};

/// Decodes a latest-state BlueStore default-column-family value.
///
/// `logical_key` must have the RocksDB column-family prefix (`S`, `C`, `O`, or
/// `X`) removed by the caller.
pub fn decode_bluestore_latest_value(
    key_space: BlueStoreKeySpace,
    logical_key: &[u8],
    value: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreDecodedRecord> {
    validate_input_lengths(logical_key, value, limits)?;
    match key_space {
        BlueStoreKeySpace::Super => {
            decode_super(logical_key, value, limits).map(BlueStoreDecodedRecord::Super)
        }
        BlueStoreKeySpace::Collection => {
            decode_collection(logical_key, value, limits).map(BlueStoreDecodedRecord::Collection)
        }
        BlueStoreKeySpace::Object => decode_object(logical_key, value, limits)
            .map(Box::new)
            .map(BlueStoreDecodedRecord::Object),
        BlueStoreKeySpace::SharedBlob => {
            decode_shared_blob(logical_key, value, limits).map(BlueStoreDecodedRecord::SharedBlob)
        }
    }
}

fn validate_input_lengths(
    logical_key: &[u8],
    value: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<()> {
    if logical_key.len() > limits.max_logical_key_bytes {
        return Err(CephWireError::LengthLimit {
            context: "BlueStore logical key",
            length: logical_key.len(),
            limit: limits.max_logical_key_bytes,
        });
    }
    if value.len() > limits.max_value_bytes {
        return Err(CephWireError::LengthLimit {
            context: "BlueStore value",
            length: value.len(),
            limit: limits.max_value_bytes,
        });
    }
    Ok(())
}
