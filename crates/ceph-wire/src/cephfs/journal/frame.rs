use sha2::{Digest, Sha256};

use crate::{
    codec::CephDecode,
    cursor::CephCursor,
    error::{CephWireError, Result},
};

use super::{
    event::decode_event, CephFsJournalFrame, CephFsJournalFramePrefix, CephFsJournalStreamFormat,
};

const RESILIENT_SENTINEL: u64 = 0x3141_5926_5358_9793;

pub fn decode_cephfs_journal_frame_prefix(
    input: &[u8],
    logical_offset: u64,
    format: CephFsJournalStreamFormat,
    maximum_payload_length: usize,
) -> Result<CephFsJournalFramePrefix> {
    let mut cursor = CephCursor::new(input);
    let prefix_length: usize = match format {
        CephFsJournalStreamFormat::Legacy => 4,
        CephFsJournalStreamFormat::Resilient => {
            if u64::decode(&mut cursor)? != RESILIENT_SENTINEL {
                return Err(invalid_frame(logical_offset, "sentinel mismatch"));
            }
            12
        }
    };
    let payload_length = u32::decode(&mut cursor)? as usize;
    if payload_length > maximum_payload_length {
        return Err(CephWireError::CephFsJournalEventTooLarge {
            offset: logical_offset,
            length: payload_length,
            limit: maximum_payload_length,
        });
    }
    let trailer_length = usize::from(matches!(format, CephFsJournalStreamFormat::Resilient)) * 8;
    let total_length = prefix_length
        .checked_add(payload_length)
        .and_then(|value| value.checked_add(trailer_length))
        .ok_or(CephWireError::CephFsJournalRangeOverflow)?;
    logical_offset
        .checked_add(total_length as u64)
        .ok_or(CephWireError::CephFsJournalRangeOverflow)?;
    Ok(CephFsJournalFramePrefix {
        logical_offset,
        prefix_length,
        payload_length,
        trailer_length,
        total_length,
    })
}

pub fn decode_cephfs_journal_frame(
    input: &[u8],
    logical_offset: u64,
    format: CephFsJournalStreamFormat,
    maximum_payload_length: usize,
) -> Result<CephFsJournalFrame> {
    let prefix =
        decode_cephfs_journal_frame_prefix(input, logical_offset, format, maximum_payload_length)?;
    if input.len() != prefix.total_length {
        return Err(invalid_frame(logical_offset, "frame length mismatch"));
    }
    let payload_end = prefix
        .prefix_length
        .checked_add(prefix.payload_length)
        .ok_or(CephWireError::CephFsJournalRangeOverflow)?;
    let payload = &input[prefix.prefix_length..payload_end];
    if matches!(format, CephFsJournalStreamFormat::Resilient) {
        let mut trailer = CephCursor::new(&input[payload_end..]);
        if u64::decode(&mut trailer)? != logical_offset {
            return Err(invalid_frame(logical_offset, "start pointer mismatch"));
        }
    }
    let logical_end = logical_offset
        .checked_add(prefix.total_length as u64)
        .ok_or(CephWireError::CephFsJournalRangeOverflow)?;
    Ok(CephFsJournalFrame {
        logical_offset,
        logical_end,
        payload_length: prefix.payload_length as u32,
        payload_sha256: format!("{:x}", Sha256::digest(payload)),
        event: decode_event(payload),
    })
}

fn invalid_frame(offset: u64, reason: &'static str) -> CephWireError {
    CephWireError::InvalidCephFsJournalFrame { offset, reason }
}
