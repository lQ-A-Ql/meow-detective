use evtx::err::{ChunkError, EvtxError, EvtxSourceIoOperation};
use evtx::{EvtxParser, ParserSettings};
use std::io::{Cursor, Read, Seek, SeekFrom};

const EVTX_FILE_HEADER_SIZE: usize = 4_096;
const EVTX_CHUNK_SIZE: usize = 65_536;

#[test]
fn multibatch_parse_error_uses_absolute_chunk_identity() {
    let mut parser = EvtxParser::from_buffer(evtx_with_error_after_skipped_chunk())
        .expect("synthetic EVTX header should parse")
        .with_configuration(ParserSettings::default().num_threads(2));

    let errors = parser
        .records_json_value()
        .filter_map(Result::err)
        .collect::<Vec<_>>();

    assert_eq!(errors.len(), 1);
    let EvtxError::FailedToParseChunk { chunk_id, .. } = &errors[0] else {
        panic!("expected a chunk parse error, got {:?}", errors[0]);
    };

    // Chunks 0 and 1 occupy the first worker batch. Chunk 2 is empty and skipped;
    // the malformed chunk is physically chunk 3, not item 0 or 2 of a later batch.
    assert_eq!(*chunk_id, 3);
    let audit_offset = EVTX_FILE_HEADER_SIZE as u64 + chunk_id * EVTX_CHUNK_SIZE as u64;
    assert_eq!(audit_offset, 0x31_000);
}

#[test]
fn malformed_chunk_does_not_stop_later_chunk_salvage() {
    let mut bytes = evtx_with_error_after_skipped_chunk();
    bytes.extend(empty_chunk());
    let malformed_offset = EVTX_FILE_HEADER_SIZE + EVTX_CHUNK_SIZE * 3;
    bytes[malformed_offset] = b'X';
    let mut parser = EvtxParser::from_buffer(bytes).expect("synthetic EVTX header should parse");

    let results = parser
        .chunks()
        .map(|result| result.is_ok())
        .collect::<Vec<_>>();

    assert_eq!(results, vec![true, true, false, true]);
}

#[test]
fn chunk_read_failure_retains_fatal_source_io_identity() {
    let fail_at = EVTX_FILE_HEADER_SIZE as u64 + EVTX_CHUNK_SIZE as u64;
    let reader =
        FaultingReader::read_error(valid_empty_evtx(2), fail_at, std::io::ErrorKind::Other);
    let mut parser = EvtxParser::from_read_seek(reader).expect("header should parse");

    let mut records = parser.records_json_value();
    let error = records
        .find_map(Result::err)
        .expect("second chunk read should fail");

    let EvtxError::FailedToParseChunk { source, .. } = &error else {
        panic!("expected chunk failure, got {error:?}");
    };
    assert!(matches!(
        source.as_ref(),
        ChunkError::FailedToReadChunk(error) if error.kind() == std::io::ErrorKind::Other
    ));
    let (operation, source) = error.source_io().expect("source I/O classification");
    assert_eq!(operation, EvtxSourceIoOperation::ReadChunk);
    assert_eq!(source.kind(), std::io::ErrorKind::Other);
    assert!(
        records.next().is_none(),
        "fatal source I/O must fuse subsequent record iteration"
    );
}

#[test]
fn chunk_seek_failure_retains_fatal_source_io_identity() {
    let fail_at = EVTX_FILE_HEADER_SIZE as u64 + EVTX_CHUNK_SIZE as u64;
    let reader =
        FaultingReader::seek_error(valid_empty_evtx(2), fail_at, std::io::ErrorKind::Other);
    let mut parser = EvtxParser::from_read_seek(reader).expect("header should parse");

    let mut records = parser.records_json_value();
    let error = records
        .find_map(Result::err)
        .expect("second chunk seek should fail");

    let EvtxError::FailedToParseChunk { source, .. } = &error else {
        panic!("expected chunk failure, got {error:?}");
    };
    assert!(matches!(
        source.as_ref(),
        ChunkError::FailedToSeekToChunk(error) if error.kind() == std::io::ErrorKind::Other
    ));
    let (operation, source) = error.source_io().expect("source I/O classification");
    assert_eq!(operation, EvtxSourceIoOperation::SeekToChunk);
    assert_eq!(source.kind(), std::io::ErrorKind::Other);
    assert!(
        records.next().is_none(),
        "fatal source I/O must fuse subsequent record iteration"
    );
}

