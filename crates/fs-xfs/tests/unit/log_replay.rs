use super::super::checksum::crc32c;
use crate::log::{
    XfsLogError, XFS_LI_BUF, XFS_LI_DQUOT, XFS_LI_EFD, XFS_LI_EFI, XFS_LI_ICREATE, XFS_LI_INODE,
    XFS_TRANSACTION_CLIENT, XLOG_BASIC_BLOCK_SIZE, XLOG_COMMIT_TRANS, XLOG_OP_HEADER_SIZE,
    XLOG_START_TRANS,
};
use crate::reader::{sb_off, XFS_SUPER_MAGIC};
use crate::XfsReader;
use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

const FS_BLOCK_SIZE: usize = 4096;
const FS_BLOCKS: usize = 256;
const LOG_START_FSB: usize = 8;
const LOG_BLOCKS: usize = 160;
const FS_UUID: [u8; 16] = [
    0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF,
];
const RECORD_CYCLE: u32 = 7;
const RECORD_LSN: u64 = (RECORD_CYCLE as u64) << 32;

struct MemoryReader {
    bytes: Vec<u8>,
    position: u64,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        Self {
            bytes,
            position: 0,
            info: ReaderInfo {
                path: PathBuf::from("xfs-log-replay-fixture"),
                size,
                kind: "memory".to_string(),
            },
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let start = usize::try_from(self.position).unwrap_or(usize::MAX);
        let end = start.saturating_add(output.len()).min(self.bytes.len());
        let count = end.saturating_sub(start);
        output[..count].copy_from_slice(&self.bytes[start..end]);
        self.position = self.position.saturating_add(count as u64);
        Ok(count)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => self.bytes.len() as i128 + i128::from(value),
            SeekFrom::Current(value) => self.position as i128 + i128::from(value),
        };
        if next < 0 || next > u64::MAX as i128 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

#[test]
fn replays_a_committed_buffer_transaction_into_a_metadata_patch() {
    let operations = buffer_transaction(1, 200 * 8, 0xAB);
    let plan = plan_with_record(&operations);

    assert_eq!(plan.replayed_transactions, 1);
    assert_eq!(plan.skipped_items, 0);
    assert_eq!(plan.patches.len(), 2, "buffer patch plus the log rewrite");
    let patch = &plan.patches[0];
    assert_eq!(patch.offset, (200 * FS_BLOCK_SIZE) as u64);
    assert_eq!(patch.bytes.len(), FS_BLOCK_SIZE);
    assert_eq!(&patch.bytes[128..256], &[0xAB; 128]);
    let rewrite = &plan.patches[1];
    assert_eq!(rewrite.offset, (LOG_START_FSB * FS_BLOCK_SIZE) as u64);
    // The fully stamped rewrite terminates at the physical end, so the
    // kernel infers a wrapped head at block 0 in the next cycle.
    let record_block = rewrite.bytes.len() / 512 - 2;
    assert_eq!(
        &rewrite.bytes[record_block * 512..record_block * 512 + 4],
        &0xFEED_BABEu32.to_be_bytes()
    );
    assert_eq!(
        u32::from_be_bytes(rewrite.bytes[0..4].try_into().unwrap()),
        RECORD_CYCLE + 1
    );
    assert_eq!(
        u32::from_be_bytes(
            rewrite.bytes[record_block * 512 + 4..record_block * 512 + 8]
                .try_into()
                .unwrap()
        ),
        RECORD_CYCLE + 1
    );
    assert_eq!(
        u32::from_be_bytes(
            rewrite.bytes[(record_block - 1) * 512..(record_block - 1) * 512 + 4]
                .try_into()
                .unwrap()
        ),
        RECORD_CYCLE + 1
    );
    assert_eq!(
        u64::from_be_bytes(
            rewrite.bytes[record_block * 512 + 24..record_block * 512 + 32]
                .try_into()
                .unwrap()
        ),
        (u64::from(RECORD_CYCLE + 1) << 32) | record_block as u64
    );
}

#[test]
fn clean_log_cycle_is_newer_than_the_v5_superblock_lsn() {
    let operations = buffer_transaction(1, 200 * 8, 0xAB);
    let mut image = filesystem_with_record(0, RECORD_CYCLE, 32 * 1024, &operations);
    let superblock_lsn = (19u64 << 32) | 5735;
    image[sb_off::LSN..sb_off::LSN + 8].copy_from_slice(&superblock_lsn.to_be_bytes());
    let reader = XfsReader::open(Box::new(MemoryReader::new(image)), 0).unwrap();
    let plan = reader
        .plan_log_repair()
        .unwrap()
        .expect("the fixture log assesses dirty");
    let rewrite = plan.patches.last().unwrap();
    let record_block = rewrite.bytes.len() / 512 - 2;

    assert_eq!(
        u32::from_be_bytes(rewrite.bytes[0..4].try_into().unwrap()),
        20
    );
    assert_eq!(
        u32::from_be_bytes(
            rewrite.bytes[record_block * 512 + 4..record_block * 512 + 8]
                .try_into()
                .unwrap()
        ),
        20
    );
    assert_eq!(
        u64::from_be_bytes(
            rewrite.bytes[record_block * 512 + 16..record_block * 512 + 24]
                .try_into()
                .unwrap()
        ),
        (20u64 << 32) | record_block as u64
    );
}

#[test]
fn replays_a_committed_inode_transaction_with_stamped_lsn_and_valid_crc() {
    let operations = inode_transaction(1, 66, 201 * 8);
    let mut image = filesystem_with_record(0, RECORD_CYCLE, 32 * 1024, &operations);
    let inode = current_inode(66, RECORD_LSN - 1);
    let offset = 201 * FS_BLOCK_SIZE;
    image[offset..offset + inode.len()].copy_from_slice(&inode);
    let reader = XfsReader::open(Box::new(MemoryReader::new(image)), 0).unwrap();
    let plan = reader
        .plan_log_repair()
        .unwrap()
        .expect("the fixture log assesses dirty");

    assert_eq!(plan.replayed_transactions, 1);
    assert_eq!(plan.skipped_items, 0);
    assert_eq!(plan.patches.len(), 2);
    let patch = &plan.patches[0];
    assert_eq!(patch.offset, (201 * FS_BLOCK_SIZE) as u64);
    assert_eq!(patch.bytes.len(), 256);
    assert_eq!(&patch.bytes[0..2], &0x494Eu16.to_be_bytes());
    assert_eq!(patch.bytes[4], 3);
    assert_eq!(
        u64::from_be_bytes(patch.bytes[112..120].try_into().unwrap()),
        RECORD_LSN,
        "di_lsn must be the recovering record's h_lsn"
    );
    assert_eq!(
        u64::from_be_bytes(patch.bytes[152..160].try_into().unwrap()),
        66
    );
    let stored_crc = u32::from_le_bytes(patch.bytes[100..104].try_into().unwrap());
    assert_eq!(stored_crc, metadata_crc(&patch.bytes));
}

#[test]
fn drops_an_uncommitted_transaction_without_patches() {
    // No COMMIT op: the transaction never becomes eligible for replay.
    let operations = vec![
        operation(1, XLOG_START_TRANS, &[]),
        operation(1, 0, &transaction_header()),
        operation(1, 0, &buf_descriptor(200 * 8)),
        operation(1, 0, &[0xCD; 128]),
    ];
    let plan = plan_with_record(&operations);
    assert_eq!(plan.replayed_transactions, 0);
    assert_eq!(plan.patches.len(), 1, "only the log-area rewrite");
    assert_eq!(
        plan.patches[0].offset,
        (LOG_START_FSB * FS_BLOCK_SIZE) as u64
    );
}

#[test]
fn treats_a_cancelled_buffer_as_safely_handled() {
    let blkno = 200 * 8;
    let mut operations = buffer_transaction(1, blkno, 0xCD);
    operations.extend([
        operation(2, XLOG_START_TRANS, &[]),
        operation(2, 0, &transaction_header()),
        operation(2, 0, &cancel_buf_descriptor(blkno)),
        operation(2, XLOG_COMMIT_TRANS, &[]),
    ]);

    let plan = plan_with_record(&operations);

    assert_eq!(plan.replayed_transactions, 2);
    assert_eq!(plan.skipped_items, 0);
    assert_eq!(plan.patches.len(), 1, "only the log-area rewrite");
}

#[test]
fn rejects_an_unknown_item_type() {
    let unknown = vec![0x00, 0x12, 0x01, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];
    let operations = vec![
        operation(1, XLOG_START_TRANS, &[]),
        operation(1, 0, &transaction_header()),
        operation(1, 0, &unknown),
        operation(1, XLOG_COMMIT_TRANS, &[]),
    ];
    assert!(matches!(
        plan_error(&operations),
        XfsLogError::UnsafeReplay(message) if message.contains("unsupported log item")
    ));
}

#[test]
fn ignores_valid_but_stale_records_before_the_active_tail() {
    let unknown = vec![0x00, 0x12, 0x01, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];
    let stale = one_item_transaction(1, &unknown, &[]);
    let active = buffer_transaction(2, 200 * 8, 0xAB);
    let mut image = build_filesystem_image();
    let log =
        &mut image[LOG_START_FSB * FS_BLOCK_SIZE..(LOG_START_FSB + LOG_BLOCKS) * FS_BLOCK_SIZE];
    let stale_record = build_record(0, RECORD_CYCLE - 1, 32 * 1024, &stale);
    let active_record = build_record(32, RECORD_CYCLE, 32 * 1024, &active);
    write_circular(log, 0, &stale_record);
    write_circular(log, 32 * XLOG_BASIC_BLOCK_SIZE, &active_record);

    let reader = XfsReader::open(Box::new(MemoryReader::new(image)), 0).unwrap();
    let plan = reader.plan_log_repair().unwrap().unwrap();

    assert_eq!(plan.replayed_transactions, 1);
    assert_eq!(plan.patches[0].offset, (200 * FS_BLOCK_SIZE) as u64);
}

#[test]
fn rejects_an_incomplete_snapshot_before_replay() {
    let image = filesystem_with_record(
        0,
        RECORD_CYCLE,
        32 * 1024,
        &buffer_transaction(1, 200 * 8, 0xAB),
    );
    let reader = XfsReader::open(Box::new(MemoryReader::new(image)), 0).unwrap();
    let snapshot = reader
        .read_internal_log_snapshot(XLOG_BASIC_BLOCK_SIZE * 4)
        .unwrap();
    assert!(!snapshot.complete);

    let error = crate::log::replay::replay_log_snapshot(
        &snapshot,
        &crate::log::replay::ReplayGeometry {
            block_size: FS_BLOCK_SIZE as u64,
            dblocks: FS_BLOCKS as u64,
            ag_blocks: FS_BLOCKS as u64,
            ag_count: 1,
            inode_size: 256,
            inopblog: 4,
            agblklog: 8,
            metadata_uuid: FS_UUID,
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        XfsLogError::UnsafeReplay(message) if message.contains("complete internal log snapshot")
    ));
}

#[test]
fn selects_a_contiguous_multi_record_active_chain() {
    let tail = synthetic_log_record(2, 7, (7u64 << 32) | 2, u32::MAX);
    let head = synthetic_log_record(4, 7, (7u64 << 32) | 2, 2);

    let active = crate::log::replay::active::select_active_records(vec![head, tail], 16).unwrap();

    assert_eq!(
        active
            .iter()
            .map(|record| record.log_block)
            .collect::<Vec<_>>(),
        vec![2, 4]
    );
}

#[test]
fn selects_an_active_chain_that_wraps_at_the_physical_log_end() {
    let tail = synthetic_log_record(14, 7, (7u64 << 32) | 14, u32::MAX);
    let head = synthetic_log_record(0, 8, (7u64 << 32) | 14, 14);

    let active = crate::log::replay::active::select_active_records(vec![head, tail], 16).unwrap();

    assert_eq!(
        active
            .iter()
            .map(|record| record.log_block)
            .collect::<Vec<_>>(),
        vec![14, 0]
    );
}

#[test]
fn rejects_a_missing_tail_or_broken_previous_record_chain() {
    let missing_tail = synthetic_log_record(4, 8, (7u64 << 32) | 2, 2);
    assert!(matches!(
        crate::log::replay::active::select_active_records(vec![missing_tail], 16),
        Err(XfsLogError::UnsafeReplay(message)) if message.contains("no valid record")
    ));

    let tail = synthetic_log_record(2, 7, (7u64 << 32) | 2, u32::MAX);
    let broken = synthetic_log_record(4, 7, (7u64 << 32) | 2, 3);
    assert!(matches!(
        crate::log::replay::active::select_active_records(vec![broken, tail], 16),
        Err(XfsLogError::UnsafeReplay(message)) if message.contains("previous-record chain")
    ));
}

#[test]
fn rejects_a_dirty_log_without_any_valid_record() {
    assert!(matches!(
        crate::log::replay::active::select_active_records(Vec::new(), 16),
        Err(XfsLogError::UnsafeReplay(message)) if message.contains("no valid replay record")
    ));
}

#[test]
fn rejects_an_out_of_geometry_buffer_item() {
    let operations = buffer_transaction(1, 1_000_000, 0xAB);
    assert!(matches!(
        plan_error(&operations),
        XfsLogError::UnsafeReplay(message) if message.contains("outside filesystem geometry")
    ));
}

#[test]
fn accepts_a_completed_efi_efd_pair() {
    let operations = deferred_transactions(Some(0x1122_3344), 0x1122_3344);
    let plan = plan_with_record(&operations);

    assert_eq!(plan.replayed_transactions, 2);
    assert_eq!(plan.skipped_items, 0);
    assert_eq!(plan.patches.len(), 1, "only the log-area rewrite");
}

#[test]
fn rejects_an_efi_without_a_matching_efd() {
    let operations = deferred_transactions(Some(0x1122_3344), 0x5566_7788);
    assert!(matches!(
        plan_error(&operations),
        XfsLogError::UnsafeReplay(message) if message.contains("EFI intent")
    ));
}

#[test]
fn accepts_an_efd_without_a_matching_efi() {
    let operations = deferred_transactions(None, 0x5566_7788);
    let plan = plan_with_record(&operations);
    assert_eq!(plan.replayed_transactions, 1);
    assert_eq!(plan.skipped_items, 0);
}

#[test]
fn rejects_dquot_and_inode_btree_root_replay() {
    let dquot = generic_item(XFS_LI_DQUOT, 1, 32);
    let dquot_ops = one_item_transaction(1, &dquot, &[]);
    assert!(matches!(
        plan_error(&dquot_ops),
        XfsLogError::UnsafeReplay(message) if message.contains("DQUOT")
    ));

    let mut descriptor = inode_descriptor(66, 201 * 8);
    descriptor[4..8].copy_from_slice(&0x009u32.to_le_bytes());
    let inode_ops = one_item_transaction(2, &descriptor, &[logged_core_v3(66)]);
    assert!(matches!(
        plan_error(&inode_ops),
        XfsLogError::UnsafeReplay(message) if message.contains("btree-root")
    ));
}

#[test]
fn rejects_malformed_or_incomplete_items() {
    let malformed = generic_item(XFS_LI_BUF, 1, 8);
    assert!(matches!(
        plan_error(&one_item_transaction(1, &malformed, &[])),
        XfsLogError::InvalidData(message) if message.contains("BUF descriptor")
    ));

    let incomplete = generic_item(XFS_LI_BUF, 2, 24);
    assert!(matches!(
        plan_error(&one_item_transaction(2, &incomplete, &[])),
        XfsLogError::InvalidData(message) if message.contains("incomplete log item")
    ));
}

#[test]
fn replays_icreate_as_a_regenerated_inode_cluster() {
    let icreate = icreate_region(0, 4, 16, 256, 1, 7);
    let operations = vec![
        operation(1, XLOG_START_TRANS, &[]),
        operation(1, 0, &transaction_header()),
        operation(1, 0, &icreate),
        operation(1, XLOG_COMMIT_TRANS, &[]),
    ];
    let plan = plan_with_record(&operations);

    assert_eq!(plan.replayed_transactions, 1);
    assert_eq!(plan.skipped_items, 0);
    assert_eq!(plan.patches.len(), 2);
    let patch = &plan.patches[0];
    assert_eq!(patch.offset, (4 * FS_BLOCK_SIZE) as u64);
    assert_eq!(patch.bytes.len(), FS_BLOCK_SIZE);
    let first = &patch.bytes[..256];
    assert_eq!(&first[0..2], &0x494Eu16.to_be_bytes());
    assert_eq!(first[4], 3);
    assert_eq!(u32::from_be_bytes(first[92..96].try_into().unwrap()), 7);
    assert_eq!(
        u32::from_be_bytes(first[96..100].try_into().unwrap()),
        u32::MAX
    );
    assert_eq!(u64::from_be_bytes(first[112..120].try_into().unwrap()), 0);
    assert_eq!(u64::from_be_bytes(first[152..160].try_into().unwrap()), 64);
    let second = &patch.bytes[256..512];
    assert_eq!(u64::from_be_bytes(second[152..160].try_into().unwrap()), 65);
    let stored_crc = u32::from_le_bytes(first[100..104].try_into().unwrap());
    assert_eq!(stored_crc, metadata_crc(first));
}

#[test]
fn skips_a_buffer_when_the_current_metadata_lsn_is_newer() {
    let current = agi_block((RECORD_CYCLE as u64 + 1) << 32);
    let replay = crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Buffer(
            crate::log::replay::XfsBufferReplay {
                offset: 0,
                length: current.len(),
                lsn: RECORD_LSN,
                buffer_type: 7,
                inode_unlinked_only: false,
                inode_size: 256,
                ag_inode_count: 4096,
                writes: vec![crate::log::replay::XfsReplayPatch {
                    offset: 64,
                    bytes: vec![0xAA; 128],
                }],
            },
        )],
        max_record_lsn: RECORD_LSN,
        replayed_transactions: 1,
        skipped_items: 0,
    };
    let finalized =
        crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, _| Ok(current.clone()))
            .unwrap();
    assert!(finalized.patches.is_empty());
    assert_eq!(finalized.skipped_items, 0);
}

