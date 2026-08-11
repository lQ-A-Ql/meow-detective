//! Boot-relevant clean/dirty assessment of an XFS log snapshot.
//!
//! The logic follows the kernel's recovery chain
//! (`xlog_find_zeroed` → `xlog_find_head` → `xlog_check_unmount_rec`) in a
//! deliberately fail-safe form: the log is reported `Clean` only when the
//! record immediately before the head is a well-formed unmount record, and
//! every ambiguity — truncated snapshots, unusual cycle layouts, parse
//! failures — degrades to `Dirty`. A false `Dirty` only triggers a harmless
//! log zeroing through the emulation overlay; a false `Clean` would let a
//! guest bootloader that refuses dirty XFS logs (the RHEL/CentOS GRUB
//! builds) block the boot, so the bias is intentional.

use super::operation::XLOG_UNMOUNT_TRANS;
use super::wire::header_offset;
use super::{XfsLogSnapshot, XLOG_BASIC_BLOCK_SIZE, XLOG_HEADER_MAGIC_NUM};

/// Whether the log needs replay before the filesystem can be mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsLogState {
    /// The record at the head is an unmount record (or the log is zeroed):
    /// nothing to replay.
    Clean,
    /// Anything else: pending transactions, an unreadable layout, or an
    /// incomplete snapshot all map here.
    Dirty,
}

/// Cap on the backward scan for the last record header. The kernel bounds
/// the equivalent search by the maximum in-core log buffering
/// (`XLOG_MAX_ICLOGS * XLOG_MAX_RECORD_BSIZE` = 4096 basic blocks); doubling
/// that keeps the scan tolerant without making it unbounded.
const MAX_HEADER_SCAN_BLOCKS: u64 = 8192;

/// The `oh_flags` byte inside `xlog_op_header` (tid 4 + len 4 + clientid 1).
const OP_HEADER_FLAGS_OFFSET: usize = 9;

pub fn assess_log_state(snapshot: &XfsLogSnapshot) -> XfsLogState {
    assess(&snapshot.bytes, snapshot.geometry.record_version)
}

fn assess(bytes: &[u8], record_version: u32) -> XfsLogState {
    let bbcount = (bytes.len() / XLOG_BASIC_BLOCK_SIZE) as u64;
    if bbcount == 0 {
        return XfsLogState::Dirty;
    }
    let cycle0 = cycle_at(bytes, 0);
    if cycle0 == Some(0) {
        // Only a fully zeroed log is certainly empty (mkfs writes a dummy
        // unmount record, so a genuinely fresh log is not all zero). A zeroed
        // head with live data later in the snapshot is ambiguous — a torn
        // zeroing pass looks exactly like this — so fail safe to Dirty.
        if bytes.iter().all(|byte| *byte == 0) {
            return XfsLogState::Clean;
        }
        return XfsLogState::Dirty;
    }
    let cycle_last = cycle_at(bytes, bbcount - 1);
    let (Some(first_cycle), Some(last_cycle)) = (cycle0, cycle_last) else {
        return XfsLogState::Dirty;
    };
    let head = if last_cycle == 0 {
        // Partially zeroed log: the head is the first zeroed block.
        find_first_block_with_cycle(bytes, bbcount, 0)
    } else if first_cycle != last_cycle {
        // "x + 1 ... | x ... | x": the head is the first block stamped with
        // the older cycle.
        find_first_block_with_cycle(bytes, bbcount, last_cycle)
    } else {
        // Uniform cycle: the precise head needs the kernel's hole scan;
        // start from the wrap point and accept a false Dirty when the
        // unmount record does not line up exactly.
        Some(0)
    };
    let Some(head) = head else {
        return XfsLogState::Dirty;
    };
    match last_record_header_before(bytes, bbcount, head) {
        Some(record_block)
            if unmount_record_at(bytes, bbcount, record_version, record_block, head) =>
        {
            XfsLogState::Clean
        }
        _ => XfsLogState::Dirty,
    }
}

/// The cycle stamp of a basic block, per `xlog_get_cycle`: a record header
/// block carries the cycle in `h_cycle`, every other block in its first
/// word.
fn cycle_at(bytes: &[u8], block: u64) -> Option<u32> {
    let offset = usize::try_from(block)
        .ok()?
        .checked_mul(XLOG_BASIC_BLOCK_SIZE)?;
    let first = be_u32_checked(bytes, offset)?;
    if first == XLOG_HEADER_MAGIC_NUM {
        be_u32_checked(bytes, offset + header_offset::CYCLE)
    } else {
        Some(first)
    }
}

