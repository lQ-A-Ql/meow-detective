use super::wire::header_offset;
use super::*;
use crate::reader::{sb_off, XFS_SUPER_MAGIC};
use crate::XfsReader;
use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

const FS_BLOCK_SIZE: usize = 4096;
const FS_BLOCKS: usize = 40;
const LOG_START_FSB: usize = 8;
const LOG_BLOCKS: usize = 16;
const FS_UUID: [u8; 16] = [
    0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF,
];

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
                path: PathBuf::from("xfs-log-wire-fixture"),
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
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
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
fn parses_real_v2_record_and_metadata_only_inode_candidate() {
    let operations = committed_inode_transaction(0xAABB_CCDD, 99);
    let image = filesystem_with_record(0, 7, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
    assert_eq!(analysis.records.len(), 1);
    assert_eq!(
        analysis.records[0].record.header.magic,
        XLOG_HEADER_MAGIC_NUM
    );
    assert_eq!(analysis.records[0].record.header.version, 2);
    assert!(matches!(
        analysis.records[0].record.checksum_status,
        XfsLogChecksumStatus::Verified
    ));
    assert_eq!(analysis.records[0].operations.len(), 5);
    assert_eq!(
        analysis.records[0].operations[0].transaction_id, 0xAABB_CCDD,
        "the first operation word must be restored from h_cycle_data"
    );
    assert!(analysis.records[0].operations[0].flags.starts_transaction());

    let transaction = &analysis.transactions[0];
    assert!(transaction.started);
    assert!(transaction.committed);
    assert_eq!(transaction.header.as_ref().unwrap().transaction_type, 40);
    assert_eq!(transaction.header.as_ref().unwrap().item_count, 2);
    assert_eq!(transaction.item_region_count, 2);

    let candidate = &analysis.metadata_candidates[0];
    assert_eq!(candidate.item_type, XFS_LI_INODE);
    assert_eq!(candidate.kind, XfsMetadataCandidateKind::InodeUpdate);
    assert_eq!(candidate.inode, Some(99));
    assert_eq!(candidate.disk_block, Some(0x1234));
    assert!(candidate.transaction_committed);
    assert_eq!(
        candidate.completeness,
        XfsRecoveryCompleteness::MetadataOnly
    );
    assert_eq!(candidate.deletion_status, XfsDeletionStatus::NotProven);
    assert_eq!(candidate.record_log_block, 0);
    assert_eq!(candidate.operation_index, 2);
    assert_eq!(
        candidate.record_source_offset,
        (LOG_START_FSB * FS_BLOCK_SIZE) as u64
    );
    assert_eq!(
        candidate.record_checksum_status,
        XfsLogChecksumStatus::Verified
    );
    assert!(analysis.deleted_file_candidates.is_empty());
}

#[test]
fn rejects_the_old_u16_feed_synthetic_header() {
    let mut header = vec![0u8; XLOG_BASIC_BLOCK_SIZE];
    header[..2].copy_from_slice(&0xFEEDu16.to_be_bytes());

    let error = LogRecordHeader::parse(&header).unwrap_err();

    assert!(error.to_string().contains("magic"));
}

#[test]
fn empty_snapshot_is_rejected_before_parsing() {
    let image = build_filesystem_image(LOG_START_FSB as u64, LOG_BLOCKS as u32);
    let reader = open_reader(image);
    let snapshot = XfsLogSnapshot {
        geometry: reader.log_geometry().clone(),
        bytes: Vec::new(),
        complete: false,
        byte_limit: 0,
        source_offset: 0,
    };

    let error = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap_err();

    assert!(matches!(error, XfsLogError::InvalidGeometry(_)));
    assert!(error.to_string().contains("empty"));
}

#[test]
fn record_body_byte_budget_stops_collection_with_limit_issue() {
    let operations = committed_inode_transaction(0xAABB_CCDD, 99);
    let image = filesystem_with_record(0, 7, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();
    let limits = XfsLogParseLimits {
        max_body_bytes: 1,
        ..XfsLogParseLimits::default()
    };

    let analysis = analyze_log_snapshot(&snapshot, limits).unwrap();

    assert!(analysis.records.is_empty());
    assert!(analysis.issues.iter().any(|issue| {
        issue.kind == XfsLogIssueKind::LimitReached && issue.message.contains("body byte limit")
    }));
}

#[test]
fn zero_body_byte_limit_is_rejected() {
    let image = build_filesystem_image(LOG_START_FSB as u64, LOG_BLOCKS as u32);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();
    let limits = XfsLogParseLimits {
        max_body_bytes: 0,
        ..XfsLogParseLimits::default()
    };

    let error = analyze_log_snapshot(&snapshot, limits).unwrap_err();

    assert!(matches!(error, XfsLogError::InvalidGeometry(_)));
}

#[test]
fn bounded_snapshot_reports_a_truncated_record_without_guessing() {
    let image = filesystem_with_record(0, 3, 32 * 1024, &committed_inode_transaction(7, 42));
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XLOG_BASIC_BLOCK_SIZE)
        .unwrap();

    assert!(!snapshot.complete);
    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();
    assert!(analysis.records.is_empty());
    assert!(analysis
        .issues
        .iter()
        .any(|issue| issue.kind == XfsLogIssueKind::TruncatedRecord));
}

#[test]
fn rejects_a_record_body_with_the_wrong_cycle_stamp() {
    let mut image = filesystem_with_record(0, 11, 32 * 1024, &committed_inode_transaction(8, 43));
    let log_offset = LOG_START_FSB * FS_BLOCK_SIZE;
    image[log_offset + XLOG_BASIC_BLOCK_SIZE..log_offset + XLOG_BASIC_BLOCK_SIZE + 4]
        .copy_from_slice(&12u32.to_be_bytes());
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.records.is_empty());
    assert!(analysis
        .issues
        .iter()
        .any(|issue| issue.kind == XfsLogIssueKind::CycleMismatch));
}