#[test]
fn seals_a_replayed_agi_buffer_after_partial_writes() {
    let current = agi_block(RECORD_LSN - 1);
    let replay_lsn = RECORD_LSN;
    let replay = crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Buffer(
            crate::log::replay::XfsBufferReplay {
                offset: 0,
                length: current.len(),
                lsn: replay_lsn,
                buffer_type: 7,
                inode_unlinked_only: false,
                inode_size: 256,
                ag_inode_count: 4096,
                writes: vec![crate::log::replay::XfsReplayPatch {
                    offset: 64,
                    bytes: vec![0xAA; 128],
                }],
            },
        )],
        max_record_lsn: replay_lsn,
        replayed_transactions: 1,
        skipped_items: 0,
    };
    let finalized =
        crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, _| Ok(current.clone()))
            .unwrap();
    assert_eq!(finalized.patches.len(), 1);
    let bytes = &finalized.patches[0].bytes;
    assert_eq!(
        u64::from_be_bytes(bytes[320..328].try_into().unwrap()),
        replay_lsn
    );
    assert_eq!(
        u32::from_le_bytes(bytes[312..316].try_into().unwrap()),
        agi_crc(bytes)
    );
    assert_eq!(&bytes[64..192], &[0xAA; 128]);
}

#[test]
fn seals_an_agf_using_the_rmap_era_uuid_offset() {
    let mut current = vec![0u8; 512];
    current[0..4].copy_from_slice(&0x5841_4746u32.to_be_bytes());
    current[64..80].copy_from_slice(&FS_UUID);
    current[208..216].copy_from_slice(&(RECORD_LSN - 1).to_be_bytes());
    let replay = crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Buffer(
            crate::log::replay::XfsBufferReplay {
                offset: 0,
                length: current.len(),
                lsn: RECORD_LSN,
                buffer_type: 5,
                inode_unlinked_only: false,
                inode_size: 256,
                ag_inode_count: 4096,
                writes: vec![crate::log::replay::XfsReplayPatch {
                    offset: 128,
                    bytes: vec![0xA5; 128],
                }],
            },
        )],
        max_record_lsn: RECORD_LSN,
        replayed_transactions: 1,
        skipped_items: 0,
    };
    let finalized =
        crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, _| Ok(current.clone()))
            .unwrap();

    assert_eq!(finalized.patches.len(), 1);
    assert_eq!(
        u64::from_be_bytes(finalized.patches[0].bytes[208..216].try_into().unwrap()),
        RECORD_LSN
    );
}

