use super::*;
use crate::format::Ext4Extent;
use sha2::Digest;

const DATA_BLOCK_A: u64 = 30;
const DATA_BLOCK_B: u64 = 31;
const DELETED_INODE: u32 = 3;
const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
const EXT4_FEATURE_RO_COMPAT_METADATA_CSUM: u32 = 0x0400;

#[test]
fn free_inode_and_free_extent_blocks_produce_complete_hashed_content() {
    let inode = extent_inode(1_500, 0, &[(0, 2, DATA_BLOCK_A)]);
    let candidate = recover_one(content_reader(false, &[], false, None), &inode);

    assert_eq!(candidate.completeness, RecoveryCompleteness::Complete);
    assert_eq!(candidate.recoverable_bytes, 1_500);
    assert_eq!(
        candidate.content_mapping.inode_allocation_state,
        RecoveryAllocationState::Free
    );
    assert_eq!(
        candidate.content_mapping.data_allocation_state,
        RecoveryAllocationState::Free
    );
    assert_eq!(candidate.content_mapping.ranges.len(), 2);
    assert!(candidate
        .content_mapping
        .ranges
        .iter()
        .all(|range| range.kind == DeletedContentRangeKind::RecoverableData));
    assert!(candidate
        .content_mapping
        .ranges
        .iter()
        .all(|range| range.sha256.is_some()));

    let mut expected = vec![0xA5; BLOCK_SIZE];
    expected.extend(std::iter::repeat_n(0x5A, 1_500 - BLOCK_SIZE));
    assert_eq!(
        candidate.content_mapping.content_sha256.as_deref(),
        Some(crate::journal::checksum::sha256_hex(&expected).as_str())
    );
    assert_eq!(
        candidate.content_mapping.content_md5,
        Some(hex::encode(md5::Md5::digest(&expected)))
    );
    assert_eq!(
        candidate.content_mapping.content_sha1,
        Some(hex::encode(sha1::Sha1::digest(&expected)))
    );
}

#[test]
fn currently_allocated_inode_never_trusts_historical_extents() {
    let inode = extent_inode(BLOCK_SIZE as u64, 0, &[(0, 1, DATA_BLOCK_A)]);
    let candidate = recover_one(content_reader(true, &[], false, None), &inode);

    assert_eq!(candidate.completeness, RecoveryCompleteness::MetadataOnly);
    assert_eq!(candidate.recoverable_bytes, 0);
    assert!(candidate.content_mapping.ranges.is_empty());
    assert_eq!(
        candidate.content_mapping.inode_allocation_state,
        RecoveryAllocationState::Allocated
    );
    assert!(candidate
        .content_mapping
        .issue
        .as_deref()
        .is_some_and(|issue| issue.contains("currently allocated")));
}

#[test]
fn mixed_free_and_reallocated_blocks_produce_only_partial_content() {
    let inode = extent_inode(2 * BLOCK_SIZE as u64, 0, &[(0, 2, DATA_BLOCK_A)]);
    let candidate = recover_one(content_reader(false, &[DATA_BLOCK_B], false, None), &inode);

    assert_eq!(candidate.completeness, RecoveryCompleteness::Partial);
    assert_eq!(candidate.recoverable_bytes, BLOCK_SIZE as u64);
    assert_eq!(
        candidate.content_mapping.data_allocation_state,
        RecoveryAllocationState::Mixed
    );
    assert_eq!(
        candidate.content_mapping.ranges[0].kind,
        DeletedContentRangeKind::RecoverableData
    );
    assert_eq!(
        candidate.content_mapping.ranges[1].kind,
        DeletedContentRangeKind::AllocatedData
    );
    assert!(candidate.content_mapping.content_sha256.is_none());
}