#[test]
fn rejects_a_crc32c_mismatch_even_when_cycle_stamps_are_valid() {
    let mut image = filesystem_with_record(0, 12, 32 * 1024, &committed_inode_transaction(18, 51));
    let log_offset = LOG_START_FSB * FS_BLOCK_SIZE;
    image[log_offset + XLOG_BASIC_BLOCK_SIZE + 24] ^= 0x80;
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.records.is_empty());
    assert!(analysis
        .issues
        .iter()
        .any(|issue| issue.kind == XfsLogIssueKind::ChecksumMismatch));
}

#[test]
fn crc32c_uses_the_castagnoli_wire_polynomial() {
    let checksum = !super::checksum::crc32c(u32::MAX, b"123456789");

    assert_eq!(checksum, 0xE306_9283);
}

#[test]
fn parses_a_record_that_wraps_at_the_physical_end_of_the_log() {
    let total_log_blocks = LOG_BLOCKS * FS_BLOCK_SIZE / XLOG_BASIC_BLOCK_SIZE;
    let start_block = total_log_blocks - 1;
    let image = filesystem_with_record(
        start_block,
        17,
        32 * 1024,
        &committed_inode_transaction(9, 44),
    );
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
    assert_eq!(analysis.records[0].record.log_block as usize, start_block);
    assert_eq!(
        analysis.records[0].record.source_offset,
        (LOG_START_FSB * FS_BLOCK_SIZE + start_block * XLOG_BASIC_BLOCK_SIZE) as u64
    );
    assert_eq!(analysis.records[0].operations[0].transaction_id, 9);
    let provenance = analysis.records[0].record.provenance;
    assert_eq!(provenance.spans().count(), 2);
    assert_eq!(
        provenance.spans().map(|span| span.length).sum::<u64>(),
        analysis.records[0].record.header.header_blocks() as u64 * XLOG_BASIC_BLOCK_SIZE as u64
            + analysis.records[0].record.header.data_blocks() as u64 * XLOG_BASIC_BLOCK_SIZE as u64
    );
}

#[test]
fn accepts_bounded_v2_extended_header_geometry() {
    let image = filesystem_with_record(0, 23, 64 * 1024, &committed_inode_transaction(10, 45));
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
    assert_eq!(analysis.records[0].record.header.header_blocks(), 2);
}