#[test]
fn rejects_a_buffer_whose_magic_does_not_match_its_verifier() {
    let replay = crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Buffer(
            crate::log::replay::XfsBufferReplay {
                offset: 0,
                length: FS_BLOCK_SIZE,
                lsn: RECORD_LSN,
                buffer_type: 7,
                inode_unlinked_only: false,
                inode_size: 256,
                ag_inode_count: 4096,
                writes: vec![crate::log::replay::XfsReplayPatch {
                    offset: 64,
                    bytes: vec![0xAA; 128],
                }],
            },
        )],
        max_record_lsn: RECORD_LSN,
        replayed_transactions: 1,
        skipped_items: 0,
    };
    let error = crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, length| {
        Ok(vec![0u8; length])
    })
    .unwrap_err();
    assert!(matches!(
        error,
        XfsLogError::UnsafeReplay(message) if message.contains("verifier rejected")
    ));
}

#[test]
fn dino_buffer_replays_only_unlinked_fields_and_reseals_inodes() {
    let mut current = current_inode(64, RECORD_LSN - 1);
    let second = current_inode(65, RECORD_LSN - 1);
    current.extend_from_slice(&second);
    let logged_next = 77u32.to_be_bytes();
    let replay = crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Buffer(
            crate::log::replay::XfsBufferReplay {
                offset: 0,
                length: current.len(),
                lsn: RECORD_LSN,
                buffer_type: 8,
                inode_unlinked_only: true,
                inode_size: 256,
                ag_inode_count: 4096,
                writes: vec![crate::log::replay::XfsReplayPatch {
                    offset: 0,
                    bytes: {
                        let mut region = vec![0xA5; 128];
                        region[96..100].copy_from_slice(&logged_next);
                        region
                    },
                }],
            },
        )],
        max_record_lsn: RECORD_LSN,
        replayed_transactions: 1,
        skipped_items: 0,
    };
    let finalized =
        crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, _| Ok(current.clone()))
            .unwrap();
    let bytes = &finalized.patches[0].bytes;

    assert_eq!(&bytes[96..100], &logged_next);
    assert_eq!(&bytes[104..128], &current[104..128]);
    assert_eq!(&bytes[256..], &current[256..]);
    assert_eq!(
        u32::from_le_bytes(bytes[100..104].try_into().unwrap()),
        metadata_crc(&bytes[..256])
    );
}

