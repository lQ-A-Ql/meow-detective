use crate::{RocksDbWireError, ROCKSDB_MAX_SEQUENCE_NUMBER};

use super::SstEntryKind;

pub(super) struct DecodedInternalKey<'a> {
    pub(super) user_key: &'a [u8],
    pub(super) sequence: u64,
    pub(super) value_type: u8,
}

pub(super) fn decode_internal_key(
    key: &[u8],
) -> std::result::Result<DecodedInternalKey<'_>, RocksDbWireError> {
    if key.len() < 8 {
        return Err(RocksDbWireError::InternalKeyTooShort {
            context: "SST entry stream",
            length: key.len(),
        });
    }
    let trailer = u64::from_le_bytes(key[key.len() - 8..].try_into().map_err(|_| {
        RocksDbWireError::InvalidField {
            context: "SST entry stream internal key",
            reason: "fixed64 width",
        }
    })?);
    let sequence = trailer >> 8;
    if sequence > ROCKSDB_MAX_SEQUENCE_NUMBER {
        return Err(RocksDbWireError::InvalidSequenceNumber { sequence });
    }
    Ok(DecodedInternalKey {
        user_key: &key[..key.len() - 8],
        sequence,
        value_type: trailer as u8,
    })
}

pub(super) fn decode_data_kind(
    value_type: u8,
) -> std::result::Result<SstEntryKind, RocksDbWireError> {
    match value_type {
        0x00 => Ok(SstEntryKind::Deletion),
        0x01 => Ok(SstEntryKind::Value),
        0x02 => Ok(SstEntryKind::Merge),
        0x07 => Ok(SstEntryKind::SingleDeletion),
        0x11 => Ok(SstEntryKind::BlobIndex),
        0x14 => Ok(SstEntryKind::DeletionWithTimestamp),
        0x16 => Ok(SstEntryKind::WideColumnEntity),
        _ => Err(RocksDbWireError::UnsupportedSstEntryType { value_type }),
    }
}

pub(super) fn validate_internal_order(
    previous: &[u8],
    current: &[u8],
) -> std::result::Result<(), RocksDbWireError> {
    if previous.is_empty() {
        return Ok(());
    }
    let previous = decode_internal_key(previous)?;
    let current = decode_internal_key(current)?;
    let ordered = match previous.user_key.cmp(current.user_key) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => {
            let previous_trailer = (previous.sequence << 8) | u64::from(previous.value_type);
            let current_trailer = (current.sequence << 8) | u64::from(current.value_type);
            previous_trailer > current_trailer
        }
    };
    if !ordered {
        return Err(RocksDbWireError::InvalidSstProperty {
            context: "SST entry stream",
            reason: "internal keys are not strictly ordered",
        });
    }
    Ok(())
}
