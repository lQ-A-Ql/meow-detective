use crate::{
    bluestore_semantic::types::{
        BlueStoreDeferred, BlueStoreDeferredReason, BlueStoreOmapMode, BlueStoreSemanticLimits,
        BlueStoreSuperRecord,
    },
    error::{CephWireError, Result},
};

pub(crate) fn decode_super(
    logical_key: &[u8],
    value: &[u8],
    limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreSuperRecord> {
    if logical_key.len() > limits.max_string_bytes {
        return Err(CephWireError::LengthLimit {
            context: "BlueStore super field",
            length: logical_key.len(),
            limit: limits.max_string_bytes,
        });
    }
    let field = std::str::from_utf8(logical_key).map_err(|error| CephWireError::InvalidUtf8 {
        context: "BlueStore super field",
        message: error.to_string(),
    })?;
    if field.is_empty() || field.as_bytes().contains(&0) {
        return Err(CephWireError::InvalidBlueStoreSemanticKey {
            key_space: "super",
            reason: "field name must be non-empty UTF-8 without NUL",
        });
    }

    match field {
        "nid_max" => decode_u64(value, "BlueStore nid_max").map(BlueStoreSuperRecord::NidMax),
        "blobid_max" => {
            decode_u64(value, "BlueStore blobid_max").map(BlueStoreSuperRecord::BlobIdMax)
        }
        "min_alloc_size" => {
            decode_u64(value, "BlueStore min_alloc_size").map(BlueStoreSuperRecord::MinAllocSize)
        }
        "ondisk_format" => {
            decode_i32(value, "BlueStore ondisk_format").map(BlueStoreSuperRecord::OndiskFormat)
        }
        "min_compat_ondisk_format" => decode_i32(value, "BlueStore min_compat_ondisk_format")
            .map(BlueStoreSuperRecord::MinCompatOndiskFormat),
        "per_pool_omap" => decode_omap_mode(value).map(BlueStoreSuperRecord::PerPoolOmap),
        "freelist_type" => {
            decode_freelist_type(value, limits).map(BlueStoreSuperRecord::FreelistType)
        }
        _ => Ok(BlueStoreSuperRecord::Unknown {
            field: field.to_owned(),
            deferred: BlueStoreDeferred {
                reason: BlueStoreDeferredReason::UnknownSuperField,
                encoded_length: value.len(),
            },
        }),
    }
}

fn decode_u64(value: &[u8], context: &'static str) -> Result<u64> {
    let bytes: [u8; 8] =
        value
            .try_into()
            .map_err(|_| CephWireError::InvalidBlueStoreSemanticValue {
                context,
                reason: "expected exactly 8 little-endian bytes",
            })?;
    Ok(u64::from_le_bytes(bytes))
}

fn decode_i32(value: &[u8], context: &'static str) -> Result<i32> {
    let bytes: [u8; 4] =
        value
            .try_into()
            .map_err(|_| CephWireError::InvalidBlueStoreSemanticValue {
                context,
                reason: "expected exactly 4 little-endian bytes",
            })?;
    Ok(i32::from_le_bytes(bytes))
}

fn decode_omap_mode(value: &[u8]) -> Result<BlueStoreOmapMode> {
    match value {
        b"0" => Ok(BlueStoreOmapMode::Bulk),
        b"1" => Ok(BlueStoreOmapMode::PerPool),
        b"2" => Ok(BlueStoreOmapMode::PerPg),
        _ => Err(CephWireError::InvalidBlueStoreSemanticValue {
            context: "BlueStore per_pool_omap",
            reason: "expected canonical ASCII mode 0, 1, or 2",
        }),
    }
}

fn decode_freelist_type(value: &[u8], limits: BlueStoreSemanticLimits) -> Result<String> {
    if value.is_empty() {
        return Err(CephWireError::InvalidBlueStoreSemanticValue {
            context: "BlueStore freelist_type",
            reason: "freelist type must not be empty",
        });
    }
    if value.len() > limits.max_string_bytes {
        return Err(CephWireError::LengthLimit {
            context: "BlueStore freelist_type",
            length: value.len(),
            limit: limits.max_string_bytes,
        });
    }
    if value.contains(&0) {
        return Err(CephWireError::InvalidBlueStoreSemanticValue {
            context: "BlueStore freelist_type",
            reason: "freelist type must not contain NUL",
        });
    }
    std::str::from_utf8(value)
        .map(str::to_owned)
        .map_err(|error| CephWireError::InvalidUtf8 {
            context: "BlueStore freelist_type",
            message: error.to_string(),
        })
}