#[test]
fn dino_allocation_buffer_replays_complete_logged_inode_images() {
    let mut current = current_inode(64, RECORD_LSN - 1);
    current.extend_from_slice(&current_inode(65, RECORD_LSN - 1));
    let mut logged = current_inode(64, 0);
    logged[92..96].copy_from_slice(&99u32.to_be_bytes());
    let crc = metadata_crc(&logged);
    logged[100..104].copy_from_slice(&crc.to_le_bytes());
    logged.extend_from_slice(&current_inode(65, 0));
    let replay = crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Buffer(
            crate::log::replay::XfsBufferReplay {
                offset: 0,
                length: current.len(),
                lsn: RECORD_LSN,
                buffer_type: 8,
                inode_unlinked_only: false,
                inode_size: 256,
                ag_inode_count: 4096,
                writes: vec![crate::log::replay::XfsReplayPatch {
                    offset: 0,
                    bytes: logged.clone(),
                }],
            },
        )],
        max_record_lsn: RECORD_LSN,
        replayed_transactions: 1,
        skipped_items: 0,
    };
    let finalized =
        crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, _| Ok(current.clone()))
            .unwrap();

    assert_eq!(finalized.patches[0].bytes, logged);
    assert_eq!(
        u32::from_be_bytes(finalized.patches[0].bytes[92..96].try_into().unwrap()),
        99
    );
}

