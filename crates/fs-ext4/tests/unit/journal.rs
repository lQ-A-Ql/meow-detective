use super::*;
use crate::journal::checksum::{crc32c, crc32c_with_zeroed_range, journal_checksum_seed};
use crate::journal::types::JBD2_CRC32C_CHKSUM;
use crate::Ext4Reader;
use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

const BLOCK_SIZE: usize = 1024;
const JOURNAL_UUID: [u8; 16] = *b"journal-wire-001";
const FILESYSTEM_UUID: [u8; 16] = *b"ext4-wire-fs-001";

#[path = "journal_content_recovery.rs"]
mod content_recovery;

mod cases {
    use super::*;

    #[test]
    fn crc32c_matches_the_castagnoli_check_vector() {
        assert_eq!(crc32c(u32::MAX, b"123456789"), 0x1CF9_6D7C);
    }

    #[test]
    fn parses_v2_superblock_and_single_common_magic() {
        let bytes = build_superblock(SuperblockSpec::default());
        let superblock = JournalSuperblock::parse(&bytes).unwrap();

        assert_eq!(JBD2_MAGIC_NUMBER, 0xC03B_3998);
        assert_eq!(superblock.version, JournalSuperblockVersion::V2);
        assert_eq!(superblock.header.block_type, JournalBlockType::SuperblockV2);
        assert_eq!(superblock.block_size, BLOCK_SIZE as u32);
        assert_eq!(superblock.max_len, 12);
        assert_eq!(superblock.first, 1);
        assert_eq!(superblock.sequence, 7);
        assert_eq!(superblock.uuid, JOURNAL_UUID);
    }

    #[test]
    fn parses_v1_superblock_without_v2_feature_fields() {
        let mut bytes = build_superblock(SuperblockSpec::default());
        put_header(&mut bytes, JournalBlockType::SuperblockV1, 0);
        bytes[0x24..0x100].fill(0xA5);

        let superblock = JournalSuperblock::parse(&bytes).unwrap();

        assert_eq!(superblock.version, JournalSuperblockVersion::V1);
        assert_eq!(superblock.feature_compat, 0);
        assert_eq!(superblock.feature_incompat, 0);
        assert_eq!(superblock.uuid, [0; 16]);
        assert_eq!(superblock.tag_format(), JournalTagFormat::Legacy32);
    }

    #[test]
    fn rejects_invalid_and_truncated_superblocks() {
        let mut invalid = build_superblock(SuperblockSpec::default());
        invalid[0] ^= 0xFF;
        assert!(matches!(
            JournalSuperblock::parse(&invalid),
            Err(JournalError::Invalid(_))
        ));
        assert!(matches!(
            JournalSuperblock::parse(&invalid[..128]),
            Err(JournalError::Truncated { .. })
        ));
    }

    #[test]
    fn rejects_unknown_incompatible_features_with_typed_error() {
        let bytes = build_superblock(SuperblockSpec {
            incompat: 0x8000_0000,
            ..SuperblockSpec::default()
        });

        assert!(matches!(
            JournalSuperblock::parse(&bytes),
            Err(JournalError::Unsupported(_))
        ));
    }

    #[test]
    fn parses_legacy_tags_with_uuid_omission_and_escape() {
        let superblock = parsed_superblock(SuperblockSpec::default());
        let payloads = [payload(0x11), payload(0x22)];
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[
                TagSpec::new(40, JBD2_FLAG_ESCAPE),
                TagSpec::new(41, JBD2_FLAG_SAME_UUID | JBD2_FLAG_LAST_TAG),
            ],
            &payloads,
        );

        let parsed = parse_descriptor_block(&descriptor, &superblock).unwrap();