fn evtx_with_error_after_skipped_chunk() -> Vec<u8> {
    let mut bytes = vec![0_u8; EVTX_FILE_HEADER_SIZE];
    bytes[0..8].copy_from_slice(b"ElfFile\0");
    bytes[32..36].copy_from_slice(&128_u32.to_le_bytes());
    bytes[36..38].copy_from_slice(&1_u16.to_le_bytes());
    bytes[38..40].copy_from_slice(&3_u16.to_le_bytes());
    bytes[40..42].copy_from_slice(&(EVTX_FILE_HEADER_SIZE as u16).to_le_bytes());
    bytes[42..44].copy_from_slice(&4_u16.to_le_bytes());

    bytes.extend(empty_chunk());
    bytes.extend(empty_chunk());
    bytes.extend(vec![0_u8; EVTX_CHUNK_SIZE]);

    let mut malformed_chunk = empty_chunk();
    malformed_chunk[128..132].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend(malformed_chunk);
    bytes
}

fn empty_chunk() -> Vec<u8> {
    let mut chunk = vec![0_u8; EVTX_CHUNK_SIZE];
    chunk[0..8].copy_from_slice(b"ElfChnk\0");
    chunk[40..44].copy_from_slice(&128_u32.to_le_bytes());
    chunk[48..52].copy_from_slice(&512_u32.to_le_bytes());
    chunk
}

fn valid_empty_evtx(chunk_count: usize) -> Vec<u8> {
    let mut bytes = vec![0_u8; EVTX_FILE_HEADER_SIZE];
    bytes[0..8].copy_from_slice(b"ElfFile\0");
    bytes[32..36].copy_from_slice(&128_u32.to_le_bytes());
    bytes[36..38].copy_from_slice(&1_u16.to_le_bytes());
    bytes[38..40].copy_from_slice(&3_u16.to_le_bytes());
    bytes[40..42].copy_from_slice(&(EVTX_FILE_HEADER_SIZE as u16).to_le_bytes());
    bytes[42..44].copy_from_slice(&(chunk_count as u16).to_le_bytes());
    for _ in 0..chunk_count {
        bytes.extend(empty_chunk());
    }
    bytes
}

enum FaultMode {
    Read,
    Seek,
}

struct FaultingReader {
    inner: Cursor<Vec<u8>>,
    fail_at: u64,
    kind: std::io::ErrorKind,
    mode: FaultMode,
}

impl FaultingReader {
    fn read_error(bytes: Vec<u8>, fail_at: u64, kind: std::io::ErrorKind) -> Self {
        Self {
            inner: Cursor::new(bytes),
            fail_at,
            kind,
            mode: FaultMode::Read,
        }
    }

    fn seek_error(bytes: Vec<u8>, fail_at: u64, kind: std::io::ErrorKind) -> Self {
        Self {
            inner: Cursor::new(bytes),
            fail_at,
            kind,
            mode: FaultMode::Seek,
        }
    }
}

impl Read for FaultingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if matches!(self.mode, FaultMode::Read) && self.inner.position() >= self.fail_at {
            return Err(std::io::Error::new(
                self.kind,
                "injected chunk read failure",
            ));
        }
        let allowed = usize::try_from(self.fail_at.saturating_sub(self.inner.position()))
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let buffer = if matches!(self.mode, FaultMode::Read) {
            &mut buffer[..allowed]
        } else {
            buffer
        };
        self.inner.read(buffer)
    }
}

impl Seek for FaultingReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        if matches!(self.mode, FaultMode::Seek)
            && matches!(position, SeekFrom::Start(offset) if offset >= self.fail_at)
        {
            return Err(std::io::Error::new(
                self.kind,
                "injected chunk seek failure",
            ));
        }
        self.inner.seek(position)
    }
}
