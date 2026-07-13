use std::io::{Read, Seek, SeekFrom};

use ceph_wire::{BluefsExtent, BluefsFnode, CephUtime};

#[test]
fn current_accepts_a_single_manifest_line() {
    assert_eq!(
        super::parse_current(b"MANIFEST-000143\n").unwrap(),
        ("MANIFEST-000143".to_string(), 143)
    );
    assert_eq!(
        super::parse_current(b"MANIFEST-1\n").unwrap(),
        ("MANIFEST-1".to_string(), 1)
    );
}

#[test]
fn current_rejects_non_manifest_paths_and_line_variants() {
    for bytes in [
        b"MANIFEST-000143\r\n".as_slice(),
        b"MANIFEST-000143\n\n",
        b"db/MANIFEST-000143\n",
        b"manifest-000143\n",
        b"MANIFEST-0\n",
        b"MANIFEST-18446744073709551616\n",
        b"MANIFEST-000143",
    ] {
        assert!(super::parse_current(bytes).is_err(), "{bytes:?}");
    }
}

#[test]
fn identity_requires_canonical_lowercase_uuid() {
    let expected = "318c61d3-7d8b-497a-b02a-d3683123595d";
    assert_eq!(
        super::parse_identity(expected.as_bytes()).unwrap(),
        expected
    );
    assert!(super::parse_identity(expected.to_uppercase().as_bytes()).is_err());
    assert!(super::parse_identity(format!("{expected}\n").as_bytes()).is_err());
    assert!(super::parse_identity(b"not-a-uuid").is_err());
}

#[test]
fn reads_the_manifest_named_by_current_instead_of_the_highest_number() {
    let mut bytes = vec![0u8; 256];
    bytes[16..32].copy_from_slice(b"MANIFEST-000121\n");
    bytes[48..52].copy_from_slice(b"old!");
    bytes[80..84].copy_from_slice(b"new!");
    let mut evidence = VecEvidenceReader::new(bytes);
    let mut reader = crate::import_pipeline::ceph_bluefs_file_reader::BluefsExtentReader::new(
        &mut evidence,
        1,
        256,
        8,
    );
    let snapshot = snapshot(vec![
        file("db/CURRENT", 2, 16, 16),
        file("db/MANIFEST-000121", 3, 48, 4),
        file("db/MANIFEST-000999", 4, 80, 4),
    ]);

    let control = super::read_rocksdb_control_files(&mut reader, &snapshot).unwrap();

    assert_eq!(control.manifest_path, "db/MANIFEST-000121");
    assert_eq!(control.manifest_file_number, 121);
    assert_eq!(control.manifest_file_size, 4);
    assert_eq!(control.manifest_bytes, b"old!");
    assert_eq!(control.identity_uuid, None);
}

fn snapshot(
    files: Vec<crate::import_pipeline::ceph_bluefs_replay::BluefsReplayFile>,
) -> crate::import_pipeline::ceph_bluefs_replay::BluefsReplaySnapshot {
    crate::import_pipeline::ceph_bluefs_replay::BluefsReplaySnapshot {
        transaction_count: 1,
        first_sequence: 1,
        final_sequence: 1,
        logical_bytes: 4096,
        stop_reason: "extentEnd".to_string(),
        directories: vec!["db".to_string()],
        files,
    }
}

fn file(
    path: &str,
    inode: u64,
    offset: u64,
    size: u32,
) -> crate::import_pipeline::ceph_bluefs_replay::BluefsReplayFile {
    crate::import_pipeline::ceph_bluefs_replay::BluefsReplayFile {
        path: path.to_string(),
        inode,
        fnode: BluefsFnode {
            ino: inode,
            size: u64::from(size),
            mtime: CephUtime {
                seconds: 0,
                nanoseconds: 0,
            },
            extents: vec![BluefsExtent {
                offset,
                length: size,
                bdev: 1,
                struct_version: 1,
                struct_compat_version: 1,
            }],
            encoding: 0,
            content_size: 0,
            struct_version: 2,
            struct_compat_version: 1,
        },
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
                path: std::path::PathBuf::from("rocksdb-control-test"),
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