#[test]
fn inode_unlinked_buffers_are_ordered_after_inode_items() {
    let mut buffer_descriptor = buf_descriptor(200 * 8);
    buffer_descriptor[4..6].copy_from_slice(&((8u16 << 11) | 1u16).to_le_bytes());
    let transaction = crate::log::replay::assemble::CommittedTransaction {
        lsn: RECORD_LSN,
        format: crate::log::XfsLogFormat::LinuxLittleEndian,
        items: vec![
            crate::log::replay::assemble::AssembledItem {
                regions: vec![buffer_descriptor, vec![0xA5; 128]],
            },
            crate::log::replay::assemble::AssembledItem {
                regions: vec![inode_descriptor(3200, 200 * 8), logged_core_v3(3200)],
            },
        ],
    };
    let outcome = crate::log::replay::items::apply_transactions(
        &crate::log::replay::ReplayGeometry {
            block_size: FS_BLOCK_SIZE as u64,
            dblocks: FS_BLOCKS as u64,
            ag_blocks: FS_BLOCKS as u64,
            ag_count: 1,
            inode_size: 256,
            inopblog: 4,
            agblklog: 8,
            metadata_uuid: FS_UUID,
        },
        &[transaction],
    )
    .unwrap();

    assert!(matches!(
        outcome.actions.as_slice(),
        [
            crate::log::replay::XfsReplayAction::Inode(_),
            crate::log::replay::XfsReplayAction::Buffer(buffer)
        ] if buffer.inode_unlinked_only
    ));
}

#[test]
fn rejects_inode_unlinked_flag_on_a_non_dino_buffer() {
    let replay = crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Buffer(
            crate::log::replay::XfsBufferReplay {
                offset: 0,
                length: FS_BLOCK_SIZE,
                lsn: RECORD_LSN,
                buffer_type: 7,
                inode_unlinked_only: true,
                inode_size: 256,
                ag_inode_count: 4096,
                writes: Vec::new(),
            },
        )],
        max_record_lsn: RECORD_LSN,
        replayed_transactions: 1,
        skipped_items: 0,
    };
    let error = crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, length| {
        Ok(vec![0u8; length])
    })
    .unwrap_err();

    assert!(matches!(
        error,
        XfsLogError::UnsafeReplay(message) if message.contains("not typed as a DINO")
    ));
}

#[test]
fn compares_buffer_lsns_by_cycle_then_block() {
    assert!(crate::log::replay::buffer::lsn_is_at_or_after(
        (20u64 << 32) | 1,
        (19u64 << 32) | u32::MAX as u64
    ));
    assert!(!crate::log::replay::buffer::lsn_is_at_or_after(
        (19u64 << 32) | 4,
        (19u64 << 32) | 5
    ));
}

#[test]
fn skips_an_inode_when_the_current_dinode_lsn_is_newer() {
    let current = current_inode(66, RECORD_LSN + 1);
    let replay = inode_replay_action(66, RECORD_LSN, vec![]);
    let finalized =
        crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, _| Ok(current.clone()))
            .unwrap();
    assert!(finalized.patches.is_empty());
    assert_eq!(finalized.skipped_items, 0);
}

#[test]
fn reports_the_fields_of_an_inode_identity_mismatch() {
    let current = vec![0u8; 256];
    let replay = crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Inode(
            crate::log::replay::XfsInodeReplay {
                offset: 4096,
                length: current.len(),
                lsn: RECORD_LSN,
                inode_number: 66,
                writes: Vec::new(),
            },
        )],
        max_record_lsn: RECORD_LSN,
        replayed_transactions: 1,
        skipped_items: 0,
    };

    let error =
        crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, _| Ok(current.clone()))
            .unwrap_err();
    assert!(matches!(
        error,
        XfsLogError::UnsafeReplay(message)
            if message.contains("volume offset 4096")
                && message.contains("expected inode 66")
                && message.contains("magic 0x0000")
                && message.contains("version 0")
                && message.contains("inode 0")
    ));
}

#[test]
fn seals_the_complete_inode_after_replaying_a_fork() {
    let current = current_inode(66, RECORD_LSN - 1);
    let mut core = current[..176].to_vec();
    core[112..120].copy_from_slice(&RECORD_LSN.to_be_bytes());
    let replay = inode_replay_action(
        66,
        RECORD_LSN,
        vec![
            crate::log::replay::XfsReplayPatch {
                offset: 0,
                bytes: core,
            },
            crate::log::replay::XfsReplayPatch {
                offset: 176,
                bytes: vec![0x5A; 32],
            },
        ],
    );
    let finalized =
        crate::log::replay::finalize_replay(replay, true, &FS_UUID, |_, _| Ok(current.clone()))
            .unwrap();
    let bytes = &finalized.patches[0].bytes;
    assert_eq!(bytes.len(), 256);
    assert_eq!(&bytes[176..208], &[0x5A; 32]);
    assert_eq!(
        u64::from_be_bytes(bytes[112..120].try_into().unwrap()),
        RECORD_LSN
    );
    assert_eq!(
        u32::from_le_bytes(bytes[100..104].try_into().unwrap()),
        metadata_crc(bytes)
    );
}

