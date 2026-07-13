fn log_fnode() -> ceph_wire::BluefsFnode {
    ceph_wire::BluefsFnode {
        ino: 1,
        size: 0,
        mtime: ceph_wire::CephUtime {
            seconds: 0,
            nanoseconds: 0,
        },
        extents: vec![ceph_wire::BluefsExtent {
            offset: 0x100000,
            length: 0x10000,
            bdev: 1,
            struct_version: 1,
            struct_compat_version: 1,
        }],
        encoding: 0,
        content_size: 0,
        struct_version: 2,
        struct_compat_version: 1,
    }
}

#[test]
fn jump_sequence_matches_ceph_log_completion_semantics() {
    let mut state = super::ReplayState::new(log_fnode());

    state.validate_sequence(1).unwrap();
    let first = state
        .apply_transaction(
            &[
                ceph_wire::BluefsOperation::Init,
                ceph_wire::BluefsOperation::Jump {
                    next_sequence: 1,
                    offset: 0x10000,
                },
            ],
            1,
        )
        .unwrap();
    assert_eq!(first.completion_sequence, 1);
    assert_eq!(first.jump_offset, Some(0x10000));
    state.final_sequence = first.completion_sequence;

    state.validate_sequence(2).unwrap();
    let second = state
        .apply_transaction(
            &[ceph_wire::BluefsOperation::Jump {
                next_sequence: 186_888,
                offset: 0x20000,
            }],
            2,
        )
        .unwrap();
    assert_eq!(second.completion_sequence, 186_888);
    state.final_sequence = second.completion_sequence;

    state.validate_sequence(186_889).unwrap();
    let third = state.apply_transaction(&[], 186_889).unwrap();
    state.final_sequence = third.completion_sequence;
    state.validate_sequence(186_890).unwrap();
}

#[test]
fn jump_sequence_must_advance_from_prior_log_state() {
    let mut state = super::ReplayState::new(log_fnode());
    state.final_sequence = 10;

    assert!(state
        .apply_transaction(
            &[ceph_wire::BluefsOperation::JumpSequence { next_sequence: 10 }],
            11,
        )
        .is_err());
    assert_eq!(
        state
            .apply_transaction(
                &[ceph_wire::BluefsOperation::JumpSequence { next_sequence: 11 }],
                11,
            )
            .unwrap()
            .completion_sequence,
        11
    );
}

#[test]
fn replay_records_truncated_multiblock_tail_after_valid_transaction() {
    let uuid = uuid::Uuid::new_v4();
    let mut bytes = vec![0u8; 8192];
    let first = encode_transaction(uuid, 1, &[1]);
    bytes[..first.len()].copy_from_slice(&first);
    let tail = truncated_transaction_prefix(uuid, 2, 5000);
    bytes[4096..4096 + tail.len()].copy_from_slice(&tail);
    let mut reader = VecEvidenceReader::new(bytes);
    let superblock = superblock(uuid, 8192);

    let replay = super::replay_bluefs_log(&mut reader, &superblock).unwrap();

    assert_eq!(replay.transaction_count, 1);
    assert_eq!(replay.final_sequence, 1);
    assert_eq!(replay.logical_bytes, 4096);
    assert_eq!(replay.stop_reason, "truncatedTail");
}

#[test]
fn replay_records_corrupt_tail_after_valid_transaction() {
    let uuid = uuid::Uuid::new_v4();
    let mut bytes = vec![0u8; 8192];
    let first = encode_transaction(uuid, 1, &[1]);
    bytes[..first.len()].copy_from_slice(&first);
    let mut tail = encode_transaction(uuid, 2, &[1]);
    let last = tail.len() - 1;
    tail[last] ^= 1;
    bytes[4096..4096 + tail.len()].copy_from_slice(&tail);
    let mut reader = VecEvidenceReader::new(bytes);
    let superblock = superblock(uuid, 8192);

    let replay = super::replay_bluefs_log(&mut reader, &superblock).unwrap();

    assert_eq!(replay.transaction_count, 1);
    assert_eq!(replay.final_sequence, 1);
    assert_eq!(replay.logical_bytes, 4096);
    assert_eq!(replay.stop_reason, "invalidTail");
}

#[test]
fn replay_rejects_oversized_block_size_before_reading() {
    let uuid = uuid::Uuid::new_v4();
    let mut superblock = superblock(uuid, 8192);
    superblock.block_size = 2 * 1024 * 1024;

    assert!(super::validate_replay_superblock(&superblock).is_err());
}

fn encode_transaction(uuid: uuid::Uuid, sequence: u64, operations: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    uuid.encode(&mut payload);
    sequence.encode(&mut payload);
    (operations.len() as u32).encode(&mut payload);
    payload.extend_from_slice(operations);
    ceph_wire::crc32c::ceph_crc32c(operations).encode(&mut payload);
    let mut encoded = Vec::new();
    CephStructEnvelope {
        version: 1,
        compat_version: 1,
        payload_length: payload.len() as u32,
    }
    .encode(&mut encoded);
    encoded.extend_from_slice(&payload);
    encoded
}

fn truncated_transaction_prefix(uuid: uuid::Uuid, sequence: u64, payload_length: u32) -> Vec<u8> {
    let mut encoded = Vec::new();
    CephStructEnvelope {
        version: 1,
        compat_version: 1,
        payload_length,
    }
    .encode(&mut encoded);
    uuid.encode(&mut encoded);
    sequence.encode(&mut encoded);
    (payload_length - 32).encode(&mut encoded);
    encoded
}

fn superblock(uuid: uuid::Uuid, extent_length: u32) -> ceph_wire::BluefsSuper {
    let mut log = log_fnode();
    log.extents[0].offset = 0;
    log.extents[0].length = extent_length;
    ceph_wire::BluefsSuper {
        uuid,
        osd_uuid: uuid::Uuid::new_v4(),
        seq: 1,
        block_size: 4096,
        log_fnode: log,
        memorized_layout: None,
        crc32c: 0,
        struct_version: 2,
        struct_compat_version: 1,
    }
}

struct VecEvidenceReader {
    inner: std::io::Cursor<Vec<u8>>,
    info: evidence_core::ReaderInfo,
}

impl VecEvidenceReader {
    fn new(bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        Self {
            inner: std::io::Cursor::new(bytes),
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("bluefs-test"),
                size,
                kind: "test".to_string(),
            },
        }
    }
}

impl Read for VecEvidenceReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl Seek for VecEvidenceReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

impl evidence_core::EvidenceReader for VecEvidenceReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}
use std::io::{Read, Seek, SeekFrom};

use ceph_wire::{CephEncode, CephStructEnvelope};
