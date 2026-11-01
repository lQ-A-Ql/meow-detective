use std::io::{Cursor, Read, Seek, SeekFrom};

use evidence_block::{BlockProvider, EvidenceReaderBlockProvider};
use evidence_core::{EvidenceReader, ReaderInfo};

struct FixtureReader {
    cursor: Cursor<Vec<u8>>,
    info: ReaderInfo,
}

impl FixtureReader {
    fn new(bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        Self {
            cursor: Cursor::new(bytes),
            info: ReaderInfo {
                path: "fixture".into(),
                size,
                kind: "fixture".into(),
            },
        }
    }
}

impl Read for FixtureReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(buffer)
    }
}

impl Seek for FixtureReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(position)
    }
}

impl EvidenceReader for FixtureReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

#[test]
fn adapts_sequential_reader_to_bounded_random_access() {
    let provider = EvidenceReaderBlockProvider::new(FixtureReader::new(b"0123456789".to_vec()));
    let mut output = [0; 4];
    provider.read_exact_at(3, &mut output).unwrap();
    assert_eq!(&output, b"3456");
}

#[test]
fn rejects_reads_past_reader_end() {
    let provider = EvidenceReaderBlockProvider::new(FixtureReader::new(b"0123456789".to_vec()));
    let error = provider.read_exact_at(8, &mut [0; 3]).unwrap_err();
    assert!(matches!(
        error,
        evidence_block::BlockDeviceError::OutOfBounds { .. }
    ));
}