fn plan_with_record(operations: &[Vec<u8>]) -> crate::XfsLogClearPlan {
    let image = filesystem_with_record(0, RECORD_CYCLE, 32 * 1024, operations);
    let reader = XfsReader::open(Box::new(MemoryReader::new(image)), 0).unwrap();
    reader
        .plan_log_repair()
        .unwrap()
        .expect("the fixture log assesses dirty")
}

fn plan_error(operations: &[Vec<u8>]) -> XfsLogError {
    let image = filesystem_with_record(0, RECORD_CYCLE, 32 * 1024, operations);
    let reader = XfsReader::open(Box::new(MemoryReader::new(image)), 0).unwrap();
    reader.plan_log_repair().unwrap_err()
}

/// The v5 metadata CRC32C: plain `~0` seed, four zero bytes in place of the
/// CRC field at offset 100, one's complement.
fn metadata_crc(object: &[u8]) -> u32 {
    let mut crc = crc32c(u32::MAX, &object[..100]);
    crc = crc32c(crc, &[0u8; 4]);
    crc = crc32c(crc, &object[104..]);
    !crc
}

fn agi_block(lsn: u64) -> Vec<u8> {
    let mut block = vec![0u8; FS_BLOCK_SIZE];
    block[0..4].copy_from_slice(&0x5841_4749u32.to_be_bytes());
    block[4..8].copy_from_slice(&1u32.to_be_bytes());
    block[8..12].copy_from_slice(&3u32.to_be_bytes());
    block[12..16].copy_from_slice(&0x10FFu32.to_be_bytes());
    block[296..312].copy_from_slice(&FS_UUID);
    block[320..328].copy_from_slice(&lsn.to_be_bytes());
    let crc = agi_crc(&block);
    block[312..316].copy_from_slice(&crc.to_le_bytes());
    block
}

fn agi_crc(object: &[u8]) -> u32 {
    let mut crc = crc32c(u32::MAX, &object[..312]);
    crc = crc32c(crc, &[0u8; 4]);
    crc = crc32c(crc, &object[316..]);
    !crc
}

fn current_inode(inode_number: u64, lsn: u64) -> Vec<u8> {
    let mut inode = vec![0u8; 256];
    inode[0..2].copy_from_slice(&0x494Eu16.to_be_bytes());
    inode[4] = 3;
    inode[96..100].copy_from_slice(&u32::MAX.to_be_bytes());
    inode[112..120].copy_from_slice(&lsn.to_be_bytes());
    inode[152..160].copy_from_slice(&inode_number.to_be_bytes());
    inode[160..176].copy_from_slice(&FS_UUID);
    let crc = metadata_crc(&inode);
    inode[100..104].copy_from_slice(&crc.to_le_bytes());
    inode
}

fn inode_replay_action(
    inode_number: u64,
    lsn: u64,
    writes: Vec<crate::log::replay::XfsReplayPatch>,
) -> crate::log::replay::XfsLogReplay {
    crate::log::replay::XfsLogReplay {
        actions: vec![crate::log::replay::XfsReplayAction::Inode(
            crate::log::replay::XfsInodeReplay {
                offset: 0,
                length: 256,
                lsn,
                inode_number,
                writes,
            },
        )],
        max_record_lsn: lsn,
        replayed_transactions: 1,
        skipped_items: 0,
    }
}

fn buffer_transaction(tid: u32, blkno_sectors: i64, fill: u8) -> Vec<Vec<u8>> {
    vec![
        operation(tid, XLOG_START_TRANS, &[]),
        operation(tid, 0, &transaction_header()),
        operation(tid, 0, &buf_descriptor(blkno_sectors)),
        operation(tid, 0, &[fill; 128]),
        operation(tid, XLOG_COMMIT_TRANS, &[]),
    ]
}

fn inode_transaction(tid: u32, ino: u64, blkno_sectors: i64) -> Vec<Vec<u8>> {
    vec![
        operation(tid, XLOG_START_TRANS, &[]),
        operation(tid, 0, &transaction_header()),
        operation(tid, 0, &inode_descriptor(ino, blkno_sectors)),
        operation(tid, 0, &logged_core_v3(ino)),
        operation(tid, XLOG_COMMIT_TRANS, &[]),
    ]
}

fn operation(transaction_id: u32, flags: u8, region: &[u8]) -> Vec<u8> {
    let mut operation = Vec::with_capacity(XLOG_OP_HEADER_SIZE + region.len());
    operation.extend_from_slice(&transaction_id.to_be_bytes());
    operation.extend_from_slice(&(region.len() as u32).to_be_bytes());
    operation.push(XFS_TRANSACTION_CLIENT);
    operation.push(flags);
    operation.extend_from_slice(&0u16.to_be_bytes());
    operation.extend_from_slice(region);
    operation
}

fn synthetic_log_record(
    block: u32,
    cycle: u32,
    tail_lsn: u64,
    previous_block: u32,
) -> crate::log::XfsLogRecord {
    let lsn = (u64::from(cycle) << 32) | u64::from(block);
    crate::log::XfsLogRecord {
        header: crate::log::LogRecordHeader {
            magic: crate::log::XLOG_HEADER_MAGIC_NUM,
            cycle,
            version: 2,
            data_len: XLOG_BASIC_BLOCK_SIZE as u32,
            lsn,
            tail_lsn,
            crc: 0,
            previous_block,
            operation_count: 0,
            format: crate::log::XfsLogFormat::LinuxLittleEndian,
            fs_uuid: FS_UUID,
            iclog_size: 32 * 1024,
        },
        log_block: block,
        source_offset: u64::from(block) * XLOG_BASIC_BLOCK_SIZE as u64,
        provenance: crate::log::XfsLogRecordProvenance {
            first: crate::log::XfsLogSourceSpan {
                snapshot_offset: u64::from(block) * XLOG_BASIC_BLOCK_SIZE as u64,
                source_offset: u64::from(block) * XLOG_BASIC_BLOCK_SIZE as u64,
                length: 2 * XLOG_BASIC_BLOCK_SIZE as u64,
            },
            second: None,
        },
        checksum_status: crate::log::XfsLogChecksumStatus::Verified,
        body: vec![0u8; XLOG_BASIC_BLOCK_SIZE],
    }
}

