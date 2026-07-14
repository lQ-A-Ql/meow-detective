use std::io::{Read, Seek, SeekFrom};

use ceph_wire::{BluefsExtent, BluefsFnode, CephUtime};

#[test]
fn reads_plain_file_across_extents_without_allocated_padding() {
    let mut bytes = vec![0u8; 128];
    bytes[16..20].copy_from_slice(b"abcd");
    bytes[40..44].copy_from_slice(b"efgh");
    let mut evidence = VecEvidenceReader::new(bytes);
    let mut reader = super::BluefsExtentReader::new(&mut evidence, 1, 128, 8);
    let mut fnode = fnode(vec![extent(16, 4, 1), extent(40, 4, 1)]);
    fnode.size = 6;

    assert_eq!(reader.read_plain_file(&fnode).unwrap(), b"abcdef");
}

#[test]
fn allocated_range_supports_zero_size_metadata_logs() {
    let mut bytes = vec![0u8; 64];
    bytes[16..20].copy_from_slice(b"log!");
    let mut evidence = VecEvidenceReader::new(bytes);
    let mut reader = super::BluefsExtentReader::new(&mut evidence, 1, 64, 8);
    let mut log = fnode(vec![extent(16, 4, 1)]);
    log.size = 0;

    assert_eq!(
        reader.read_allocated_range(&log, 0, 4).unwrap(),
        Some(b"log!".to_vec())
    );
}

#[test]
fn file_range_is_exact_and_never_reads_allocated_padding() {
    let mut bytes = vec![0u8; 128];
    bytes[16..20].copy_from_slice(b"abcd");
    bytes[40..44].copy_from_slice(b"efgh");
    let mut evidence = VecEvidenceReader::new(bytes);
    let mut reader = super::BluefsExtentReader::new(&mut evidence, 1, 128, 8);
    let mut fnode = fnode(vec![extent(16, 4, 1), extent(40, 4, 1)]);
    fnode.size = 6;

    let prepared = reader.prepare_file(&fnode).unwrap();
    assert_eq!(
        reader.read_prepared_file_range(&prepared, 2, 4).unwrap(),
        b"cdef"
    );
    assert!(reader.read_prepared_file_range(&prepared, 4, 4).is_err());
}

#[test]
fn prepared_file_range_reads_fragmented_extents_and_rejects_overlap() {
    let mut bytes = vec![0u8; 256];
    bytes[16..20].copy_from_slice(b"abcd");
    bytes[80..84].copy_from_slice(b"efgh");
    bytes[160..164].copy_from_slice(b"ijkl");
    let mut evidence = VecEvidenceReader::new(bytes);
    let mut reader = super::BluefsExtentReader::new(&mut evidence, 1, 256, 8);
    let file = fnode(vec![extent(16, 4, 1), extent(80, 4, 1), extent(160, 4, 1)]);
    let prepared = reader.prepare_file(&file).expect("prepare fragmented file");

    assert_eq!(
        reader
            .read_prepared_file_range(&prepared, 3, 7)
            .expect("read prepared range"),
        b"defghij"
    );

    let overlapping = fnode(vec![extent(16, 8, 1), extent(20, 8, 1)]);
    assert!(reader.prepare_file(&overlapping).is_err());
}

#[test]
fn rejects_cross_device_and_reserved_extents() {
    let mut evidence = VecEvidenceReader::new(vec![0u8; 128]);
    let mut reader = super::BluefsExtentReader::new(&mut evidence, 1, 128, 8);

    assert!(reader
        .read_plain_file(&fnode(vec![extent(16, 4, 2)]))
        .is_err());
    assert!(reader
        .read_plain_file(&fnode(vec![extent(4, 4, 1)]))
        .is_err());
}

#[test]
fn rejects_encoded_or_logically_oversized_files() {
    let mut evidence = VecEvidenceReader::new(vec![0u8; 128]);
    let mut reader = super::BluefsExtentReader::new(&mut evidence, 1, 128, 8);
    let mut encoded = fnode(vec![extent(16, 4, 1)]);
    encoded.encoding = 1;
    let error = reader.read_plain_file(&encoded).unwrap_err();
    assert_eq!(error.category, "unsupported");

    let mut oversized = fnode(vec![extent(16, 4, 1)]);
    oversized.size = 5;
    assert!(reader.read_plain_file(&oversized).is_err());
}

fn fnode(extents: Vec<BluefsExtent>) -> BluefsFnode {
    BluefsFnode {
        ino: 2,
        size: extents.iter().map(|extent| u64::from(extent.length)).sum(),
        mtime: CephUtime {
            seconds: 0,
            nanoseconds: 0,
        },
        extents,
        encoding: 0,
        content_size: 0,
        struct_version: 2,
        struct_compat_version: 1,
    }
}

fn extent(offset: u64, length: u32, bdev: u8) -> BluefsExtent {
    BluefsExtent {
        offset,
        length,
        bdev,
        struct_version: 1,
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
                path: std::path::PathBuf::from("bluefs-file-test"),
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
