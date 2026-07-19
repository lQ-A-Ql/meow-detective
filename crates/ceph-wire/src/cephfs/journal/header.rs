use crate::{
    codec::{decode_string, CephDecode, CephStructEnvelope},
    cursor::CephCursor,
    error::{CephWireError, Result},
};

use super::{
    CephFsJournalHeader, CephFsJournalLayout, CephFsJournalStreamFormat, CEPHFS_JOURNAL_MAGIC,
};

const MAX_MAGIC_LENGTH: usize = 256;
const CEPH_MIN_STRIPE_UNIT: u32 = 64 * 1024;

pub fn decode_cephfs_journal_header(input: &[u8]) -> Result<CephFsJournalHeader> {
    let mut cursor = CephCursor::new(input);
    let version = u8::decode(&mut cursor)?;
    let (mut payload, stream_format) = if version == 1 {
        (cursor, CephFsJournalStreamFormat::Legacy)
    } else {
        let mut envelope_cursor = CephCursor::new(input);
        let (envelope, payload) = CephStructEnvelope::decode_payload(&mut envelope_cursor, 2)?;
        if envelope.version < 2 || envelope.compat_version > 2 {
            return Err(CephWireError::UnsupportedCephFsJournalVersion {
                structure: "header",
                encoded_version: envelope.version,
                compat_version: envelope.compat_version,
            });
        }
        if !envelope_cursor.is_empty() {
            return Err(invalid("trailing bytes"));
        }
        (payload, CephFsJournalStreamFormat::Legacy)
    };
    let header = CephFsJournalHeader {
        magic: decode_string(&mut payload, MAX_MAGIC_LENGTH, "CephFS journal magic")?,
        trimmed_pos: u64::decode(&mut payload)?,
        expire_pos: u64::decode(&mut payload)?,
        unused_pos: u64::decode(&mut payload)?,
        write_pos: u64::decode(&mut payload)?,
        layout: decode_legacy_layout(&mut payload)?,
        stream_format: if version == 1 {
            stream_format
        } else {
            CephFsJournalStreamFormat::try_from(u8::decode(&mut payload)?)?
        },
    };
    if version == 1 && !payload.is_empty() {
        return Err(invalid("trailing bytes"));
    }
    validate_header(&header)?;
    Ok(header)
}

fn decode_legacy_layout(cursor: &mut CephCursor<'_>) -> Result<CephFsJournalLayout> {
    let stripe_unit = u32::decode(cursor)?;
    let stripe_count = u32::decode(cursor)?;
    let object_size = u32::decode(cursor)?;
    let _cas_hash = u32::decode(cursor)?;
    let _object_stripe_unit = u32::decode(cursor)?;
    let _unused = u32::decode(cursor)?;
    let pool_id = i64::from(i32::from_le_bytes(u32::decode(cursor)?.to_le_bytes()));
    Ok(CephFsJournalLayout {
        stripe_unit,
        stripe_count,
        object_size,
        pool_id,
    })
}

fn validate_header(header: &CephFsJournalHeader) -> Result<()> {
    if header.magic != CEPHFS_JOURNAL_MAGIC {
        return Err(invalid("unexpected on-disk magic"));
    }
    let layout = header.layout;
    if layout.stripe_unit == 0
        || !layout.stripe_unit.is_multiple_of(CEPH_MIN_STRIPE_UNIT)
        || layout.object_size == 0
        || !layout.object_size.is_multiple_of(CEPH_MIN_STRIPE_UNIT)
        || layout.object_size < layout.stripe_unit
        || !layout.object_size.is_multiple_of(layout.stripe_unit)
        || layout.stripe_count == 0
        || layout.pool_id < 0
    {
        return Err(invalid("invalid legacy file layout"));
    }
    let period = layout.period()?;
    if header.trimmed_pos < period
        || header.trimmed_pos > header.expire_pos
        || header.expire_pos > header.write_pos
    {
        return Err(invalid("invalid position ordering"));
    }
    Ok(())
}

fn invalid(reason: &'static str) -> CephWireError {
    CephWireError::InvalidCephFsJournal {
        context: "header",
        reason,
    }
}
