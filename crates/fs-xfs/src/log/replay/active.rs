//! Select the live tail-to-head record chain from the circular log.

use std::collections::HashMap;

use super::super::{XfsLogError, XfsLogRecord};

pub(super) fn select_active_records(
    records: Vec<XfsLogRecord>,
    total_blocks: u32,
) -> Result<Vec<XfsLogRecord>, XfsLogError> {
    let Some(latest) = records.iter().max_by_key(|record| record.header.lsn) else {
        return unsafe_replay("dirty XFS log contains no valid replay record");
    };
    let latest_lsn = latest.header.lsn;
    let tail_lsn = latest.header.tail_lsn;
    if tail_lsn == 0 || tail_lsn == u64::MAX || tail_lsn > latest_lsn {
        return unsafe_replay("latest XFS log record declares an invalid tail LSN");
    }
    let tail_block = tail_lsn as u32;
    if tail_block >= total_blocks {
        return unsafe_replay("XFS log tail lies outside the circular log");
    }

    let mut by_block = records
        .into_iter()
        .map(|record| (record.log_block, record))
        .collect::<HashMap<_, _>>();
    let record_count = by_block.len();
    let mut active: Vec<XfsLogRecord> = Vec::new();
    let mut block = tail_block;
    let mut previous_lsn = None;
    loop {
        let record = by_block.remove(&block).ok_or_else(|| {
            XfsLogError::UnsafeReplay(format!(
                "XFS active log chain has no valid record at block {block}"
            ))
        })?;
        if active.is_empty() && record.header.lsn != tail_lsn {
            return unsafe_replay("XFS log tail LSN does not identify its physical record");
        }
        if record.header.lsn > latest_lsn
            || previous_lsn.is_some_and(|previous| record.header.lsn <= previous)
        {
            return unsafe_replay("XFS active log record LSNs are not strictly increasing");
        }
        if let Some(previous) = active.last() {
            if record.header.previous_block != previous.log_block {
                return unsafe_replay("XFS active log previous-record chain is inconsistent");
            }
        }
        let is_latest = record.header.lsn == latest_lsn;
        let span = record
            .header
            .header_blocks()
            .checked_add(record.header.data_blocks())
            .ok_or_else(|| XfsLogError::InvalidData("XFS log record span overflows".into()))?;
        previous_lsn = Some(record.header.lsn);
        active.push(record);
        if is_latest {
            break;
        }
        let span = u32::try_from(span)
            .map_err(|_| XfsLogError::InvalidData("XFS log record span is too large".into()))?;
        block = block
            .checked_add(span)
            .map(|next| next % total_blocks)
            .ok_or_else(|| XfsLogError::InvalidData("XFS log block address overflows".into()))?;
        if active.len() > record_count {
            return unsafe_replay("XFS active log chain loops before reaching the head");
        }
    }
    Ok(active)
}

fn unsafe_replay<T>(message: impl Into<String>) -> Result<T, XfsLogError> {
    Err(XfsLogError::UnsafeReplay(message.into()))
}
