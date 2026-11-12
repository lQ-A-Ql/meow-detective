use ceph_wire::{codec::decode_string, CephCursor, CephDecode};

use super::error::{invalid_field, BlueStoreOmapError};

const MAX_VALUE_BYTES: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 4 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DecodedOmapEntry {
    DirectoryName {
        image_name: String,
        image_id: String,
    },
    DirectoryId {
        image_id: String,
        image_name: String,
    },
    Size(u64),
    Order(u8),
    Features(u64),
    OperationFeatures(u64),
    ParentKeyPresent,
    ObjectPrefix(String),
    StripeUnit(u64),
    StripeCount(u64),
    DataPoolId(i64),
}

pub(super) fn decode_entry(
    user_key: &[u8],
    value: &[u8],
) -> Result<Option<DecodedOmapEntry>, BlueStoreOmapError> {
    if user_key.starts_with(b"name_") {
        let image_name = decode_key_text(&user_key[5..], "rbd directory image name")?;
        let image_id = decode_value_text(value, "rbd directory name_ value")?;
        validate_identifier(&image_id, "rbd directory image id")?;
        return Ok(Some(DecodedOmapEntry::DirectoryName {
            image_name,
            image_id,
        }));
    }
    if user_key.starts_with(b"id_") {
        let image_id = decode_key_text(&user_key[3..], "rbd directory image id")?;
        validate_identifier(&image_id, "rbd directory image id")?;
        let image_name = decode_value_text(value, "rbd directory id_ value")?;
        return Ok(Some(DecodedOmapEntry::DirectoryId {
            image_id,
            image_name,
        }));
    }

    let entry = match user_key {
        b"size" => DecodedOmapEntry::Size(decode_primitive(value, "rbd header size")?),
        b"order" => DecodedOmapEntry::Order(decode_primitive(value, "rbd header order")?),
        b"features" => DecodedOmapEntry::Features(decode_primitive(value, "rbd header features")?),
        b"op_features" => DecodedOmapEntry::OperationFeatures(decode_primitive(
            value,
            "rbd header operation features",
        )?),
        b"parent" => {
            validate_bounded_value(value)?;
            DecodedOmapEntry::ParentKeyPresent
        }
        b"object_prefix" => {
            DecodedOmapEntry::ObjectPrefix(decode_value_text(value, "rbd header object_prefix")?)
        }
        b"stripe_unit" => {
            DecodedOmapEntry::StripeUnit(decode_primitive(value, "rbd header stripe_unit")?)
        }
        b"stripe_count" => {
            DecodedOmapEntry::StripeCount(decode_primitive(value, "rbd header stripe_count")?)
        }
        b"data_pool_id" => {
            DecodedOmapEntry::DataPoolId(decode_primitive(value, "rbd header data_pool_id")?)
        }
        _ => return Ok(None),
    };
    validate_entry(&entry)?;
    Ok(Some(entry))
}

pub(super) fn is_rbd_candidate_key(user_key: &[u8]) -> bool {
    user_key.starts_with(b"name_")
        || user_key.starts_with(b"id_")
        || matches!(
            user_key,
            b"size"
                | b"order"
                | b"features"
                | b"op_features"
                | b"parent"
                | b"object_prefix"
                | b"stripe_unit"
                | b"stripe_count"
                | b"data_pool_id"
        )
}

fn validate_bounded_value(value: &[u8]) -> Result<(), BlueStoreOmapError> {
    if value.len() > MAX_VALUE_BYTES {
        Err(BlueStoreOmapError::LimitExceeded {
            resource: "OMAP value",
            limit: MAX_VALUE_BYTES,
        })
    } else {
        Ok(())
    }
}

fn decode_key_text(bytes: &[u8], field: &'static str) -> Result<String, BlueStoreOmapError> {
    if bytes.is_empty() {
        return Err(invalid_field(field, "key suffix is empty"));
    }
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(BlueStoreOmapError::LimitExceeded {
            resource: "OMAP text",
            limit: MAX_TEXT_BYTES,
        });
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid_field(field, "key suffix is not valid UTF-8"))?;
    if text.contains('\0') {
        return Err(invalid_field(field, "key suffix contains NUL"));
    }
    Ok(text.to_string())
}

fn decode_value_text(value: &[u8], field: &'static str) -> Result<String, BlueStoreOmapError> {
    if value.len() > MAX_VALUE_BYTES {
        return Err(BlueStoreOmapError::LimitExceeded {
            resource: "OMAP value",
            limit: MAX_VALUE_BYTES,
        });
    }
    let mut cursor = CephCursor::new(value);
    let text = decode_string(&mut cursor, MAX_TEXT_BYTES, field)
        .map_err(|source| BlueStoreOmapError::ValueDecode { field, source })?;
    ensure_empty(&cursor, field)?;
    if text.is_empty() || text.contains('\0') {
        return Err(invalid_field(
            field,
            "decoded text is empty or contains NUL",
        ));
    }
    Ok(text)
}

fn decode_primitive<T: CephDecode>(
    value: &[u8],
    field: &'static str,
) -> Result<T, BlueStoreOmapError> {
    if value.len() > MAX_VALUE_BYTES {
        return Err(BlueStoreOmapError::LimitExceeded {
            resource: "OMAP value",
            limit: MAX_VALUE_BYTES,
        });
    }
    let mut cursor = CephCursor::new(value);
    let decoded = T::decode(&mut cursor)
        .map_err(|source| BlueStoreOmapError::ValueDecode { field, source })?;
    ensure_empty(&cursor, field)?;
    Ok(decoded)
}

fn ensure_empty(cursor: &CephCursor<'_>, field: &'static str) -> Result<(), BlueStoreOmapError> {
    if cursor.is_empty() {
        Ok(())
    } else {
        Err(BlueStoreOmapError::TrailingValue {
            field,
            remaining: cursor.remaining(),
        })
    }
}

fn validate_identifier(value: &str, field: &'static str) -> Result<(), BlueStoreOmapError> {
    if value.is_empty() || value.contains('\0') {
        return Err(invalid_field(field, "identifier is empty or contains NUL"));
    }
    Ok(())
}

fn validate_entry(entry: &DecodedOmapEntry) -> Result<(), BlueStoreOmapError> {
    match entry {
        DecodedOmapEntry::ObjectPrefix(value) if value.is_empty() => {
            Err(invalid_field("rbd header object_prefix", "value is empty"))
        }
        DecodedOmapEntry::StripeUnit(0) => {
            Err(invalid_field("rbd header stripe_unit", "value is zero"))
        }
        DecodedOmapEntry::StripeCount(0) => {
            Err(invalid_field("rbd header stripe_count", "value is zero"))
        }
        _ => Ok(()),
    }
}