#[test]
fn contiguous_nonrecoverable_blocks_coalesce_across_the_full_extent() {
    let inode = extent_inode(3 * BLOCK_SIZE as u64, 0, &[(0, 3, DATA_BLOCK_A)]);
    let candidate = recover_one(
        content_reader(
            false,
            &[DATA_BLOCK_A, DATA_BLOCK_B, DATA_BLOCK_B + 1],
            false,
            None,
        ),
        &inode,
    );

    assert_eq!(candidate.completeness, RecoveryCompleteness::MetadataOnly);
    assert_eq!(candidate.content_mapping.ranges.len(), 1);
    assert_eq!(
        candidate.content_mapping.ranges[0].kind,
        DeletedContentRangeKind::AllocatedData
    );
    assert_eq!(
        candidate.content_mapping.ranges[0].length,
        3 * BLOCK_SIZE as u64
    );
    assert_eq!(
        candidate.content_mapping.ranges[0].filesystem_block,
        Some(DATA_BLOCK_A)
    );
}

#[test]
fn sparse_and_unwritten_regions_are_not_claimed_as_recovered_bytes() {
    let sparse_inode = extent_inode(2 * BLOCK_SIZE as u64, 0, &[(1, 1, DATA_BLOCK_A)]);
    let sparse = recover_one(content_reader(false, &[], false, None), &sparse_inode);
    assert_eq!(sparse.completeness, RecoveryCompleteness::Partial);
    assert_eq!(sparse.recoverable_bytes, BLOCK_SIZE as u64);
    assert_eq!(
        sparse.content_mapping.ranges[0].kind,
        DeletedContentRangeKind::Sparse
    );
    assert!(sparse.content_mapping.content_sha256.is_none());

    let unwritten_inode = extent_inode(BLOCK_SIZE as u64, 0, &[(0, 0x8001, DATA_BLOCK_A)]);
    let unwritten = recover_one(content_reader(false, &[], false, None), &unwritten_inode);
    assert_eq!(unwritten.completeness, RecoveryCompleteness::MetadataOnly);
    assert_eq!(unwritten.recoverable_bytes, 0);
    assert_eq!(
        unwritten.content_mapping.ranges[0].kind,
        DeletedContentRangeKind::Unwritten
    );
    assert!(unwritten.content_mapping.content_sha256.is_none());
}

#[test]
fn unreadable_free_block_is_reported_without_content_claim() {
    let inode = extent_inode(BLOCK_SIZE as u64, 0, &[(0, 1, DATA_BLOCK_A)]);
    let candidate = recover_one(
        content_reader(false, &[], false, Some(25 * BLOCK_SIZE)),
        &inode,
    );

    assert_eq!(candidate.completeness, RecoveryCompleteness::MetadataOnly);
    assert_eq!(candidate.recoverable_bytes, 0);
    assert_eq!(
        candidate.content_mapping.ranges[0].kind,
        DeletedContentRangeKind::UnreadableData
    );
    assert_eq!(
        candidate.content_mapping.ranges[0].allocation_state,
        RecoveryAllocationState::Free
    );
}

#[test]
fn unsupported_extent_depth_and_safety_limit_remain_metadata_only() {
    let depth_one = extent_inode(BLOCK_SIZE as u64, 1, &[]);
    let depth_candidate = recover_one(content_reader(false, &[], false, None), &depth_one);
    assert_eq!(
        depth_candidate.completeness,
        RecoveryCompleteness::MetadataOnly
    );
    assert_eq!(
        depth_candidate.content_mapping.state,
        DeletedContentMappingState::Unsupported
    );
    assert!(depth_candidate
        .content_mapping
        .issue
        .as_deref()
        .is_some_and(|issue| issue.contains("extent depth 0")));

    let oversized = extent_inode(64 * 1024 * 1024 + 1, 0, &[(0, 1, DATA_BLOCK_A)]);
    let oversized_candidate = recover_one(content_reader(false, &[], false, None), &oversized);
    assert_eq!(
        oversized_candidate.completeness,
        RecoveryCompleteness::MetadataOnly
    );
    assert_eq!(
        oversized_candidate.content_mapping.state,
        DeletedContentMappingState::Unsupported
    );
    assert!(oversized_candidate
        .content_mapping
        .issue
        .as_deref()
        .is_some_and(|issue| issue.contains("validation limit")));
}

