use super::checksum::xlog_checksum_matches;
use super::record::{
    LogRecordHeader, XfsLogChecksumStatus, XfsLogRecordProvenance, XfsLogSourceSpan,
};
use super::wire::{be_u32, header_offset};
use super::{
    XfsLogError, XfsLogIssue, XfsLogIssueKind, XfsLogSnapshot, XLOG_BASIC_BLOCK_SIZE,
    XLOG_HEADER_CYCLE_SIZE, XLOG_HEADER_MAGIC_NUM,
};

const CYCLE_DATA_WORDS_PER_HEADER: usize = XLOG_HEADER_CYCLE_SIZE / XLOG_BASIC_BLOCK_SIZE;

pub(super) fn record_provenance(
    snapshot: &XfsLogSnapshot,
    start: usize,
    length: usize,
) -> Option<XfsLogRecordProvenance> {
    let available = snapshot.bytes.len();
    let normalized_start = if snapshot.complete {
        start % available
    } else {
        start
    };
    let first_length = length.min(available.checked_sub(normalized_start)?);
    let first = source_span(snapshot, normalized_start, first_length)?;
    let remaining = length.checked_sub(first_length)?;
    let second = if remaining == 0 {
        None
    } else {
        if !snapshot.complete || remaining > available {
            return None;
        }
        Some(source_span(snapshot, 0, remaining)?)
    };
    Some(XfsLogRecordProvenance { first, second })
}

fn source_span(
    snapshot: &XfsLogSnapshot,
    snapshot_offset: usize,
    length: usize,
) -> Option<XfsLogSourceSpan> {
    if length == 0 {
        return None;
    }
    Some(XfsLogSourceSpan {
        snapshot_offset: u64::try_from(snapshot_offset).ok()?,
        source_offset: snapshot
            .source_offset
            .checked_add(u64::try_from(snapshot_offset).ok()?)?,
        length: u64::try_from(length).ok()?,
    })
}

pub(super) fn validate_record_context(
    snapshot: &XfsLogSnapshot,
    header: &LogRecordHeader,
    block: usize,
    total_blocks: usize,
) -> Result<(), XfsLogIssue> {
    let invalid =
        |message| XfsLogIssue::new(XfsLogIssueKind::InvalidRecord, Some(block as u64), message);
    if header.version != snapshot.geometry.record_version {
        return Err(invalid(format!(
            "record version {} does not match superblock log version {}",
            header.version, snapshot.geometry.record_version
        )));
    }
    if header.fs_uuid != snapshot.geometry.fs_uuid {
        return Err(invalid(
            "record UUID does not match the filesystem UUID".to_string(),
        ));
    }
    if header.lsn_cycle() != header.cycle {
        return Err(invalid(format!(
            "header cycle {} does not match LSN cycle {}",
            header.cycle,
            header.lsn_cycle()
        )));
    }
    if header.lsn_block() as usize != block {
        return Err(invalid(format!(
            "LSN block {} does not match physical log block {block}",
            header.lsn_block()
        )));
    }
    if header.previous_block != u32::MAX && header.previous_block as usize >= total_blocks {
        return Err(invalid(format!(
            "previous record block {} exceeds log geometry",
            header.previous_block
        )));
    }
    Ok(())
}