fn transaction_header() -> Vec<u8> {
    let mut header = Vec::with_capacity(16);
    header.extend_from_slice(&0x5452_414Eu32.to_le_bytes());
    header.extend_from_slice(&40u32.to_le_bytes());
    header.extend_from_slice(&0i32.to_le_bytes());
    header.extend_from_slice(&1u32.to_le_bytes());
    header
}

fn buf_descriptor(blkno_sectors: i64) -> Vec<u8> {
    let mut descriptor = vec![0u8; 24];
    descriptor[0..2].copy_from_slice(&XFS_LI_BUF.to_le_bytes());
    descriptor[2..4].copy_from_slice(&2u16.to_le_bytes());
    descriptor[4..6].copy_from_slice(&(7u16 << 11).to_le_bytes());
    descriptor[6..8].copy_from_slice(&8u16.to_le_bytes());
    descriptor[8..16].copy_from_slice(&blkno_sectors.to_le_bytes());
    descriptor[16..20].copy_from_slice(&1u32.to_le_bytes());
    descriptor[20..24].copy_from_slice(&2u32.to_le_bytes());
    descriptor
}

fn cancel_buf_descriptor(blkno_sectors: i64) -> Vec<u8> {
    let mut descriptor = buf_descriptor(blkno_sectors);
    descriptor[2..4].copy_from_slice(&1u16.to_le_bytes());
    descriptor[4..6].copy_from_slice(&(1u16 << 1).to_le_bytes());
    descriptor
}

fn inode_descriptor(ino: u64, blkno_sectors: i64) -> Vec<u8> {
    let mut descriptor = vec![0u8; 56];
    descriptor[0..2].copy_from_slice(&XFS_LI_INODE.to_le_bytes());
    descriptor[2..4].copy_from_slice(&2u16.to_le_bytes());
    descriptor[4..8].copy_from_slice(&1u32.to_le_bytes());
    descriptor[16..24].copy_from_slice(&ino.to_le_bytes());
    descriptor[40..48].copy_from_slice(&blkno_sectors.to_le_bytes());
    descriptor[48..52].copy_from_slice(&8i32.to_le_bytes());
    descriptor[52..56].copy_from_slice(&0i32.to_le_bytes());
    descriptor
}

fn logged_core_v3(ino: u64) -> Vec<u8> {
    let mut core = vec![0u8; 176];
    core[0..2].copy_from_slice(&0x494Eu16.to_le_bytes());
    core[2..4].copy_from_slice(&0x81A4u16.to_le_bytes());
    core[4] = 3;
    core[5] = 2;
    core[16..20].copy_from_slice(&2u32.to_le_bytes());
    core[152..160].copy_from_slice(&ino.to_le_bytes());
    core[160..176].copy_from_slice(&FS_UUID);
    core
}

fn icreate_region(ag: u32, agbno: u32, count: u32, isize: u32, length: u32, gen: u32) -> Vec<u8> {
    let mut region = vec![0u8; 28];
    region[0..2].copy_from_slice(&XFS_LI_ICREATE.to_le_bytes());
    region[2..4].copy_from_slice(&1u16.to_le_bytes());
    region[4..8].copy_from_slice(&ag.to_be_bytes());
    region[8..12].copy_from_slice(&agbno.to_be_bytes());
    region[12..16].copy_from_slice(&count.to_be_bytes());
    region[16..20].copy_from_slice(&isize.to_be_bytes());
    region[20..24].copy_from_slice(&length.to_be_bytes());
    region[24..28].copy_from_slice(&gen.to_be_bytes());
    region
}

fn deferred_transactions(efi_id: Option<u64>, efd_id: u64) -> Vec<Vec<u8>> {
    let mut operations = Vec::new();
    if let Some(id) = efi_id {
        operations.extend(one_item_transaction(1, &deferred_item(XFS_LI_EFI, id), &[]));
    }
    operations.extend(one_item_transaction(
        2,
        &deferred_item(XFS_LI_EFD, efd_id),
        &[],
    ));
    operations
}

fn deferred_item(item_type: u16, id: u64) -> Vec<u8> {
    let mut region = generic_item(item_type, 1, 28);
    region[4..8].copy_from_slice(&1u32.to_le_bytes());
    region[8..16].copy_from_slice(&id.to_le_bytes());
    region[16..24].copy_from_slice(&200u64.to_le_bytes());
    region[24..28].copy_from_slice(&1u32.to_le_bytes());
    region
}

fn generic_item(item_type: u16, regions: u16, length: usize) -> Vec<u8> {
    let mut item = vec![0u8; length];
    item[0..2].copy_from_slice(&item_type.to_le_bytes());
    item[2..4].copy_from_slice(&regions.to_le_bytes());
    item
}

fn one_item_transaction(tid: u32, descriptor: &[u8], regions: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut operations = vec![
        operation(tid, XLOG_START_TRANS, &[]),
        operation(tid, 0, &transaction_header()),
        operation(tid, 0, descriptor),
    ];
    operations.extend(regions.iter().map(|region| operation(tid, 0, region)));
    operations.push(operation(tid, XLOG_COMMIT_TRANS, &[]));
    operations
}