#[test]
fn extent_length_0x8000_is_initialized_and_represents_32768_blocks() {
    let extent = Ext4Extent::parse(&extent_record(0, 0x8000, DATA_BLOCK_A)).unwrap();

    assert_eq!(extent.block_count(), 32_768);
    assert!(!extent.is_unwritten());
}

#[test]
fn metadata_checksum_validates_exact_bitmap_bytes_and_rejects_corruption() {
    let mut inode = extent_inode(BLOCK_SIZE as u64, 0, &[(0, 1, DATA_BLOCK_A)]);
    apply_inode_checksum(&mut inode);
    let valid = recover_one(content_reader(false, &[], true, None), &inode);
    assert_eq!(valid.completeness, RecoveryCompleteness::Complete);
    assert!(valid.inode_checksum_verified);

    let corrupt = recover_one(checksummed_content_reader_with_corrupt_bitmap(), &inode);
    assert_eq!(corrupt.completeness, RecoveryCompleteness::MetadataOnly);
    assert_eq!(
        corrupt.content_mapping.state,
        DeletedContentMappingState::Invalid
    );
    assert!(corrupt
        .content_mapping
        .issue
        .as_deref()
        .is_some_and(|issue| issue.contains("bitmap checksum mismatch")));
}

#[test]
fn metadata_checksum_mismatch_blocks_content_recovery() {
    let mut inode = extent_inode(BLOCK_SIZE as u64, 0, &[(0, 1, DATA_BLOCK_A)]);
    apply_inode_checksum(&mut inode);
    inode[0x02] ^= 0x01;

    let candidate = recover_one(content_reader(false, &[], true, None), &inode);

    assert!(!candidate.inode_checksum_verified);
    assert_eq!(candidate.completeness, RecoveryCompleteness::MetadataOnly);
    assert_eq!(
        candidate.content_mapping.state,
        DeletedContentMappingState::Invalid
    );
    assert!(candidate.content_mapping.ranges.is_empty());
    assert!(candidate
        .content_mapping
        .issue
        .as_deref()
        .is_some_and(|issue| issue.contains("inode checksum mismatch")));
}

fn recover_one(filesystem: Ext4Reader, inode: &[u8; 256]) -> DeletedInodeCandidate {
    let journal = recovery_journal(inode);
    let mut candidates = recover_deleted_inodes(&filesystem, &journal).unwrap();
    assert_eq!(candidates.len(), 1);
    candidates.remove(0)
}

fn recovery_journal(inode: &[u8; 256]) -> Vec<u8> {
    let spec = SuperblockSpec::default();
    let superblock = parsed_superblock(spec);
    let mut inode_table_payload = vec![0u8; BLOCK_SIZE];
    inode_table_payload[512..768].copy_from_slice(inode);
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
    journal
}

fn extent_inode(size: u64, depth: u16, extents: &[(u32, u16, u64)]) -> [u8; 256] {
    let mut inode = [0u8; 256];
    write_deleted_inode(&mut inode, size, 0x1234_5678);
    inode[0x20..0x24].copy_from_slice(&EXT4_EXTENTS_FL.to_le_bytes());
    let i_block = &mut inode[0x28..0x28 + 60];
    i_block[0..2].copy_from_slice(&0xF30Au16.to_le_bytes());
    i_block[2..4].copy_from_slice(&(extents.len() as u16).to_le_bytes());
    i_block[4..6].copy_from_slice(&4u16.to_le_bytes());
    i_block[6..8].copy_from_slice(&depth.to_le_bytes());
    for (index, &(logical_block, length, physical_block)) in extents.iter().enumerate() {
        let start = 12 + index * 12;
        i_block[start..start + 12].copy_from_slice(&extent_record(
            logical_block,
            length,
            physical_block,
        ));
    }
    inode
}

fn extent_record(logical_block: u32, length: u16, physical_block: u64) -> [u8; 12] {
    let mut extent = [0u8; 12];
    extent[0..4].copy_from_slice(&logical_block.to_le_bytes());
    extent[4..6].copy_from_slice(&length.to_le_bytes());
    extent[6..8].copy_from_slice(&((physical_block >> 32) as u16).to_le_bytes());
    extent[8..12].copy_from_slice(&(physical_block as u32).to_le_bytes());
    extent
}

