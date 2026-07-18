use super::*;

const BLOCK_SIZE: usize = 4096;

struct FakeReader {
    data: Vec<u8>,
    pos: u64,
    info: ReaderInfo,
    preferred_read_granularity: usize,
}

impl FakeReader {
    fn new(data: Vec<u8>, kind: &str) -> Self {
        Self::with_granularity(data, kind, 0)
    }

    fn with_granularity(data: Vec<u8>, kind: &str, preferred_read_granularity: usize) -> Self {
        Self {
            info: ReaderInfo {
                path: std::path::PathBuf::from(kind),
                size: data.len() as u64,
                kind: kind.to_string(),
            },
            data,
            pos: 0,
            preferred_read_granularity,
        }
    }
}

impl Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = self.pos as usize;
        let end = start.saturating_add(buf.len()).min(self.data.len());
        let read = end.saturating_sub(start);
        buf[..read].copy_from_slice(&self.data[start..end]);
        self.pos += read as u64;
        Ok(read)
    }
}

impl Seek for FakeReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => (self.data.len() as i64 + offset).max(0) as u64,
            SeekFrom::Current(offset) => (self.pos as i64 + offset).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl EvidenceReader for FakeReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        self.preferred_read_granularity
    }
}

#[test]
fn thin_reader_maps_allocated_blocks_and_zero_fills_unmapped_blocks() {
    let metadata = build_thin_metadata();
    let mut data = vec![0u8; 4 * 512];
    data[2 * 512..2 * 512 + 11].copy_from_slice(b"THIN-BLOCK0");

    let thin_metadata =
        ThinMetadata::open(Box::new(FakeReader::new(metadata, "thin-metadata"))).unwrap();
    let data_reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data, "thin-data"));
    let mut reader =
        ThinLvReader::new(thin_metadata, data_reader, "thin-root".to_string(), 1024, 7).unwrap();

    let mut allocated = [0u8; 11];
    reader.read_exact(&mut allocated).unwrap();
    assert_eq!(&allocated, b"THIN-BLOCK0");

    reader.seek(SeekFrom::Start(512)).unwrap();
    let mut unmapped = [0xAAu8; 16];
    reader.read_exact(&mut unmapped).unwrap();
    assert_eq!(unmapped, [0u8; 16]);
}

#[test]
fn thin_reader_propagates_data_reader_granularity() {
    let metadata = ThinMetadata::open(Box::new(FakeReader::new(
        build_thin_metadata(),
        "thin-metadata",
    )))
    .unwrap();
    let data_reader: Box<dyn EvidenceReader> = Box::new(FakeReader::with_granularity(
        vec![0u8; 4 * 512],
        "thin-data",
        64 * 1024,
    ));

    let reader =
        ThinLvReader::new(metadata, data_reader, "thin-root".to_string(), 1024, 7).unwrap();

    assert_eq!(reader.preferred_read_granularity(), 64 * 1024);
}

fn build_thin_metadata() -> Vec<u8> {
    let mut metadata = vec![0u8; 4 * BLOCK_SIZE];
    let superblock = &mut metadata[0..BLOCK_SIZE];
    superblock[8..16].copy_from_slice(&0u64.to_le_bytes());
    superblock[32..40].copy_from_slice(&27_022_010u64.to_le_bytes());
    superblock[40..44].copy_from_slice(&1u32.to_le_bytes());
    superblock[48..56].copy_from_slice(&1u64.to_le_bytes());
    superblock[320..328].copy_from_slice(&1u64.to_le_bytes());
    superblock[328..336].copy_from_slice(&2u64.to_le_bytes());
    superblock[336..340].copy_from_slice(&1u32.to_le_bytes());
    superblock[340..344].copy_from_slice(&8u32.to_le_bytes());
    superblock[344..352].copy_from_slice(&4u64.to_le_bytes());

    write_leaf_node(&mut metadata, 1, 7, 8, &3u64.to_le_bytes());
    let mut detail = [0u8; 24];
    detail[0..8].copy_from_slice(&1u64.to_le_bytes());
    detail[8..16].copy_from_slice(&1u64.to_le_bytes());
    write_leaf_node(&mut metadata, 2, 7, 24, &detail);
    let block_time = 2u64 << 24;
    write_leaf_node(&mut metadata, 3, 0, 8, &block_time.to_le_bytes());
    metadata
}

fn write_leaf_node(metadata: &mut [u8], block: u64, key: u64, value_size: u32, value: &[u8]) {
    let start = block as usize * BLOCK_SIZE;
    let node = &mut metadata[start..start + BLOCK_SIZE];
    let max_entries = 3u32;
    node[4..8].copy_from_slice(&2u32.to_le_bytes());
    node[8..16].copy_from_slice(&block.to_le_bytes());
    node[16..20].copy_from_slice(&1u32.to_le_bytes());
    node[20..24].copy_from_slice(&max_entries.to_le_bytes());
    node[24..28].copy_from_slice(&value_size.to_le_bytes());
    node[32..40].copy_from_slice(&key.to_le_bytes());
    let value_offset = 32 + max_entries as usize * 8;
    node[value_offset..value_offset + value.len()].copy_from_slice(value);
}