        assert_eq!(parsed.tags.len(), 2);
        assert_eq!(parsed.tags[0].target_block, 40);
        assert_eq!(parsed.tags[1].target_block, 41);
        assert_eq!(parsed.tags[0].uuid, FILESYSTEM_UUID);
        assert_eq!(parsed.tags[1].uuid, FILESYSTEM_UUID);
        assert!(parsed.tags[0].is_escaped());
        assert!(parsed.tags[1].is_last());
    }

    #[test]
    fn parses_checksum_v2_64bit_tag_layout() {
        let spec = SuperblockSpec {
            incompat: JBD2_FEATURE_INCOMPAT_CSUM_V2 | JBD2_FEATURE_INCOMPAT_64BIT,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let payloads = [payload(0x31)];
        let target = (3u64 << 32) | 0x1234;
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[TagSpec::new(target, JBD2_FLAG_LAST_TAG)],
            &payloads,
        );

        let parsed = parse_descriptor_block(&descriptor, &superblock).unwrap();

        assert_eq!(superblock.tag_format(), JournalTagFormat::ChecksumV2_64);
        assert_eq!(parsed.tags[0].target_block, target);
        assert!(matches!(
            parsed.tags[0].checksum,
            Some(JournalTagChecksum::V2(_))
        ));
        assert!(parsed.checksum.is_some());
    }

    #[test]
    fn parses_checksum_v3_tag_layout() {
        let spec = SuperblockSpec {
            incompat: JBD2_FEATURE_INCOMPAT_CSUM_V3 | JBD2_FEATURE_INCOMPAT_64BIT,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let payloads = [payload(0x41)];
        let target = (5u64 << 32) | 0x4321;
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[TagSpec::new(target, JBD2_FLAG_LAST_TAG)],
            &payloads,
        );

        let parsed = parse_descriptor_block(&descriptor, &superblock).unwrap();

        assert_eq!(superblock.tag_format(), JournalTagFormat::ChecksumV3);
        assert_eq!(parsed.tags[0].target_block, target);
        assert!(matches!(
            parsed.tags[0].checksum,
            Some(JournalTagChecksum::V3(_))
        ));
    }

    #[test]
    fn descriptor_rejects_missing_first_uuid_and_last_marker() {
        let superblock = parsed_superblock(SuperblockSpec::default());
        let payloads = [payload(0x51)];
        let missing_uuid = build_descriptor(
            &superblock,
            7,
            &[TagSpec::new(20, JBD2_FLAG_SAME_UUID | JBD2_FLAG_LAST_TAG)],
            &payloads,
        );
        assert!(matches!(
            parse_descriptor_block(&missing_uuid, &superblock),
            Err(JournalError::Invalid(_))
        ));

        let mut missing_last = vec![0u8; BLOCK_SIZE];
        put_header(&mut missing_last, JournalBlockType::Descriptor, 7);
        write_legacy_tag(&mut missing_last[12..20], 20, 0);
        missing_last[20..36].copy_from_slice(&FILESYSTEM_UUID);
        assert!(matches!(
            parse_descriptor_block(&missing_last, &superblock),
            Err(JournalError::Invalid(_))
        ));
    }

    #[test]
    fn ring_maps_descriptor_payload_blocks_and_commit() {
        let spec = SuperblockSpec {
            incompat: JBD2_FEATURE_INCOMPAT_CSUM_V3,
            start: 1,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let payloads = [payload(0x61), payload(0x62)];
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[
                TagSpec::new(100, 0),
                TagSpec::new(101, JBD2_FLAG_SAME_UUID | JBD2_FLAG_LAST_TAG),
            ],
            &payloads,
        );
        let mut journal = journal_image(spec);
        put_block(&mut journal, 1, &descriptor);
        put_block(&mut journal, 2, &payloads[0]);
        put_block(&mut journal, 3, &payloads[1]);
        put_block(&mut journal, 4, &build_commit(&superblock, 7));

        let scan = parse_journal(&journal).unwrap();

        assert_eq!(scan.transactions.len(), 1);
        let transaction = &scan.transactions[0];
        assert_eq!(transaction.sequence, 7);
        assert_eq!(transaction.commit.journal_block, 4);
        assert_eq!(transaction.mappings[0].payload_journal_block, 2);
        assert_eq!(transaction.mappings[1].payload_journal_block, 3);
        assert_eq!(transaction.mappings[1].target_filesystem_block, 101);
        assert!(scan.incomplete_transaction.is_none());
    }

    #[test]
    fn ring_wraps_descriptor_payloads_before_commit() {
        let spec = SuperblockSpec {
            max_len: 8,
            start: 6,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let payloads = [payload(0x71), payload(0x72)];
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[
                TagSpec::new(200, 0),
                TagSpec::new(201, JBD2_FLAG_SAME_UUID | JBD2_FLAG_LAST_TAG),
            ],
            &payloads,
        );
        let mut journal = journal_image(spec);
        put_block(&mut journal, 6, &descriptor);
        put_block(&mut journal, 7, &payloads[0]);
        put_block(&mut journal, 1, &payloads[1]);
        put_block(&mut journal, 2, &build_commit(&superblock, 7));

        let scan = parse_journal(&journal).unwrap();

        assert_eq!(scan.transactions.len(), 1);
        assert_eq!(scan.transactions[0].mappings[0].payload_journal_block, 7);
        assert_eq!(scan.transactions[0].mappings[1].payload_journal_block, 1);
        assert_eq!(scan.transactions[0].commit.journal_block, 2);
        assert_eq!(scan.next_journal_block, 3);
    }

    #[test]
    fn later_revoke_suppresses_earlier_mapping() {
        let spec = SuperblockSpec {
            incompat: JBD2_FEATURE_INCOMPAT_REVOKE,
            start: 1,
            sequence: 10,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let payloads = [payload(0x81)];
        let descriptor = build_descriptor(
            &superblock,
            10,
            &[TagSpec::new(300, JBD2_FLAG_LAST_TAG)],
            &payloads,
        );
        let mut journal = journal_image(spec);
        put_block(&mut journal, 1, &descriptor);
        put_block(&mut journal, 2, &payloads[0]);
        put_block(&mut journal, 3, &build_commit(&superblock, 10));
        put_block(&mut journal, 4, &build_revoke(&superblock, 11, &[300]));
        put_block(&mut journal, 5, &build_commit(&superblock, 11));

        let scan = parse_journal(&journal).unwrap();

        assert_eq!(scan.transactions.len(), 2);
        assert!(scan.transactions[0].mappings[0].revoked);
        assert_eq!(scan.transactions[1].revokes[0].revoke.revoked_blocks, [300]);
    }

    #[test]
    fn parses_64bit_revoke_records() {
        let spec = SuperblockSpec {
            incompat: JBD2_FEATURE_INCOMPAT_REVOKE | JBD2_FEATURE_INCOMPAT_64BIT,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let blocks = [(2u64 << 32) | 9, (4u64 << 32) | 11];
        let revoke = build_revoke(&superblock, 7, &blocks);

        let parsed = parse_revoke_block(&revoke, &superblock).unwrap();

        assert_eq!(parsed.revoked_blocks, blocks);
        assert_eq!(parsed.bytes_used, 32);
    }

    #[test]
    fn rejects_truncated_snapshot_and_corrupt_payload_checksum() {
        let spec = SuperblockSpec {
            incompat: JBD2_FEATURE_INCOMPAT_CSUM_V3,
            start: 1,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let payloads = [payload(0x91)];
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[TagSpec::new(400, JBD2_FLAG_LAST_TAG)],
            &payloads,
        );
        let mut journal = journal_image(spec);
        put_block(&mut journal, 1, &descriptor);
        put_block(&mut journal, 2, &payloads[0]);
        put_block(&mut journal, 3, &build_commit(&superblock, 7));

        assert!(matches!(
            parse_journal(&journal[..journal.len() - BLOCK_SIZE]),
            Err(JournalError::Truncated { .. })
        ));

        journal[2 * BLOCK_SIZE + 50] ^= 0xFF;
        assert!(matches!(
            parse_journal(&journal),
            Err(JournalError::Invalid(_))
        ));
    }

    #[test]
    fn reports_incomplete_transaction_without_publishing_it() {
        let spec = SuperblockSpec {
            start: 1,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let payloads = [payload(0xA1)];
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[TagSpec::new(500, JBD2_FLAG_LAST_TAG)],
            &payloads,
        );
        let mut journal = journal_image(spec);
        put_block(&mut journal, 1, &descriptor);
        put_block(&mut journal, 2, &payloads[0]);

        let scan = parse_journal(&journal).unwrap();

        assert!(scan.transactions.is_empty());
        assert_eq!(scan.incomplete_transaction.unwrap().sequence, 7);
    }

    #[test]
    fn deleted_recovery_only_emits_inode_table_metadata() {
        let filesystem = build_ext4_reader(false, None);
        let spec = SuperblockSpec::default();
        let superblock = parsed_superblock(spec);
        let mut inode_table_payload = vec![0u8; BLOCK_SIZE];
        write_deleted_inode(&mut inode_table_payload[512..768], 1234, 0x1234_5678);
        let payloads = [inode_table_payload];
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[TagSpec::new(10, JBD2_FLAG_LAST_TAG)],
            &payloads,
        );
        let mut journal = journal_image(spec);
        put_block(&mut journal, 1, &descriptor);
        put_block(&mut journal, 2, &payloads[0]);
        put_block(&mut journal, 3, &build_commit(&superblock, 7));

        assert!(parse_journal(&journal).unwrap().transactions.is_empty());
        assert_eq!(
            parse_journal_history(&journal).unwrap().transactions.len(),
            1
        );

        let candidates = recover_deleted_inodes(&filesystem, &journal).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].inode, 3);
        assert_eq!(candidates[0].declared_size, 1234);
        assert_eq!(candidates[0].inode_table_group, 0);
        assert_eq!(candidates[0].inode_table_block, 10);
        assert_eq!(candidates[0].payload_journal_block, 2);
        assert_eq!(candidates[0].inode_offset_within_payload, 512);
        assert_eq!(
            candidates[0].journal_source_offset,
            2 * BLOCK_SIZE as u64 + 512
        );
        assert_eq!(candidates[0].journal_source_length, 256);
        assert_eq!(
            candidates[0].completeness,
            RecoveryCompleteness::MetadataOnly
        );
        assert_eq!(candidates[0].recoverable_bytes, 0);
        assert!(!candidates[0].tag_marked_deleted);
        assert!(!candidates[0].replay_revoked);
        assert!(!candidates[0].inode_checksum_verified);
    }

    #[test]
    fn rejects_misaligned_revoke_record_bytes() {
        let spec = SuperblockSpec {
            incompat: JBD2_FEATURE_INCOMPAT_REVOKE,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let mut revoke = build_revoke(&superblock, 7, &[42]);
        write_be_u32(&mut revoke, 12, 19);

        assert!(matches!(
            parse_revoke_block(&revoke, &superblock),
            Err(JournalError::Invalid(_))
        ));
    }

    #[test]
    fn recovery_ignores_plausible_inode_bytes_outside_inode_table() {
        let filesystem = build_ext4_reader(false, None);
        let spec = SuperblockSpec {
            start: 1,
            ..SuperblockSpec::default()
        };
        let superblock = parsed_superblock(spec);
        let mut unrelated_payload = vec![0u8; BLOCK_SIZE];
        write_deleted_inode(&mut unrelated_payload[0..256], 4096, 0x2222_3333);
        let payloads = [unrelated_payload];
        let descriptor = build_descriptor(
            &superblock,
            7,
            &[TagSpec::new(30, JBD2_FLAG_DELETED | JBD2_FLAG_LAST_TAG)],
            &payloads,
        );
        let mut journal = journal_image(spec);
        put_block(&mut journal, 1, &descriptor);
        put_block(&mut journal, 2, &payloads[0]);
        put_block(&mut journal, 3, &build_commit(&superblock, 7));

        assert!(recover_deleted_inodes(&filesystem, &journal)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn recovery_rejects_inode_table_mapping_for_another_filesystem_uuid() {
        let filesystem = build_ext4_reader(false, None);
        let spec = SuperblockSpec::default();
        let superblock = parsed_superblock(spec);
        let mut inode_table_payload = vec![0u8; BLOCK_SIZE];
        write_deleted_inode(&mut inode_table_payload[0..256], 512, 0x4455_6677);
        let payloads = [inode_table_payload];
        let mut descriptor = build_descriptor(
            &superblock,
            7,
            &[TagSpec::new(10, JBD2_FLAG_LAST_TAG)],
            &payloads,
        );
        descriptor[20..36].copy_from_slice(b"another-fs-uuid!");
        let mut journal = journal_image(spec);
        put_block(&mut journal, 1, &descriptor);
        put_block(&mut journal, 2, &payloads[0]);
        put_block(&mut journal, 3, &build_commit(&superblock, 7));

        assert!(recover_deleted_inodes(&filesystem, &journal)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn reads_bounded_internal_journal_from_declared_inode() {
        let spec = SuperblockSpec {
            max_len: 8,
            start: 0,
            ..SuperblockSpec::default()
        };
        let journal = journal_image(spec);
        let filesystem = build_ext4_reader(true, Some(&journal));

        assert!(matches!(
            filesystem.read_internal_journal(journal.len() - 1),
            Err(JournalError::Unsupported(_))
        ));
        let read = filesystem.read_internal_journal(journal.len()).unwrap();
        assert_eq!(read, journal);
        assert!(filesystem
            .scan_internal_journal(journal.len())
            .unwrap()
            .transactions
            .is_empty());
    }

    #[test]
    fn history_scan_bounds_total_scanned_blocks_across_candidates() {
        let spec = SuperblockSpec::default();
        let superblock = parsed_superblock(spec);
        let mut journal = journal_image(spec);
        // Every one of the 11 ring blocks carries descriptor magic with a
        // distinct sequence and enough tags to walk the entire ring without
        // ever committing: a full scan per candidate would be quadratic.
        for block in 1u32..12 {
            let sequence = 100 + block;
            let mut tags = Vec::new();
            let mut payloads = Vec::new();
            for index in 0..10u32 {
                let flags = if index == 9 {
                    JBD2_FLAG_SAME_UUID | JBD2_FLAG_LAST_TAG
                } else if index == 0 {
                    0
                } else {
                    JBD2_FLAG_SAME_UUID
                };
                tags.push(TagSpec::new(u64::from(500 + index), flags));
                payloads.push(payload((index + 1) as u8));
            }
            let descriptor = build_descriptor(&superblock, sequence, &tags, &payloads);
            put_block(&mut journal, block, &descriptor);
        }

        let history = parse_journal_history(&journal).unwrap();

        assert!(history.transactions.is_empty());
        assert!(
            !history.rejected_candidates.is_empty(),
            "the first candidates are still scanned and reported"
        );
        assert!(
            history.rejected_candidates.len() < 11,
            "the shared scan budget must stop per-candidate full-ring rescans, got {} rejections",
            history.rejected_candidates.len()
        );
    }
}

#[derive(Clone, Copy)]
struct SuperblockSpec {
    max_len: u32,
    first: u32,
    start: u32,
    sequence: u32,
    compat: u32,
    incompat: u32,
}

impl Default for SuperblockSpec {
    fn default() -> Self {
        Self {
            max_len: 12,
            first: 1,
            start: 0,
            sequence: 7,
            compat: 0,
            incompat: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct TagSpec {
    target: u64,
    flags: u32,
}

impl TagSpec {
    fn new(target: u64, flags: u32) -> Self {
        Self { target, flags }
    }
}

fn build_superblock(spec: SuperblockSpec) -> Vec<u8> {
    let mut block = vec![0u8; BLOCK_SIZE];
    put_header(&mut block, JournalBlockType::SuperblockV2, 0);
    write_be_u32(&mut block, 0x0C, BLOCK_SIZE as u32);
    write_be_u32(&mut block, 0x10, spec.max_len);
    write_be_u32(&mut block, 0x14, spec.first);
    write_be_u32(&mut block, 0x18, spec.sequence);
    write_be_u32(&mut block, 0x1C, spec.start);
    write_be_u32(&mut block, 0x24, spec.compat);
    write_be_u32(&mut block, 0x28, spec.incompat);
    block[0x30..0x40].copy_from_slice(&JOURNAL_UUID);
    if has_modern_checksums(spec.incompat) {
        block[0x50] = JBD2_CRC32C_CHKSUM;
        let checksum = crc32c_with_zeroed_range(u32::MAX, &block, 0xFC..0x100).unwrap();
        write_be_u32(&mut block, 0xFC, checksum);
    }
    block
}

fn parsed_superblock(spec: SuperblockSpec) -> JournalSuperblock {
    JournalSuperblock::parse(&build_superblock(spec)).unwrap()
}

fn journal_image(spec: SuperblockSpec) -> Vec<u8> {
    let mut journal = vec![0u8; spec.max_len as usize * BLOCK_SIZE];
    put_block(&mut journal, 0, &build_superblock(spec));
    journal
}

fn build_descriptor(
    superblock: &JournalSuperblock,
    sequence: u32,
    tags: &[TagSpec],
    payloads: &[Vec<u8>],
) -> Vec<u8> {
    assert_eq!(tags.len(), payloads.len());
    let mut block = vec![0u8; BLOCK_SIZE];
    put_header(&mut block, JournalBlockType::Descriptor, sequence);
    let format = superblock.tag_format();
    let mut cursor = JOURNAL_HEADER_SIZE;
    for (tag, payload) in tags.iter().zip(payloads) {
        let checksum = payload_checksum(superblock, sequence, payload);
        write_tag(&mut block[cursor..], format, *tag, checksum);
        cursor += format.byte_len();
        if tag.flags & JBD2_FLAG_SAME_UUID == 0 {
            block[cursor..cursor + 16].copy_from_slice(&FILESYSTEM_UUID);
            cursor += 16;
        }
    }
    finalize_metadata_checksum(&mut block, superblock);
    block
}

fn write_tag(output: &mut [u8], format: JournalTagFormat, tag: TagSpec, checksum: u32) {
    write_be_u32(output, 0, tag.target as u32);
    match format {
        JournalTagFormat::Legacy32 => write_be_u16(output, 6, tag.flags as u16),
        JournalTagFormat::Legacy64 => {
            write_be_u16(output, 6, tag.flags as u16);
            write_be_u32(output, 8, (tag.target >> 32) as u32);
        }
        JournalTagFormat::ChecksumV2_32 => {
            write_be_u16(output, 4, checksum as u16);
            write_be_u16(output, 6, tag.flags as u16);
        }
        JournalTagFormat::ChecksumV2_64 => {
            write_be_u16(output, 4, checksum as u16);
            write_be_u16(output, 6, tag.flags as u16);
            write_be_u32(output, 8, (tag.target >> 32) as u32);
        }
        JournalTagFormat::ChecksumV3 => {
            write_be_u32(output, 4, tag.flags);
            write_be_u32(output, 8, (tag.target >> 32) as u32);
            write_be_u32(output, 12, checksum);
        }
    }
}

fn write_legacy_tag(output: &mut [u8], target: u32, flags: u16) {
    write_be_u32(output, 0, target);
    write_be_u16(output, 6, flags);
}

fn build_commit(superblock: &JournalSuperblock, sequence: u32) -> Vec<u8> {
    let mut block = vec![0u8; BLOCK_SIZE];
    put_header(&mut block, JournalBlockType::Commit, sequence);
    block[48..56].copy_from_slice(&1_700_000_000u64.to_be_bytes());
    block[56..60].copy_from_slice(&123_456_789u32.to_be_bytes());
    if superblock.uses_v2_or_v3_checksums() {
        block[12] = JBD2_CRC32C_CHKSUM;
        block[13] = 4;
        let checksum =
            crc32c_with_zeroed_range(journal_checksum_seed(&superblock.uuid), &block, 16..20)
                .unwrap();
        write_be_u32(&mut block, 16, checksum);
    }
    block
}

fn build_revoke(superblock: &JournalSuperblock, sequence: u32, blocks: &[u64]) -> Vec<u8> {
    let mut output = vec![0u8; BLOCK_SIZE];
    put_header(&mut output, JournalBlockType::Revoke, sequence);
    let record_size = if superblock.has_64bit_block_numbers() {
        8
    } else {
        4
    };
    write_be_u32(&mut output, 12, (16 + record_size * blocks.len()) as u32);
    let mut cursor = 16;
    for block in blocks {
        if record_size == 8 {
            output[cursor..cursor + 8].copy_from_slice(&block.to_be_bytes());
        } else {
            write_be_u32(&mut output, cursor, *block as u32);
        }
        cursor += record_size;
    }
    finalize_metadata_checksum(&mut output, superblock);
    output
}

fn finalize_metadata_checksum(block: &mut [u8], superblock: &JournalSuperblock) {
    if !superblock.uses_v2_or_v3_checksums() {
        return;
    }
    let checksum_offset = block.len() - 4;
    let checksum = crc32c_with_zeroed_range(
        journal_checksum_seed(&superblock.uuid),
        block,
        checksum_offset..block.len(),
    )
    .unwrap();
    write_be_u32(block, checksum_offset, checksum);
}

fn payload_checksum(superblock: &JournalSuperblock, sequence: u32, payload: &[u8]) -> u32 {
    if !superblock.uses_v2_or_v3_checksums() {
        return 0;
    }
    let checksum = crc32c(
        journal_checksum_seed(&superblock.uuid),
        &sequence.to_be_bytes(),
    );
    crc32c(checksum, payload)
}

fn payload(fill: u8) -> Vec<u8> {
    vec![fill; BLOCK_SIZE]
}

fn put_header(output: &mut [u8], block_type: JournalBlockType, sequence: u32) {
    write_be_u32(output, 0, JBD2_MAGIC_NUMBER);
    write_be_u32(output, 4, block_type as u32);
    write_be_u32(output, 8, sequence);
}

fn put_block(image: &mut [u8], block: u32, data: &[u8]) {
    assert_eq!(data.len(), BLOCK_SIZE);
    let start = block as usize * BLOCK_SIZE;
    image[start..start + BLOCK_SIZE].copy_from_slice(data);
}

fn write_be_u16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn write_be_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn has_modern_checksums(incompat: u32) -> bool {
    incompat & (JBD2_FEATURE_INCOMPAT_CSUM_V2 | JBD2_FEATURE_INCOMPAT_CSUM_V3) != 0
}

fn write_deleted_inode(inode: &mut [u8], size: u64, deletion_time: u32) {
    inode[0..2].copy_from_slice(&0x81A4u16.to_le_bytes());
    inode[4..8].copy_from_slice(&(size as u32).to_le_bytes());
    inode[0x14..0x18].copy_from_slice(&deletion_time.to_le_bytes());
    inode[0x1A..0x1C].copy_from_slice(&0u16.to_le_bytes());
    inode[0x6C..0x70].copy_from_slice(&((size >> 32) as u32).to_le_bytes());
}

fn build_ext4_reader(with_journal: bool, journal: Option<&[u8]>) -> Ext4Reader {
    build_ext4_reader_with_mutation(with_journal, journal, |_| {})
}

fn build_ext4_reader_with_mutation(
    with_journal: bool,
    journal: Option<&[u8]>,
    mutate: impl FnOnce(&mut Vec<u8>),
) -> Ext4Reader {
    let total_blocks = 40usize;
    let mut image = vec![0u8; total_blocks * BLOCK_SIZE];
    let superblock = &mut image[1024..2048];
    superblock[0x00..0x04].copy_from_slice(&16u32.to_le_bytes());
    superblock[0x04..0x08].copy_from_slice(&(total_blocks as u32).to_le_bytes());
    superblock[0x14..0x18].copy_from_slice(&1u32.to_le_bytes());
    superblock[0x18..0x1C].copy_from_slice(&0u32.to_le_bytes());
    superblock[0x20..0x24].copy_from_slice(&(total_blocks as u32).to_le_bytes());
    superblock[0x28..0x2C].copy_from_slice(&16u32.to_le_bytes());
    superblock[0x38..0x3A].copy_from_slice(&0xEF53u16.to_le_bytes());
    superblock[0x58..0x5A].copy_from_slice(&256u16.to_le_bytes());
    superblock[0x68..0x78].copy_from_slice(&FILESYSTEM_UUID);
    if with_journal {
        superblock[0x5C..0x60].copy_from_slice(&0x0004u32.to_le_bytes());
        superblock[0xE0..0xE4].copy_from_slice(&8u32.to_le_bytes());
    }
    image[2 * BLOCK_SIZE..2 * BLOCK_SIZE + 4].copy_from_slice(&3u32.to_le_bytes());
    image[2 * BLOCK_SIZE + 0x04..2 * BLOCK_SIZE + 0x08].copy_from_slice(&4u32.to_le_bytes());
    image[2 * BLOCK_SIZE + 0x08..2 * BLOCK_SIZE + 0x0C].copy_from_slice(&10u32.to_le_bytes());

    if let Some(journal) = journal {
        let inode_offset = 11 * BLOCK_SIZE + 768;
        let inode = &mut image[inode_offset..inode_offset + 256];
        inode[0..2].copy_from_slice(&0x8180u16.to_le_bytes());
        inode[4..8].copy_from_slice(&(journal.len() as u32).to_le_bytes());
        inode[0x28..0x2A].copy_from_slice(&0xF30Au16.to_le_bytes());
        inode[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes());
        inode[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes());
        inode[0x38..0x3A].copy_from_slice(&((journal.len() / BLOCK_SIZE) as u16).to_le_bytes());
        inode[0x3C..0x40].copy_from_slice(&20u32.to_le_bytes());
        image[20 * BLOCK_SIZE..20 * BLOCK_SIZE + journal.len()].copy_from_slice(journal);
    }

    mutate(&mut image);
    Ext4Reader::open(Box::new(MemoryEvidenceReader::new(image)), 0).unwrap()
}

struct MemoryEvidenceReader {
    cursor: io::Cursor<Vec<u8>>,
    info: ReaderInfo,
}

impl MemoryEvidenceReader {
    fn new(data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            cursor: io::Cursor::new(data),
            info: ReaderInfo {
                path: PathBuf::from("jbd2-wire-fixture"),
                size,
                kind: "memory".to_string(),
            },
        }
    }
}

impl Read for MemoryEvidenceReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buffer)
    }
}

impl Seek for MemoryEvidenceReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(position)
    }
}

impl EvidenceReader for MemoryEvidenceReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