#[test]
fn reassembles_a_real_continued_operation_region_before_classification() {
    let descriptor = inode_descriptor(0xCAFE);
    let operations = vec![
        operation(0x55, XLOG_START_TRANS, &[]),
        operation(0x55, 0, &transaction_header(2)),
        operation(0x55, XLOG_CONTINUE_TRANS, &descriptor[..20]),
        operation(
            0x55,
            XLOG_WAS_CONT_TRANS | XLOG_END_TRANS,
            &descriptor[20..],
        ),
        operation(0x55, 0, &logged_inode_core_v3(0xCAFE, 1)),
        operation(0x55, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 27, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
    assert_eq!(analysis.metadata_candidates.len(), 1);
    assert_eq!(analysis.metadata_candidates[0].inode, Some(0xCAFE));
    assert!(analysis.metadata_candidates[0].transaction_committed);
}

#[test]
fn committed_complete_inode_item_with_zero_nlink_is_a_deleted_file_candidate() {
    let inode = 0x1234_5678;
    let image = filesystem_with_record(
        0,
        41,
        32 * 1024,
        &committed_inode_transaction_with_nlink(0x81, inode, 0),
    );
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.issues.is_empty());
    assert_eq!(analysis.deleted_file_candidates.len(), 1);
    let candidate = &analysis.deleted_file_candidates[0];
    assert_eq!(candidate.inode, inode);
    assert_eq!(candidate.proof, XfsDeletionProof::InodeCoreNlinkZero);
    assert_eq!(
        candidate.completeness,
        XfsRecoveryCompleteness::MetadataOnly
    );
    assert_eq!(candidate.operation_index, 2);
    assert_eq!(candidate.record_log_block, 0);
}

#[test]
fn checkpoint_transaction_counts_complete_item_regions() {
    let inode = 0x1234_5679;
    let mut iunlink_descriptor = Vec::with_capacity(4);
    iunlink_descriptor.extend_from_slice(&XFS_LI_IUNLINK.to_le_bytes());
    iunlink_descriptor.extend_from_slice(&1u16.to_le_bytes());
    let operations = vec![
        operation(0x8B, XLOG_START_TRANS, &[]),
        operation(0x8B, 0, &transaction_header(3)),
        operation(0x8B, 0, &inode_descriptor(inode)),
        operation(0x8B, 0, &logged_inode_core_v3(inode, 0)),
        operation(0x8B, 0, &iunlink_descriptor),
        operation(0x8B, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 52, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.issues.is_empty());
    assert_eq!(analysis.metadata_candidates.len(), 2);
    assert_eq!(analysis.deleted_file_candidates.len(), 1);
    assert_eq!(analysis.deleted_file_candidates[0].inode, inode);
    assert_eq!(
        analysis.transactions[0].header.as_ref().unwrap().item_count,
        3
    );
    assert_eq!(analysis.transactions[0].item_region_count, 3);
}

#[test]
fn committed_inode_item_with_nonzero_nlink_is_not_a_deleted_file() {
    let image = filesystem_with_record(
        0,
        42,
        32 * 1024,
        &committed_inode_transaction_with_nlink(0x82, 0x200, 2),
    );
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
}

#[test]
fn uncommitted_zero_nlink_inode_item_is_not_a_deleted_file() {
    let inode = 0x201;
    let operations = vec![
        operation(0x83, XLOG_START_TRANS, &[]),
        operation(0x83, 0, &transaction_header(2)),
        operation(0x83, 0, &inode_descriptor(inode)),
        operation(0x83, 0, &logged_inode_core_v3(inode, 0)),
    ];
    let image = filesystem_with_record(0, 43, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
    assert!(!analysis.transactions[0].committed);
}

#[test]
fn committed_inode_descriptor_without_complete_core_is_invalid_not_deleted() {
    let operations = vec![
        operation(0x84, XLOG_START_TRANS, &[]),
        operation(0x84, 0, &transaction_header(2)),
        operation(0x84, 0, &inode_descriptor(0x202)),
        operation(0x84, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 44, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.deleted_file_candidates.is_empty());
    assert!(analysis
        .issues
        .iter()
        .any(|issue| issue.kind == XfsLogIssueKind::InvalidOperation
            && issue.message.contains("incomplete log item")));
}

#[test]
fn truncated_logged_inode_core_is_invalid_not_deleted() {
    let mut core = logged_inode_core_v3(0x203, 0);
    core.truncate(core.len() - 1);
    assert_invalid_inode_core_transaction(0x85, 45, 0x203, core, "has 175 bytes");
}

#[test]
fn logged_inode_core_with_bad_magic_is_invalid_not_deleted() {
    let mut core = logged_inode_core_v3(0x204, 0);
    core[0..2].copy_from_slice(&0u16.to_le_bytes());
    assert_invalid_inode_core_transaction(0x86, 46, 0x204, core, "invalid XFS inode magic");
}

#[test]
fn v3_logged_inode_identity_must_match_the_descriptor() {
    let core = logged_inode_core_v3(0x999, 0);
    assert_invalid_inode_core_transaction(0x87, 47, 0x205, core, "does not match");
}

#[test]
fn complete_v2_logged_inode_core_can_prove_zero_nlink() {
    let inode = 0x206;
    let operations = vec![
        operation(0x88, XLOG_START_TRANS, &[]),
        operation(0x88, 0, &transaction_header(2)),
        operation(0x88, 0, &inode_descriptor_32(inode)),
        operation(0x88, 0, &logged_inode_core_v2(0)),
        operation(0x88, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 48, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.issues.is_empty());
    assert_eq!(analysis.deleted_file_candidates[0].inode, inode);
}

#[test]
fn v1_logged_inode_uses_historical_di_onlink_not_di_nlink() {
    let inode = 0x209;
    let operations = vec![
        operation(0x8C, XLOG_START_TRANS, &[]),
        operation(0x8C, 0, &transaction_header(2)),
        operation(0x8C, 0, &inode_descriptor_32(inode)),
        operation(0x8C, 0, &logged_inode_core_v1(1, 0)),
        operation(0x8C, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 53, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
}

#[test]
fn v1_logged_inode_zero_di_onlink_can_prove_deletion() {
    let inode = 0x20A;
    let operations = vec![
        operation(0x8D, XLOG_START_TRANS, &[]),
        operation(0x8D, 0, &transaction_header(2)),
        operation(0x8D, 0, &inode_descriptor_32(inode)),
        operation(0x8D, 0, &logged_inode_core_v1(0, 7)),
        operation(0x8D, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 54, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.issues.is_empty());
    assert_eq!(analysis.deleted_file_candidates.len(), 1);
    assert_eq!(analysis.deleted_file_candidates[0].inode, inode);
}

#[test]
fn iunlink_metadata_without_inode_and_nlink_proof_is_not_a_deleted_file() {
    let mut descriptor = Vec::with_capacity(4);
    descriptor.extend_from_slice(&XFS_LI_IUNLINK.to_le_bytes());
    descriptor.extend_from_slice(&1u16.to_le_bytes());
    let operations = vec![
        operation(0x66, XLOG_START_TRANS, &[]),
        operation(0x66, 0, &transaction_header(1)),
        operation(0x66, 0, &descriptor),
        operation(0x66, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 28, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
    assert_eq!(analysis.metadata_candidates.len(), 1);
    assert_eq!(
        analysis.metadata_candidates[0].kind,
        XfsMetadataCandidateKind::UnlinkedInodeUpdate
    );
    assert_eq!(analysis.metadata_candidates[0].inode, None);
    assert_eq!(
        analysis.metadata_candidates[0].deletion_status,
        XfsDeletionStatus::NotProven
    );
    assert!(analysis.deleted_file_candidates.is_empty());
}

#[test]
fn iunlink_payload_that_looks_like_an_inode_core_is_never_promoted() {
    let inode_like_payload = logged_inode_core_v3(0xDEAD_BEEF, 0);
    let mut descriptor = Vec::with_capacity(4);
    descriptor.extend_from_slice(&XFS_LI_IUNLINK.to_le_bytes());
    descriptor.extend_from_slice(&2u16.to_le_bytes());
    let operations = vec![
        operation(0x67, XLOG_START_TRANS, &[]),
        operation(0x67, 0, &transaction_header(2)),
        operation(0x67, 0, &descriptor),
        operation(0x67, 0, &inode_like_payload),
        operation(0x67, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 49, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_only_deletion_boundary(&analysis);
    assert_eq!(
        analysis.metadata_candidates[0].kind,
        XfsMetadataCandidateKind::UnlinkedInodeUpdate
    );
    assert_eq!(analysis.metadata_candidates[0].inode, None);
}

#[test]
fn declared_log_item_count_mismatch_prevents_promotion() {
    let inode = 0x207;
    let operations = vec![
        operation(0x89, XLOG_START_TRANS, &[]),
        operation(0x89, 0, &transaction_header(1)),
        operation(0x89, 0, &inode_descriptor(inode)),
        operation(0x89, 0, &logged_inode_core_v3(inode, 0)),
        operation(0x89, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 50, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.deleted_file_candidates.is_empty());
    assert!(analysis.issues.iter().any(|issue| {
        issue.kind == XfsLogIssueKind::InvalidOperation
            && issue.message.contains("declares 1 item regions")
    }));
}

#[test]
fn non_checkpoint_transaction_type_is_not_promoted_without_verified_count_semantics() {
    let inode = 0x20b;
    let mut header = transaction_header(2);
    header[4..8].copy_from_slice(&1u32.to_le_bytes());
    let operations = vec![
        operation(0x8e, XLOG_START_TRANS, &[]),
        operation(0x8e, 0, &header),
        operation(0x8e, 0, &inode_descriptor(inode)),
        operation(0x8e, 0, &logged_inode_core_v3(inode, 0)),
        operation(0x8e, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, 55, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.deleted_file_candidates.is_empty());
    assert!(!analysis.metadata_candidates[0].transaction_committed);
    assert!(analysis.issues.iter().any(|issue| {
        issue.kind == XfsLogIssueKind::InvalidOperation
            && issue.message.contains("unverified th_num_items semantics")
    }));
}

#[test]
fn unknown_native_byte_order_cannot_decode_inode_deletion_evidence() {
    let mut image = filesystem_with_record(
        0,
        51,
        32 * 1024,
        &committed_inode_transaction_with_nlink(0x8A, 0x208, 0),
    );
    let log_offset = LOG_START_FSB * FS_BLOCK_SIZE;
    image[log_offset + header_offset::FORMAT..log_offset + header_offset::FORMAT + 4]
        .copy_from_slice(&0u32.to_be_bytes());
    rewrite_record_checksum(&mut image, log_offset);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.deleted_file_candidates.is_empty());
    assert!(analysis
        .issues
        .iter()
        .any(|issue| issue.kind == XfsLogIssueKind::InvalidOperation));
}

#[test]
fn rejects_v2_iclog_geometry_above_the_kernel_maximum() {
    let record = build_record(
        LOG_BLOCKS * FS_BLOCK_SIZE / XLOG_BASIC_BLOCK_SIZE,
        0,
        5,
        32 * 1024,
        &committed_inode_transaction(11, 46),
    );
    let mut header = record[..XLOG_BASIC_BLOCK_SIZE].to_vec();
    header[header_offset::ICLOG_SIZE..header_offset::ICLOG_SIZE + 4]
        .copy_from_slice(&(512u32 * 1024).to_be_bytes());

    let error = LogRecordHeader::parse(&header).unwrap_err();

    assert!(error.to_string().contains("iclog size"));
}

#[test]
fn reports_an_operation_region_overrun() {
    let mut invalid_operation = vec![0u8; XLOG_OP_HEADER_SIZE];
    invalid_operation[0..4].copy_from_slice(&12u32.to_be_bytes());
    invalid_operation[4..8].copy_from_slice(&1024u32.to_be_bytes());
    invalid_operation[8] = XFS_TRANSACTION_CLIENT;
    let image = filesystem_with_record(0, 29, 32 * 1024, &[invalid_operation]);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert_eq!(analysis.records.len(), 1);
    assert!(analysis.records[0].operations.is_empty());
    assert!(analysis
        .issues
        .iter()
        .any(|issue| issue.kind == XfsLogIssueKind::InvalidOperation));
}

#[test]
fn internal_log_snapshot_uses_superblock_geometry_and_honors_the_bound() {
    let image = filesystem_with_record(0, 31, 32 * 1024, &committed_inode_transaction(13, 48));
    let reader = open_reader(image);

    assert_eq!(
        reader.log_geometry().location,
        XfsLogLocation::Internal {
            start_fsb: LOG_START_FSB as u64
        }
    );
    assert_eq!(reader.log_geometry().log_blocks, LOG_BLOCKS as u32);
    let snapshot = reader.read_internal_log_snapshot(1024).unwrap();
    assert_eq!(snapshot.bytes.len(), 1024);
    assert_eq!(snapshot.byte_limit, 1024);
    assert!(!snapshot.complete);
}

#[test]
fn external_log_returns_a_typed_unsupported_issue() {
    let image = build_filesystem_image(0, LOG_BLOCKS as u32);
    let reader = open_reader(image);

    let error = reader.read_internal_log_snapshot(4096).unwrap_err();

    match error {
        XfsLogError::Unsupported(issue) => {
            assert_eq!(issue.kind, XfsLogIssueKind::ExternalLogUnsupported)
        }
        other => panic!("expected typed unsupported error, got {other:?}"),
    }
}

#[test]
fn rejects_internal_log_geometry_that_exceeds_the_data_device() {
    let image = build_filesystem_image((FS_BLOCKS - 2) as u64, LOG_BLOCKS as u32);
    let reader = open_reader(image);

    let error = reader.read_internal_log_snapshot(4096).unwrap_err();

    assert!(matches!(error, XfsLogError::InvalidGeometry(_)));
}

#[test]
fn rejects_a_record_uuid_from_another_filesystem() {
    let mut image = filesystem_with_record(0, 37, 32 * 1024, &committed_inode_transaction(14, 49));
    let offset = LOG_START_FSB * FS_BLOCK_SIZE + header_offset::FS_UUID;
    image[offset] ^= 0xFF;
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.records.is_empty());
    assert!(analysis
        .issues
        .iter()
        .any(|issue| issue.kind == XfsLogIssueKind::InvalidRecord));
}

fn open_reader(image: Vec<u8>) -> XfsReader {
    XfsReader::open(Box::new(MemoryReader::new(image)), 0).unwrap()
}

fn assert_only_deletion_boundary(analysis: &XfsLogAnalysis) {
    assert_eq!(analysis.issues.len(), 1);
    assert_eq!(
        analysis.issues[0].kind,
        XfsLogIssueKind::DeletionEvidenceUnavailable
    );
    assert!(analysis.deleted_file_candidates.is_empty());
}

fn committed_inode_transaction(transaction_id: u32, inode: u64) -> Vec<Vec<u8>> {
    committed_inode_transaction_with_nlink(transaction_id, inode, 1)
}

fn committed_inode_transaction_with_nlink(
    transaction_id: u32,
    inode: u64,
    link_count: u32,
) -> Vec<Vec<u8>> {
    vec![
        operation(transaction_id, XLOG_START_TRANS, &[]),
        operation(transaction_id, 0, &transaction_header(2)),
        operation(transaction_id, 0, &inode_descriptor(inode)),
        operation(transaction_id, 0, &logged_inode_core_v3(inode, link_count)),
        operation(transaction_id, XLOG_COMMIT_TRANS, &[]),
    ]
}

fn assert_invalid_inode_core_transaction(
    transaction_id: u32,
    cycle: u32,
    descriptor_inode: u64,
    core: Vec<u8>,
    expected_message: &str,
) {
    let operations = vec![
        operation(transaction_id, XLOG_START_TRANS, &[]),
        operation(transaction_id, 0, &transaction_header(2)),
        operation(transaction_id, 0, &inode_descriptor(descriptor_inode)),
        operation(transaction_id, 0, &core),
        operation(transaction_id, XLOG_COMMIT_TRANS, &[]),
    ];
    let image = filesystem_with_record(0, cycle, 32 * 1024, &operations);
    let reader = open_reader(image);
    let snapshot = reader
        .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();

    let analysis = analyze_log_snapshot(&snapshot, XfsLogParseLimits::default()).unwrap();

    assert!(analysis.deleted_file_candidates.is_empty());
    assert!(analysis.issues.iter().any(|issue| {
        issue.kind == XfsLogIssueKind::InvalidOperation && issue.message.contains(expected_message)
    }));
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

fn transaction_header(item_count: u32) -> Vec<u8> {
    let mut header = Vec::with_capacity(16);
    header.extend_from_slice(&0x5452_414Eu32.to_le_bytes());
    header.extend_from_slice(&40u32.to_le_bytes());
    header.extend_from_slice(&0i32.to_le_bytes());
    header.extend_from_slice(&item_count.to_le_bytes());
    header
}

fn inode_descriptor(inode: u64) -> Vec<u8> {
    let mut descriptor = vec![0u8; 56];
    descriptor[0..2].copy_from_slice(&XFS_LI_INODE.to_le_bytes());
    descriptor[2..4].copy_from_slice(&2u16.to_le_bytes());
    descriptor[4..8].copy_from_slice(&1u32.to_le_bytes());
    descriptor[16..24].copy_from_slice(&inode.to_le_bytes());
    descriptor[40..48].copy_from_slice(&0x1234i64.to_le_bytes());
    descriptor[48..52].copy_from_slice(&1i32.to_le_bytes());
    descriptor[52..56].copy_from_slice(&0i32.to_le_bytes());
    descriptor
}

fn inode_descriptor_32(inode: u64) -> Vec<u8> {
    let mut descriptor = vec![0u8; 52];
    descriptor[0..2].copy_from_slice(&XFS_LI_INODE.to_le_bytes());
    descriptor[2..4].copy_from_slice(&2u16.to_le_bytes());
    descriptor[4..8].copy_from_slice(&1u32.to_le_bytes());
    descriptor[12..20].copy_from_slice(&inode.to_le_bytes());
    descriptor[36..44].copy_from_slice(&0x1234i64.to_le_bytes());
    descriptor[44..48].copy_from_slice(&1i32.to_le_bytes());
    descriptor[48..52].copy_from_slice(&0i32.to_le_bytes());
    descriptor
}

fn logged_inode_core_v2(link_count: u32) -> Vec<u8> {
    let mut core = vec![0u8; 96];
    core[0..2].copy_from_slice(&0x494Eu16.to_le_bytes());
    core[2..4].copy_from_slice(&0x81A4u16.to_le_bytes());
    core[4] = 2;
    core[5] = 2;
    core[16..20].copy_from_slice(&link_count.to_le_bytes());
    core
}

fn logged_inode_core_v1(onlink: u16, ignored_nlink: u32) -> Vec<u8> {
    let mut core = vec![0u8; 96];
    core[0..2].copy_from_slice(&0x494Eu16.to_le_bytes());
    core[2..4].copy_from_slice(&0x81A4u16.to_le_bytes());
    core[4] = 1;
    core[5] = 2;
    core[6..8].copy_from_slice(&onlink.to_le_bytes());
    core[16..20].copy_from_slice(&ignored_nlink.to_le_bytes());
    core
}

fn logged_inode_core_v3(inode: u64, link_count: u32) -> Vec<u8> {
    let mut core = vec![0u8; 176];
    core[0..2].copy_from_slice(&0x494Eu16.to_le_bytes());
    core[2..4].copy_from_slice(&0x81A4u16.to_le_bytes());
    core[4] = 3;
    core[5] = 2;
    core[16..20].copy_from_slice(&link_count.to_le_bytes());
    core[152..160].copy_from_slice(&inode.to_le_bytes());
    core
}

fn filesystem_with_record(
    start_log_block: usize,
    cycle: u32,
    iclog_size: usize,
    operations: &[Vec<u8>],
) -> Vec<u8> {
    let mut image = build_filesystem_image(LOG_START_FSB as u64, LOG_BLOCKS as u32);
    let total_log_blocks = LOG_BLOCKS * FS_BLOCK_SIZE / XLOG_BASIC_BLOCK_SIZE;
    let record = build_record(
        total_log_blocks,
        start_log_block,
        cycle,
        iclog_size,
        operations,
    );
    let log =
        &mut image[LOG_START_FSB * FS_BLOCK_SIZE..(LOG_START_FSB + LOG_BLOCKS) * FS_BLOCK_SIZE];
    write_circular(log, start_log_block * XLOG_BASIC_BLOCK_SIZE, &record);
    image
}

fn build_filesystem_image(log_start: u64, log_blocks: u32) -> Vec<u8> {
    let mut image = vec![0u8; FS_BLOCKS * FS_BLOCK_SIZE];
    let superblock = &mut image[..512];
    superblock[sb_off::MAGIC..sb_off::MAGIC + 4].copy_from_slice(&XFS_SUPER_MAGIC.to_be_bytes());
    superblock[sb_off::BLOCKSIZE..sb_off::BLOCKSIZE + 4]
        .copy_from_slice(&(FS_BLOCK_SIZE as u32).to_be_bytes());
    superblock[sb_off::DBLOCKS..sb_off::DBLOCKS + 8]
        .copy_from_slice(&(FS_BLOCKS as u64).to_be_bytes());
    superblock[sb_off::UUID..sb_off::UUID + 16].copy_from_slice(&FS_UUID);
    superblock[sb_off::LOGSTART..sb_off::LOGSTART + 8].copy_from_slice(&log_start.to_be_bytes());
    superblock[sb_off::ROOTINO..sb_off::ROOTINO + 8].copy_from_slice(&2u64.to_be_bytes());
    superblock[sb_off::AGBLOCKS..sb_off::AGBLOCKS + 4]
        .copy_from_slice(&(FS_BLOCKS as u32).to_be_bytes());
    superblock[sb_off::AGCOUNT..sb_off::AGCOUNT + 4].copy_from_slice(&1u32.to_be_bytes());
    superblock[sb_off::LOGBLOCKS..sb_off::LOGBLOCKS + 4].copy_from_slice(&log_blocks.to_be_bytes());
    superblock[sb_off::VERSIONNUM..sb_off::VERSIONNUM + 2].copy_from_slice(&5u16.to_be_bytes());
    superblock[sb_off::SECTSIZE..sb_off::SECTSIZE + 2]
        .copy_from_slice(&(XLOG_BASIC_BLOCK_SIZE as u16).to_be_bytes());
    superblock[sb_off::INODESIZE..sb_off::INODESIZE + 2].copy_from_slice(&256u16.to_be_bytes());
    superblock[sb_off::INOPBLOCK..sb_off::INOPBLOCK + 2].copy_from_slice(&16u16.to_be_bytes());
    superblock[sb_off::LOGSECTSIZE..sb_off::LOGSECTSIZE + 2]
        .copy_from_slice(&(XLOG_BASIC_BLOCK_SIZE as u16).to_be_bytes());
    image
}

fn build_record(
    total_log_blocks: usize,
    start_log_block: usize,
    cycle: u32,
    iclog_size: usize,
    operations: &[Vec<u8>],
) -> Vec<u8> {
    let header_blocks = iclog_size.div_ceil(XLOG_HEADER_CYCLE_SIZE);
    let mut header = vec![0u8; header_blocks * XLOG_BASIC_BLOCK_SIZE];
    let mut body = operations.iter().flatten().copied().collect::<Vec<_>>();
    body.resize(
        body.len().div_ceil(XLOG_BASIC_BLOCK_SIZE) * XLOG_BASIC_BLOCK_SIZE,
        0,
    );

    header[header_offset::MAGIC..header_offset::MAGIC + 4]
        .copy_from_slice(&XLOG_HEADER_MAGIC_NUM.to_be_bytes());
    header[header_offset::CYCLE..header_offset::CYCLE + 4].copy_from_slice(&cycle.to_be_bytes());
    header[header_offset::VERSION..header_offset::VERSION + 4].copy_from_slice(&2u32.to_be_bytes());
    header[header_offset::DATA_LEN..header_offset::DATA_LEN + 4]
        .copy_from_slice(&(body.len() as u32).to_be_bytes());
    let lsn = (u64::from(cycle) << 32) | start_log_block as u64;
    header[header_offset::LSN..header_offset::LSN + 8].copy_from_slice(&lsn.to_be_bytes());
    header[header_offset::TAIL_LSN..header_offset::TAIL_LSN + 8]
        .copy_from_slice(&lsn.to_be_bytes());
    header[header_offset::PREV_BLOCK..header_offset::PREV_BLOCK + 4]
        .copy_from_slice(&u32::MAX.to_be_bytes());
    header[header_offset::NUM_LOGOPS..header_offset::NUM_LOGOPS + 4]
        .copy_from_slice(&(operations.len() as u32).to_be_bytes());
    header[header_offset::FORMAT..header_offset::FORMAT + 4].copy_from_slice(&1u32.to_be_bytes());
    header[header_offset::FS_UUID..header_offset::FS_UUID + 16].copy_from_slice(&FS_UUID);
    header[header_offset::ICLOG_SIZE..header_offset::ICLOG_SIZE + 4]
        .copy_from_slice(&(iclog_size as u32).to_be_bytes());

    for extension in 0..header_blocks.saturating_sub(1) {
        let absolute_block = start_log_block + extension + 1;
        let extension_cycle = wrapped_cycle(cycle, absolute_block, total_log_blocks);
        let offset = (extension + 1) * XLOG_BASIC_BLOCK_SIZE;
        header[offset..offset + 4].copy_from_slice(&extension_cycle.to_be_bytes());
    }
    for body_block in 0..body.len() / XLOG_BASIC_BLOCK_SIZE {
        let body_offset = body_block * XLOG_BASIC_BLOCK_SIZE;
        let original = u32::from_be_bytes(body[body_offset..body_offset + 4].try_into().unwrap());
        write_saved_cycle_word(&mut header, body_block, original);
        let absolute_block = start_log_block + header_blocks + body_block;
        let stamped_cycle = wrapped_cycle(cycle, absolute_block, total_log_blocks);
        body[body_offset..body_offset + 4].copy_from_slice(&stamped_cycle.to_be_bytes());
    }

    let checksum = super::checksum::xlog_checksum(&header, &body, 328).unwrap();
    header[header_offset::CRC..header_offset::CRC + 4].copy_from_slice(&checksum.to_le_bytes());

    header.extend_from_slice(&body);
    header
}

fn rewrite_record_checksum(image: &mut [u8], record_offset: usize) {
    let iclog_size = u32::from_be_bytes(
        image[record_offset + header_offset::ICLOG_SIZE
            ..record_offset + header_offset::ICLOG_SIZE + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let header_len = iclog_size.div_ceil(XLOG_HEADER_CYCLE_SIZE) * XLOG_BASIC_BLOCK_SIZE;
    let body_len = u32::from_be_bytes(
        image[record_offset + header_offset::DATA_LEN..record_offset + header_offset::DATA_LEN + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let body_offset = record_offset + header_len;
    let checksum = super::checksum::xlog_checksum(
        &image[record_offset..body_offset],
        &image[body_offset..body_offset + body_len],
        328,
    )
    .unwrap();
    image[record_offset + header_offset::CRC..record_offset + header_offset::CRC + 4]
        .copy_from_slice(&checksum.to_le_bytes());
}

fn write_saved_cycle_word(header: &mut [u8], body_block: usize, value: u32) {
    let words_per_header = XLOG_HEADER_CYCLE_SIZE / XLOG_BASIC_BLOCK_SIZE;
    let (header_index, base, word_index) = if body_block < words_per_header {
        (0, header_offset::CYCLE_DATA, body_block)
    } else {
        (
            body_block / words_per_header,
            4,
            body_block % words_per_header,
        )
    };
    let offset = header_index * XLOG_BASIC_BLOCK_SIZE + base + word_index * 4;
    header[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn wrapped_cycle(cycle: u32, absolute_block: usize, total_log_blocks: usize) -> u32 {
    let mut value = cycle.wrapping_add((absolute_block / total_log_blocks) as u32);
    if value == XLOG_HEADER_MAGIC_NUM {
        value = value.wrapping_add(1);
    }
    value
}

fn write_circular(target: &mut [u8], start: usize, bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        target[(start + index) % target.len()] = *byte;
    }
}
