use crate::{
    codec::{decode_string, CephDecode},
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub const RBD_MIN_ORDER: u8 = 12;
pub const RBD_MAX_ORDER: u8 = 25;
pub const RBD_MAX_IMAGE_NAME_LENGTH: usize = 96;
pub const RBD_MAX_IMAGE_ID_LENGTH: usize = 14;
pub const RBD_MAX_OBJECT_PREFIX_LENGTH: usize = 43;

/// Normalized RBD metadata values needed to construct a head-image layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbdImageMetadata {
    pub name: String,
    pub id: String,
    pub object_prefix: String,
    pub image_size: u64,
    pub order: u8,
    pub features: u64,
    pub stripe_unit: u64,
    pub stripe_count: u64,
    pub data_pool_id: i64,
}

pub fn decode_rbd_order(bytes: &[u8]) -> Result<u8> {
    let order = decode_exact(bytes, "order")?;
    if !(RBD_MIN_ORDER..=RBD_MAX_ORDER).contains(&order) {
        return Err(CephWireError::InvalidRbdMetadata {
            field: "order",
            reason: "must be in the Ceph-supported range [12, 25]",
        });
    }
    Ok(order)
}

pub fn decode_rbd_size(bytes: &[u8]) -> Result<u64> {
    decode_exact(bytes, "size")
}

pub fn decode_rbd_features(bytes: &[u8]) -> Result<u64> {
    decode_exact(bytes, "features")
}

pub fn decode_rbd_stripe_unit(bytes: &[u8]) -> Result<u64> {
    decode_exact(bytes, "stripe_unit")
}

pub fn decode_rbd_stripe_count(bytes: &[u8]) -> Result<u64> {
    decode_exact(bytes, "stripe_count")
}

pub fn decode_rbd_data_pool_id(bytes: &[u8]) -> Result<i64> {
    decode_exact(bytes, "data_pool_id")
}

pub fn decode_rbd_string(bytes: &[u8], field: &'static str) -> Result<String> {
    let mut cursor = CephCursor::new(bytes);
    let value = decode_string(&mut cursor, 16 * 1024 * 1024, field)?;
    ensure_empty(&cursor, field)?;
    Ok(value)
}

pub fn decode_rbd_object_prefix(bytes: &[u8]) -> Result<String> {
    let value = decode_bounded_string(bytes, "object_prefix", RBD_MAX_OBJECT_PREFIX_LENGTH)?;
    validate_nonempty_text(&value, "object_prefix")?;
    Ok(value)
}

pub fn decode_rbd_name(bytes: &[u8]) -> Result<String> {
    let value = decode_bounded_string(bytes, "name", RBD_MAX_IMAGE_NAME_LENGTH)?;
    validate_nonempty_text(&value, "name")?;
    Ok(value)
}

pub fn decode_rbd_id(bytes: &[u8]) -> Result<String> {
    let value = decode_bounded_string(bytes, "id", RBD_MAX_IMAGE_ID_LENGTH)?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(CephWireError::InvalidRbdMetadata {
            field: "id",
            reason: "must be a nonempty ASCII alphanumeric identifier",
        });
    }
    Ok(value)
}

fn decode_bounded_string(bytes: &[u8], field: &'static str, max_length: usize) -> Result<String> {
    let mut cursor = CephCursor::new(bytes);
    let value = decode_string(&mut cursor, max_length, field)?;
    ensure_empty(&cursor, field)?;
    Ok(value)
}

fn decode_exact<T: CephDecode>(bytes: &[u8], field: &'static str) -> Result<T> {
    let mut cursor = CephCursor::new(bytes);
    let value = T::decode(&mut cursor)?;
    ensure_empty(&cursor, field)?;
    Ok(value)
}

fn ensure_empty(cursor: &CephCursor<'_>, field: &'static str) -> Result<()> {
    if cursor.is_empty() {
        Ok(())
    } else {
        Err(CephWireError::RbdTrailingBytes {
            field,
            remaining: cursor.remaining(),
        })
    }
}

fn validate_nonempty_text(value: &str, field: &'static str) -> Result<()> {
    if value.is_empty() || value.contains('\0') {
        return Err(CephWireError::InvalidRbdMetadata {
            field,
            reason: "must be nonempty and contain no NUL bytes",
        });
    }
    Ok(())
}