fn content_reader(
    inode_allocated: bool,
    allocated_blocks: &[u64],
    metadata_csum: bool,
    truncate_to: Option<usize>,
) -> Ext4Reader {
    let allocated_blocks = allocated_blocks.to_vec();
    build_ext4_reader_with_mutation(false, None, move |image| {
        image[DATA_BLOCK_A as usize * BLOCK_SIZE..(DATA_BLOCK_A as usize + 1) * BLOCK_SIZE]
            .fill(0xA5);
        image[DATA_BLOCK_B as usize * BLOCK_SIZE..(DATA_BLOCK_B as usize + 1) * BLOCK_SIZE]
            .fill(0x5A);
        if inode_allocated {
            set_bitmap_bit(
                &mut image[4 * BLOCK_SIZE..5 * BLOCK_SIZE],
                DELETED_INODE - 1,
            );
        }
        for block in allocated_blocks {
            set_bitmap_bit(
                &mut image[3 * BLOCK_SIZE..4 * BLOCK_SIZE],
                u32::try_from(block - 1).unwrap(),
            );
        }
        if metadata_csum {
            apply_metadata_checksums(image);
        }
        if let Some(length) = truncate_to {
            image.truncate(length);
        }
    })
}

fn checksummed_content_reader_with_corrupt_bitmap() -> Ext4Reader {
    build_ext4_reader_with_mutation(false, None, |image| {
        image[DATA_BLOCK_A as usize * BLOCK_SIZE..(DATA_BLOCK_A as usize + 1) * BLOCK_SIZE]
            .fill(0xA5);
        apply_metadata_checksums(image);
        image[3 * BLOCK_SIZE + 4] ^= 0x80;
    })
}

fn set_bitmap_bit(bitmap: &mut [u8], bit: u32) {
    bitmap[(bit / 8) as usize] |= 1 << (bit % 8);
}

fn apply_metadata_checksums(image: &mut [u8]) {
    let ro_compat = 1024 + 0x64;
    image[ro_compat..ro_compat + 4]
        .copy_from_slice(&EXT4_FEATURE_RO_COMPAT_METADATA_CSUM.to_le_bytes());
    let seed = crc32c(u32::MAX, &FILESYSTEM_UUID);
    let block_bitmap_checksum = crc32c(seed, &image[3 * BLOCK_SIZE..3 * BLOCK_SIZE + 5]);
    let inode_bitmap_checksum = crc32c(seed, &image[4 * BLOCK_SIZE..4 * BLOCK_SIZE + 2]);
    let descriptor_start = 2 * BLOCK_SIZE;
    image[descriptor_start + 0x18..descriptor_start + 0x1A]
        .copy_from_slice(&(block_bitmap_checksum as u16).to_le_bytes());
    image[descriptor_start + 0x1A..descriptor_start + 0x1C]
        .copy_from_slice(&(inode_bitmap_checksum as u16).to_le_bytes());

    let descriptor = &image[descriptor_start..descriptor_start + 32];
    let mut descriptor_checksum = crc32c(seed, &0u32.to_le_bytes());
    descriptor_checksum = crc32c(descriptor_checksum, &descriptor[..0x1E]);
    descriptor_checksum = crc32c(descriptor_checksum, &[0, 0]);
    image[descriptor_start + 0x1E..descriptor_start + 0x20]
        .copy_from_slice(&(descriptor_checksum as u16).to_le_bytes());
}

fn apply_inode_checksum(inode: &mut [u8; 256]) {
    inode[0x7C..0x7E].fill(0);
    inode[0x82..0x84].fill(0);
    let seed = crc32c(u32::MAX, &FILESYSTEM_UUID);
    let mut checksum = crc32c(seed, &DELETED_INODE.to_le_bytes());
    checksum = crc32c(checksum, &inode[0x64..0x68]);
    checksum = crc32c(checksum, inode);
    inode[0x7C..0x7E].copy_from_slice(&(checksum as u16).to_le_bytes());
}
