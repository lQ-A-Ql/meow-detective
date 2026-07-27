use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, PartitionWindowReader, ReaderInfo};

struct MemoryEvidence {
    cursor: Cursor<Vec<u8>>,
    info: ReaderInfo,
}

impl MemoryEvidence {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            info: ReaderInfo {
                path: PathBuf::from("memory.raw"),
                size: bytes.len() as u64,
                kind: "memory".to_string(),
            },
            cursor: Cursor::new(bytes),
        }
    }
}

impl Read for MemoryEvidence {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(buf)
    }
}

impl Seek for MemoryEvidence {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(position)
    }
}

impl EvidenceReader for MemoryEvidence {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        4096
    }
}

#[test]
fn partition_offsets_are_zero_based_and_reads_stop_at_the_boundary() {
    let source = Box::new(MemoryEvidence::new((0..32u8).collect()));
    let mut window = PartitionWindowReader::new(source, 8, Some(10)).expect("valid window");
    let mut bytes = [0u8; 16];
    let read = window.read(&mut bytes).expect("bounded read");
    assert_eq!(read, 10);
    assert_eq!(&bytes[..read], &(8..18u8).collect::<Vec<_>>());
    assert_eq!(window.read(&mut bytes).expect("EOF"), 0);
}

#[test]
fn seeks_are_relative_to_the_partition_not_the_source() {
    let source = Box::new(MemoryEvidence::new((0..32u8).collect()));
    let mut window = PartitionWindowReader::new(source, 8, Some(10)).expect("valid window");
    window.seek(SeekFrom::End(-2)).expect("seek in window");
    let mut bytes = [0u8; 2];
    window.read_exact(&mut bytes).expect("read tail");
    assert_eq!(bytes, [16, 17]);
    assert_eq!(window.info().size, 10);
    assert_eq!(window.preferred_read_granularity(), 4096);
}

#[test]
fn windows_outside_the_evidence_are_rejected() {
    let source = Box::new(MemoryEvidence::new(vec![0u8; 32]));
    assert!(PartitionWindowReader::new(source, 24, Some(9)).is_err());
}
