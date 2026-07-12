use super::*;
use evidence_core::filesystem::join_child_path;
use std::sync::{Arc, Mutex};

type ReadLog = Arc<Mutex<Vec<(u64, usize)>>>;

struct FakeReader {
    data: Vec<u8>,
    pos: u64,
    info: evidence_core::ReaderInfo,
    reads: Option<ReadLog>,
}

impl FakeReader {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            pos: 0,
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("fake-fat"),
                size: 0,
                kind: "fake-fat".to_string(),
            },
            reads: None,
        }
    }

    fn with_read_log(data: Vec<u8>, reads: ReadLog) -> Self {
        Self {
            data,
            pos: 0,
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("fake-fat"),
                size: 0,
                kind: "fake-fat".to_string(),
            },
            reads: Some(reads),
        }
    }
}

impl Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = self.pos.min(self.data.len() as u64) as usize;
        let end = (start + buf.len()).min(self.data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&self.data[start..end]);
        if let Some(reads) = &self.reads {
            reads.lock().unwrap().push((self.pos, buf.len()));
        }
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for FakeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(pos) => pos,
            SeekFrom::End(delta) => (self.data.len() as i64 + delta).max(0) as u64,
            SeekFrom::Current(delta) => (self.pos as i64 + delta).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl evidence_core::EvidenceReader for FakeReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

fn build_fat32_fixture() -> Vec<u8> {
    const SECTOR_SIZE: usize = 512;
    const RESERVED_SECTORS: usize = 1;
    const FAT_SECTORS: usize = 1;
    const FIRST_DATA_SECTOR: usize = RESERVED_SECTORS + FAT_SECTORS;
    const CLUSTER_SIZE: usize = SECTOR_SIZE;

    let total_sectors = 16usize;
    let mut data = vec![0u8; total_sectors * SECTOR_SIZE];

    let boot = &mut data[0..SECTOR_SIZE];
    boot[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
    boot[3..11].copy_from_slice(b"MSDOS5.0");
    boot[11..13].copy_from_slice(&(SECTOR_SIZE as u16).to_le_bytes());
    boot[13] = 1;
    boot[14..16].copy_from_slice(&(RESERVED_SECTORS as u16).to_le_bytes());
    boot[16] = 1;
    boot[17..19].copy_from_slice(&0u16.to_le_bytes());
    boot[32..36].copy_from_slice(&(total_sectors as u32).to_le_bytes());
    boot[36..40].copy_from_slice(&(FAT_SECTORS as u32).to_le_bytes());
    boot[44..48].copy_from_slice(&2u32.to_le_bytes());
    boot[0x42] = 0x29;
    boot[82..90].copy_from_slice(b"FAT32   ");
    boot[510] = 0x55;
    boot[511] = 0xAA;

    let fat_offset = RESERVED_SECTORS * SECTOR_SIZE;
    let fat = &mut data[fat_offset..fat_offset + SECTOR_SIZE];
    fat[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
    fat[4..8].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    fat[8..12].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    fat[12..16].copy_from_slice(&4u32.to_le_bytes());
    fat[16..20].copy_from_slice(&5u32.to_le_bytes());
    fat[20..24].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());

    let root_offset = FIRST_DATA_SECTOR * SECTOR_SIZE;
    let root = &mut data[root_offset..root_offset + CLUSTER_SIZE];
    root[0..8].copy_from_slice(b"RANGE   ");
    root[8..11].copy_from_slice(b"TXT");
    root[11] = 0x20;
    root[26..28].copy_from_slice(&3u16.to_le_bytes());
    root[28..32].copy_from_slice(&(CLUSTER_SIZE as u32 * 3).to_le_bytes());

    for cluster in 3..=5usize {
        let value = match cluster {
            3 => b'A',
            4 => b'B',
            5 => b'C',
            _ => unreachable!(),
        };
        let offset = FIRST_DATA_SECTOR * SECTOR_SIZE + (cluster - 2) * CLUSTER_SIZE;
        data[offset..offset + CLUSTER_SIZE].fill(value);
    }

    data
}

#[test]
fn test_read_sfn_name() {
    let mut entry = [0u8; 32];
    // HELLO followed by nulls, then TXT
    entry[0..5].copy_from_slice(b"HELLO");
    // bytes 5-7 are null (padding)
    entry[8..11].copy_from_slice(b"TXT");
    let name = read_sfn_name(&entry);
    assert!(name.contains("HELLO"));
    assert!(name.contains("TXT"));
}

#[test]
fn test_read_sfn_name_no_ext() {
    let mut entry = [0u8; 32];
    entry[0..6].copy_from_slice(b"README");
    let name = read_sfn_name(&entry);
    assert!(name.contains("README"));
}

#[test]
fn test_join_child_path() {
    assert_eq!(join_child_path("", "file.txt"), "file.txt");
    assert_eq!(join_child_path("dir", "file.txt"), "dir/file.txt");
    assert_eq!(join_child_path("dir/sub", "file.txt"), "dir/sub/file.txt");
}

#[test]
fn test_join_child_path_backslash() {
    assert_eq!(join_child_path("dir\\sub", "file.txt"), "dir/sub/file.txt");
}

#[test]
fn test_fat_type_detection() {
    assert_eq!(FatType::Fat12, FatType::Fat12);
    assert_ne!(FatType::Fat12, FatType::Fat32);
}

#[test]
fn fat_open_file_still_returns_complete_file() {
    let image = build_fat32_fixture();
    let reader: Box<dyn evidence_core::EvidenceReader> = Box::new(FakeReader::new(image));
    let fs = FatReader::open(reader, 0).unwrap();

    let mut file = fs.open_file("RANGE.TXT").unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();

    assert_eq!(bytes.len(), 1536);
    assert_eq!(&bytes[0..4], b"AAAA");
    assert_eq!(&bytes[512..516], b"BBBB");
    assert_eq!(&bytes[1024..1028], b"CCCC");
}

#[test]
fn fat_range_read_nonzero_offset_reads_only_target_cluster_data() {
    const SECTOR_SIZE: u64 = 512;
    const FIRST_DATA_SECTOR: u64 = 2;
    let image = build_fat32_fixture();
    let reads = Arc::new(Mutex::new(Vec::new()));
    let reader: Box<dyn evidence_core::EvidenceReader> =
        Box::new(FakeReader::with_read_log(image, reads.clone()));
    let fs = FatReader::open(reader, 0).unwrap();

    reads.lock().unwrap().clear();
    let bytes = fs.read_file_range("RANGE.TXT", 512 + 7, 9).unwrap();

    assert_eq!(bytes, vec![b'B'; 9]);
    let data_cluster_reads: Vec<_> = reads
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|(offset, _)| {
            *offset >= (FIRST_DATA_SECTOR + 2) * SECTOR_SIZE
                && *offset < (FIRST_DATA_SECTOR + 4) * SECTOR_SIZE
        })
        .collect();
    assert_eq!(
        data_cluster_reads,
        vec![((FIRST_DATA_SECTOR + 2) * SECTOR_SIZE + 7, 9)]
    );
}