fn filesystem_with_record(
    start_log_block: usize,
    cycle: u32,
    iclog_size: usize,
    operations: &[Vec<u8>],
) -> Vec<u8> {
    let mut image = build_filesystem_image();
    let record = build_record(start_log_block, cycle, iclog_size, operations);
    let log =
        &mut image[LOG_START_FSB * FS_BLOCK_SIZE..(LOG_START_FSB + LOG_BLOCKS) * FS_BLOCK_SIZE];
    write_circular(log, start_log_block * XLOG_BASIC_BLOCK_SIZE, &record);
    image
}

fn build_filesystem_image() -> Vec<u8> {
    let mut image = vec![0u8; FS_BLOCKS * FS_BLOCK_SIZE];
    let superblock = &mut image[..512];
    superblock[sb_off::MAGIC..sb_off::MAGIC + 4].copy_from_slice(&XFS_SUPER_MAGIC.to_be_bytes());
    superblock[sb_off::BLOCKSIZE..sb_off::BLOCKSIZE + 4]
        .copy_from_slice(&(FS_BLOCK_SIZE as u32).to_be_bytes());
    superblock[sb_off::DBLOCKS..sb_off::DBLOCKS + 8]
        .copy_from_slice(&(FS_BLOCKS as u64).to_be_bytes());
    superblock[sb_off::UUID..sb_off::UUID + 16].copy_from_slice(&FS_UUID);
    superblock[sb_off::LOGSTART..sb_off::LOGSTART + 8]
        .copy_from_slice(&(LOG_START_FSB as u64).to_be_bytes());
    superblock[sb_off::ROOTINO..sb_off::ROOTINO + 8].copy_from_slice(&2u64.to_be_bytes());
    superblock[sb_off::AGBLOCKS..sb_off::AGBLOCKS + 4]
        .copy_from_slice(&(FS_BLOCKS as u32).to_be_bytes());
    superblock[sb_off::AGCOUNT..sb_off::AGCOUNT + 4].copy_from_slice(&1u32.to_be_bytes());
    superblock[sb_off::LOGBLOCKS..sb_off::LOGBLOCKS + 4]
        .copy_from_slice(&(LOG_BLOCKS as u32).to_be_bytes());
    superblock[sb_off::VERSIONNUM..sb_off::VERSIONNUM + 2].copy_from_slice(&5u16.to_be_bytes());
    superblock[sb_off::SECTSIZE..sb_off::SECTSIZE + 2]
        .copy_from_slice(&(XLOG_BASIC_BLOCK_SIZE as u16).to_be_bytes());
    superblock[sb_off::INODESIZE..sb_off::INODESIZE + 2].copy_from_slice(&256u16.to_be_bytes());
    superblock[sb_off::INOPBLOCK..sb_off::INOPBLOCK + 2].copy_from_slice(&16u16.to_be_bytes());
    superblock[sb_off::LOGSECTSIZE..sb_off::LOGSECTSIZE + 2]
        .copy_from_slice(&(XLOG_BASIC_BLOCK_SIZE as u16).to_be_bytes());
    superblock[sb_off::INOPBLOG] = 4;
    superblock[sb_off::AGBLKLOG] = 8;
    let agi = agi_block(0);
    let target = 200 * FS_BLOCK_SIZE;
    image[target..target + FS_BLOCK_SIZE].copy_from_slice(&agi);
    image
}

fn build_record(
    start_log_block: usize,
    cycle: u32,
    iclog_size: usize,
    operations: &[Vec<u8>],
) -> Vec<u8> {
    let header_blocks = iclog_size.div_ceil(crate::log::XLOG_HEADER_CYCLE_SIZE);
    let mut header = vec![0u8; header_blocks * XLOG_BASIC_BLOCK_SIZE];
    let mut body = operations.iter().flatten().copied().collect::<Vec<_>>();
    body.resize(
        body.len().div_ceil(XLOG_BASIC_BLOCK_SIZE) * XLOG_BASIC_BLOCK_SIZE,
        0,
    );

    header[0..4].copy_from_slice(&crate::log::XLOG_HEADER_MAGIC_NUM.to_be_bytes());
    header[4..8].copy_from_slice(&cycle.to_be_bytes());
    header[8..12].copy_from_slice(&2u32.to_be_bytes());
    header[12..16].copy_from_slice(&(body.len() as u32).to_be_bytes());
    let lsn = (u64::from(cycle) << 32) | start_log_block as u64;
    header[16..24].copy_from_slice(&lsn.to_be_bytes());
    header[24..32].copy_from_slice(&lsn.to_be_bytes());
    header[36..40].copy_from_slice(&u32::MAX.to_be_bytes());
    header[40..44].copy_from_slice(&(operations.len() as u32).to_be_bytes());
    header[300..304].copy_from_slice(&1u32.to_be_bytes());
    header[304..320].copy_from_slice(&FS_UUID);
    header[320..324].copy_from_slice(&(iclog_size as u32).to_be_bytes());

    for body_block in 0..body.len() / XLOG_BASIC_BLOCK_SIZE {
        let body_offset = body_block * XLOG_BASIC_BLOCK_SIZE;
        let original = u32::from_be_bytes(body[body_offset..body_offset + 4].try_into().unwrap());
        let words_per_header = crate::log::XLOG_HEADER_CYCLE_SIZE / XLOG_BASIC_BLOCK_SIZE;
        let cycle_word = 44 + body_block * 4;
        if body_block < words_per_header {
            header[cycle_word..cycle_word + 4].copy_from_slice(&original.to_be_bytes());
        }
        body[body_offset..body_offset + 4].copy_from_slice(&cycle.to_be_bytes());
    }

    let checksum = super::super::checksum::xlog_checksum(&header, &body, 328).unwrap();
    header[32..36].copy_from_slice(&checksum.to_le_bytes());

    header.extend_from_slice(&body);
    header
}

fn write_circular(target: &mut [u8], start: usize, bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        target[(start + index) % target.len()] = *byte;
    }
}