/// Binary search for the first block stamped with `target`, mirroring
/// `xlog_find_cycle_start`. Returns `None` when no block matches, which the
/// caller treats as a dirty log.
fn find_first_block_with_cycle(bytes: &[u8], bbcount: u64, target: u32) -> Option<u64> {
    let mut first = 0u64;
    let mut end = bbcount - 1;
    let mut mid = (first + end) >> 1;
    while mid != first && mid != end {
        match cycle_at(bytes, mid) {
            Some(cycle) if cycle == target => end = mid,
            Some(_) => first = mid,
            None => return None,
        }
        mid = (first + end) >> 1;
    }
    if cycle_at(bytes, end) == Some(target) {
        Some(end)
    } else if cycle_at(bytes, first) == Some(target) {
        Some(first)
    } else {
        None
    }
}

/// Walk backwards from `head` (wrapping once around the physical log) for
/// the most recent record header, per `xlog_rseek_logrec_hdr`.
fn last_record_header_before(bytes: &[u8], bbcount: u64, head: u64) -> Option<u64> {
    let mut scanned = 0u64;
    let mut block = head;
    while scanned < MAX_HEADER_SCAN_BLOCKS.min(bbcount) {
        block = if block == 0 { bbcount - 1 } else { block - 1 };
        let offset = usize::try_from(block)
            .ok()?
            .checked_mul(XLOG_BASIC_BLOCK_SIZE)?;
        if be_u32_checked(bytes, offset) == Some(XLOG_HEADER_MAGIC_NUM) {
            return Some(block);
        }
        scanned += 1;
    }
    None
}

/// The kernel's clean-unmount test (`xlog_check_unmount_rec`): the record
/// immediately before the head is a single-operation unmount record that
/// ends exactly at the head.
fn unmount_record_at(
    bytes: &[u8],
    bbcount: u64,
    record_version: u32,
    record_block: u64,
    head: u64,
) -> bool {
    let base = match usize::try_from(record_block)
        .ok()
        .and_then(|block| block.checked_mul(XLOG_BASIC_BLOCK_SIZE))
    {
        Some(base) => base,
        None => return false,
    };
    let version = be_u32_checked(bytes, base + header_offset::VERSION);
    let data_len = be_u32_checked(bytes, base + header_offset::DATA_LEN);
    let operation_count = be_u32_checked(bytes, base + header_offset::NUM_LOGOPS);
    let (Some(version), Some(data_len), Some(operation_count)) =
        (version, data_len, operation_count)
    else {
        return false;
    };
    if operation_count != 1 {
        return false;
    }
    let iclog_size = if record_version == 2 && (version & 2) != 0 {
        be_u32_checked(bytes, base + header_offset::ICLOG_SIZE).unwrap_or(0)
    } else {
        0
    };
    // xlog_logrec_hblks: an iclog larger than one cycle-data page spills
    // into extra header blocks; anything smaller is a single block.
    let header_blocks = if iclog_size > super::XLOG_HEADER_CYCLE_SIZE as u32 {
        u64::from(iclog_size).div_ceil(super::XLOG_HEADER_CYCLE_SIZE as u64)
    } else {
        1
    };
    let data_blocks = u64::from(data_len).div_ceil(XLOG_BASIC_BLOCK_SIZE as u64);
    let after_record = (record_block + header_blocks + data_blocks) % bbcount;
    if after_record != head {
        return false;
    }
    let op_offset = (record_block + header_blocks) % bbcount;
    let op_offset = match usize::try_from(op_offset)
        .ok()
        .and_then(|block| block.checked_mul(XLOG_BASIC_BLOCK_SIZE))
    {
        Some(offset) => offset,
        None => return false,
    };
    bytes
        .get(op_offset + OP_HEADER_FLAGS_OFFSET)
        .is_some_and(|flags| flags & XLOG_UNMOUNT_TRANS != 0)
}

fn be_u32_checked(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let chunk = bytes.get(offset..end)?;
    Some(u32::from_be_bytes(chunk.try_into().ok()?))
}

#[cfg(test)]
#[path = "../../tests/unit/log_state.rs"]
mod tests;
