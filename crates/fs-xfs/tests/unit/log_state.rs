use super::*;
use crate::log::{XfsLogGeometry, XfsLogLocation};

const BB: usize = XLOG_BASIC_BLOCK_SIZE;
const MAGIC: u32 = XLOG_HEADER_MAGIC_NUM;

fn geometry(record_version: u32) -> XfsLogGeometry {
    XfsLogGeometry {
        location: XfsLogLocation::Internal { start_fsb: 8 },
        block_size: 4096,
        log_blocks: 16,
        sector_size: 512,
        record_version,
        metadata_crc: true,
        fs_uuid: [0x42; 16],
    }
}

fn snapshot(bytes: Vec<u8>, record_version: u32) -> XfsLogSnapshot {
    XfsLogSnapshot {
        geometry: geometry(record_version),
        complete: true,
        byte_limit: bytes.len(),
        source_offset: 0,
        bytes,
    }
}

/// Stamp a plain cycle word on a sector (not a record header).
fn stamp_cycle(bytes: &mut [u8], block: usize, cycle: u32) {
    bytes[block * BB..block * BB + 4].copy_from_slice(&cycle.to_be_bytes());
}

/// Write a log record header at `block`; when `unmount`, the single
/// operation carries the XLOG_UNMOUNT_TRANS flag.
fn write_record(
    bytes: &mut [u8],
    block: usize,
    cycle: u32,
    data_len: u32,
    operation_count: u32,
    unmount: bool,
) {
    let base = block * BB;
    bytes[base..base + 4].copy_from_slice(&MAGIC.to_be_bytes());
    bytes[base + 4..base + 8].copy_from_slice(&cycle.to_be_bytes());
    bytes[base + 8..base + 12].copy_from_slice(&2u32.to_be_bytes());
    bytes[base + 12..base + 16].copy_from_slice(&data_len.to_be_bytes());
    bytes[base + 40..base + 44].copy_from_slice(&operation_count.to_be_bytes());
    bytes[base + 320..base + 324].copy_from_slice(&512u32.to_be_bytes());
    if unmount {
        let op = (block + 1) * BB;
        bytes[op + 9] = XLOG_UNMOUNT_TRANS;
    }
}

#[test]
fn zeroed_log_is_clean() {
    let snap = snapshot(vec![0u8; 64 * BB], 2);
    assert_eq!(assess_log_state(&snap), XfsLogState::Clean);
}

#[test]
fn unmount_record_ending_at_the_wrap_is_clean() {
    // Uniform cycle everywhere; the unmount record header sits at block 62
    // with one data block, so the record ends exactly at block 0 (the
    // approximated head for a uniform-cycle log).
    let mut bytes = vec![0u8; 64 * BB];
    for block in 0..64 {
        stamp_cycle(&mut bytes, block, 5);
    }
    write_record(&mut bytes, 62, 5, 512, 1, true);
    let snap = snapshot(bytes, 2);
    assert_eq!(assess_log_state(&snap), XfsLogState::Clean);
}

#[test]
fn pending_transactions_are_dirty() {
    // Newer cycle at the front, older cycle behind: the head is block 10,
    // and the record before it is an ordinary multi-operation transaction.
    let mut bytes = vec![0u8; 64 * BB];
    for block in 0..10 {
        stamp_cycle(&mut bytes, block, 6);
    }
    for block in 10..64 {
        stamp_cycle(&mut bytes, block, 5);
    }
    write_record(&mut bytes, 8, 6, 1024, 3, false);
    let snap = snapshot(bytes, 2);
    assert_eq!(assess_log_state(&snap), XfsLogState::Dirty);
}

#[test]
fn clean_unmount_after_partial_zeroing_is_clean() {
    // Blocks 8.. are zeroed (head = 8); the unmount record at block 6 with
    // one data block ends exactly at block 8.
    let mut bytes = vec![0u8; 64 * BB];
    for block in 0..8 {
        stamp_cycle(&mut bytes, block, 3);
    }
    write_record(&mut bytes, 6, 3, 512, 1, true);
    let snap = snapshot(bytes, 2);
    assert_eq!(assess_log_state(&snap), XfsLogState::Clean);
}

#[test]
fn garbage_log_is_dirty() {
    let snap = snapshot(vec![0xFFu8; 64 * BB], 2);
    assert_eq!(assess_log_state(&snap), XfsLogState::Dirty);
}

#[test]
fn zeroed_head_with_live_record_later_is_dirty() {
    // Torn zeroing: the head block reads as cycle 0, but a live record sits
    // later in the snapshot. Only a fully zeroed log may be reported Clean.
    let mut bytes = vec![0u8; 64 * BB];
    write_record(&mut bytes, 20, 3, 512, 1, false);
    let snap = snapshot(bytes, 2);
    assert_eq!(assess_log_state(&snap), XfsLogState::Dirty);
}

#[test]
fn truncated_snapshot_is_dirty() {
    let snap = snapshot(vec![0u8; BB / 2], 2);
    assert_eq!(assess_log_state(&snap), XfsLogState::Dirty);
}