pub(super) fn validate_extended_header_cycles(
    header: &LogRecordHeader,
    header_bytes: &[u8],
    start_block: usize,
    total_blocks: usize,
) -> Result<(), XfsLogIssue> {
    for extension in 0..header.header_blocks().saturating_sub(1) {
        let offset = (extension + 1) * XLOG_BASIC_BLOCK_SIZE;
        let actual = be_u32(header_bytes, offset);
        let expected = physical_cycle(header.cycle, start_block + extension + 1, total_blocks);
        if actual != expected {
            return Err(XfsLogIssue::new(
                XfsLogIssueKind::CycleMismatch,
                Some(start_block as u64),
                format!(
                    "extended header {} has cycle {actual}, expected {expected}",
                    extension + 1
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn restore_cycle_words(
    header: &LogRecordHeader,
    header_bytes: &[u8],
    body: &mut [u8],
    start_block: usize,
    total_blocks: usize,
) -> Result<(), XfsLogIssue> {
    for body_block in 0..header.data_blocks() {
        let body_offset = body_block * XLOG_BASIC_BLOCK_SIZE;
        let actual_cycle = be_u32(body, body_offset);
        let physical_block = start_block + header.header_blocks() + body_block;
        let expected_cycle = physical_cycle(header.cycle, physical_block, total_blocks);
        if actual_cycle != expected_cycle {
            return Err(XfsLogIssue::new(
                XfsLogIssueKind::CycleMismatch,
                Some(start_block as u64),
                format!(
                    "record body block {body_block} has cycle {actual_cycle}, expected {expected_cycle}"
                ),
            ));
        }
        let saved_word = saved_cycle_word(header_bytes, body_block).ok_or_else(|| {
            XfsLogIssue::new(
                XfsLogIssueKind::InvalidRecord,
                Some(start_block as u64),
                format!("missing saved cycle word for body block {body_block}"),
            )
        })?;
        body[body_offset..body_offset + 4].copy_from_slice(&saved_word.to_be_bytes());
    }
    Ok(())
}

pub(super) fn validate_body_cycles(
    header: &LogRecordHeader,
    body: &[u8],
    start_block: usize,
    total_blocks: usize,
) -> Result<(), XfsLogIssue> {
    for body_block in 0..header.data_blocks() {
        let body_offset = body_block * XLOG_BASIC_BLOCK_SIZE;
        let actual_cycle = be_u32(body, body_offset);
        let physical_block = start_block + header.header_blocks() + body_block;
        let expected_cycle = physical_cycle(header.cycle, physical_block, total_blocks);
        if actual_cycle != expected_cycle {
            return Err(XfsLogIssue::new(
                XfsLogIssueKind::CycleMismatch,
                Some(start_block as u64),
                format!(
                    "record body block {body_block} has cycle {actual_cycle}, expected {expected_cycle}"
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_checksum(
    snapshot: &XfsLogSnapshot,
    header: &LogRecordHeader,
    header_bytes: &[u8],
    packed_body: &[u8],
    start_block: usize,
) -> Result<XfsLogChecksumStatus, XfsLogIssue> {
    if !snapshot.geometry.metadata_crc && header.crc == 0 {
        return Ok(XfsLogChecksumStatus::NotPresent);
    }
    if xlog_checksum_matches(header_bytes, packed_body, header.crc) {
        return Ok(XfsLogChecksumStatus::Verified);
    }
    Err(XfsLogIssue::new(
        XfsLogIssueKind::ChecksumMismatch,
        Some(start_block as u64),
        format!("log record CRC32C 0x{:08X} does not verify", header.crc),
    ))
}

fn saved_cycle_word(header_bytes: &[u8], body_block: usize) -> Option<u32> {
    let (header_index, word_index, base) = if body_block < CYCLE_DATA_WORDS_PER_HEADER {
        (0, body_block, header_offset::CYCLE_DATA)
    } else {
        (
            body_block / CYCLE_DATA_WORDS_PER_HEADER,
            body_block % CYCLE_DATA_WORDS_PER_HEADER,
            4,
        )
    };
    let offset = header_index
        .checked_mul(XLOG_BASIC_BLOCK_SIZE)?
        .checked_add(base)?
        .checked_add(word_index.checked_mul(4)?)?;
    header_bytes
        .get(offset..offset + 4)
        .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn physical_cycle(cycle: u32, absolute_block: usize, total_blocks: usize) -> u32 {
    let wraps = absolute_block / total_blocks;
    let mut value = cycle.wrapping_add(wraps as u32);
    if value == XLOG_HEADER_MAGIC_NUM {
        value = value.wrapping_add(1);
    }
    value
}

pub(super) fn read_log_span(
    snapshot: &XfsLogSnapshot,
    start: usize,
    length: usize,
) -> Option<Vec<u8>> {
    let available = snapshot.bytes.len();
    let normalized_start = if snapshot.complete {
        start % available
    } else {
        start
    };
    if normalized_start.checked_add(length)? <= available {
        return Some(snapshot.bytes[normalized_start..normalized_start + length].to_vec());
    }
    if !snapshot.complete || length > available {
        return None;
    }
    let first_len = available - normalized_start;
    let second_len = length - first_len;
    let mut result = Vec::with_capacity(length);
    result.extend_from_slice(&snapshot.bytes[normalized_start..]);
    result.extend_from_slice(&snapshot.bytes[..second_len]);
    Some(result)
}

pub(super) fn truncated_issue(block: usize, section: &str) -> XfsLogIssue {
    XfsLogIssue::new(
        XfsLogIssueKind::TruncatedRecord,
        Some(block as u64),
        format!("bounded snapshot ends inside {section}"),
    )
}

pub(super) fn validate_snapshot(snapshot: &XfsLogSnapshot) -> Result<(), XfsLogError> {
    if snapshot.bytes.is_empty() {
        return Err(XfsLogError::InvalidGeometry(
            "snapshot is empty".to_string(),
        ));
    }
    if !snapshot.bytes.len().is_multiple_of(XLOG_BASIC_BLOCK_SIZE) {
        return Err(XfsLogError::InvalidGeometry(format!(
            "snapshot length {} is not 512-byte aligned",
            snapshot.bytes.len()
        )));
    }
    let declared = snapshot.geometry.log_bytes()?;
    if snapshot.bytes.len() as u64 > declared {
        return Err(XfsLogError::InvalidGeometry(format!(
            "snapshot length {} exceeds declared log length {declared}",
            snapshot.bytes.len()
        )));
    }
    if snapshot.complete && snapshot.bytes.len() as u64 != declared {
        return Err(XfsLogError::InvalidGeometry(format!(
            "complete snapshot has {} bytes but geometry declares {declared}",
            snapshot.bytes.len()
        )));
    }
    Ok(())
}
