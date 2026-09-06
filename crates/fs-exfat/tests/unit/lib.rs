use super::*;
use crate::types::*;
use evidence_core::{EvidenceReader, FileSystemReader, LocalDiskReader};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

type ReadLog = Arc<Mutex<Vec<(u64, usize)>>>;

/// A fake reader that wraps a byte vector for testing.
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
                path: std::path::PathBuf::from("fake-exfat"),
                size: 0,
                kind: "fake-exfat".to_string(),
            },
            reads: None,
        }
    }

    fn with_read_log(data: Vec<u8>, reads: ReadLog) -> Self {
        Self {
            data,
            pos: 0,
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("fake-exfat"),
                size: 0,
                kind: "fake-exfat".to_string(),
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
            SeekFrom::Start(p) => p,
            SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
            SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl EvidenceReader for FakeReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

/// Build a minimal exFAT fixture with:
/// - Boot sector at offset 0
/// - FAT at sector 24
/// - Cluster heap at sector 32
/// - Root directory at cluster 2
/// - A file "TEST.TXT" at cluster 3
fn build_exfat_fixture() -> Vec<u8> {
    let sector_size = 512;
    let sectors_per_cluster = 1;
    let total_sectors = 1024u64; // 512KB

    let mut data = vec![0u8; (total_sectors * sector_size as u64) as usize];

    // === Boot Sector (sector 0) ===
    let boot = &mut data[0..512];
    boot[0..3].copy_from_slice(&JUMP_BOOT);
    boot[3..11].copy_from_slice(EXFAT_MAGIC);
    // PartitionOffset = 0
    boot[72..80].copy_from_slice(&total_sectors.to_le_bytes()); // VolumeLength
    boot[80..84].copy_from_slice(&24u32.to_le_bytes()); // FatOffset
    boot[84..88].copy_from_slice(&1u32.to_le_bytes()); // FatLength
    boot[88..92].copy_from_slice(&32u32.to_le_bytes()); // ClusterHeapOffset
    boot[92..96].copy_from_slice(&100u32.to_le_bytes()); // ClusterCount
    boot[96..100].copy_from_slice(&2u32.to_le_bytes()); // FirstClusterOfRootDirectory
    boot[100..104].copy_from_slice(&0x12345678u32.to_le_bytes()); // VolumeSerialNumber
    boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes()); // FileSystemRevision (1.00)
    boot[106..108].copy_from_slice(&0u16.to_le_bytes()); // VolumeFlags
    boot[108] = 9; // BytesPerSectorShift (512 = 2^9)
    boot[109] = 0; // SectorsPerClusterShift (1 = 2^0)
    boot[110] = 1; // NumberOfFats
    boot[111] = 0x80; // DriveSelect
    boot[112] = 0xFF; // PercentInUse (unknown)
    boot[510..512].copy_from_slice(&BOOT_SIGNATURE.to_le_bytes());

    // === FAT (sector 24, offset 12288) ===
    let fat_offset = 24 * sector_size;
    let fat = &mut data[fat_offset..fat_offset + sector_size];
    // FatEntry[0]: Media type
    fat[0..4].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF]);
    // FatEntry[1]: Reserved
    fat[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    // FatEntry[2]: Root directory (EOC)
    fat[8..12].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    // FatEntry[3]: TEST.TXT file data (EOC)
    fat[12..16].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);

    // === Cluster Heap (sector 32, offset 16384) ===
    let cluster_heap_offset = 32 * sector_size;
    let cluster_size = sector_size * sectors_per_cluster;

    // Cluster 2: Root directory
    let root_offset = cluster_heap_offset;
    let root = &mut data[root_offset..root_offset + cluster_size];

    // File Directory Entry for TEST.TXT
    let mut pos = 0;

    // File entry
    root[pos] = 0x85; // In-use, type 5 (File)
    root[pos + 1] = 0x02; // SecondaryCount = 2
    root[pos + 4] = 0x20; // FileAttributes = Archive
    root[pos + 5] = 0x00;
    pos += 32;

    // Stream extension
    root[pos] = 0xC0; // In-use, type 0 (Stream)
    root[pos + 1] = NO_FAT_CHAIN;
    root[pos + 3] = 8; // NameLength = 8 ("TEST.TXT")
    root[pos + 8] = 11; // ValidDataLength = 11
    root[pos + 9] = 0;
    root[pos + 10] = 0;
    root[pos + 11] = 0;
    root[pos + 12] = 0;
    root[pos + 13] = 0;
    root[pos + 14] = 0;
    root[pos + 15] = 0;
    root[pos + 20] = 3; // FirstCluster = 3
    root[pos + 21] = 0;
    root[pos + 22] = 0;
    root[pos + 23] = 0;
    root[pos + 24] = 11; // DataLength = 11
    root[pos + 25] = 0;
    root[pos + 26] = 0;
    root[pos + 27] = 0;
    pos += 32;

    // File Name entry
    root[pos] = 0xC1; // In-use, type 1 (FileName)
                      // "TEST.TXT" in UTF-16LE
    let name = "TEST.TXT";
    for (i, c) in name.encode_utf16().enumerate() {
        let offset = pos + 2 + i * 2;
        root[offset] = (c & 0xFF) as u8;
        root[offset + 1] = ((c >> 8) & 0xFF) as u8;
    }

    // Cluster 3: TEST.TXT content
    let file_offset = cluster_heap_offset + cluster_size; // Second cluster
    data[file_offset..file_offset + 11].copy_from_slice(b"Hello World");

    data
}

#[test]
fn exfat_open_valid() {
    let img = build_exfat_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    assert_eq!(fat.boot.bytes_per_sector(), 512);
    assert_eq!(fat.boot.cluster_size(), 512);
    assert_eq!(fat.boot.first_cluster_of_root, 2);
}

#[test]
fn exfat_list_root() {
    let img = build_exfat_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    let children = fat.list_children("").unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "TEST.TXT");
    assert!(!children[0].is_dir);
    assert_eq!(children[0].size, 11);
}

#[test]
fn exfat_open_file() {
    let img = build_exfat_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    let mut file = fat.open_file("TEST.TXT").unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    assert_eq!(content, "Hello World");
}

#[test]
fn exfat_range_read_nonzero_offset_reads_only_requested_extent() {
    let img = build_exfat_fixture();
    let reads = Arc::new(Mutex::new(Vec::new()));
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::with_read_log(img, reads.clone()));
    let fat = ExfatReader::open(reader, 0).unwrap();

    reads.lock().unwrap().clear();
    let bytes = fat.read_file_range("TEST.TXT", 6, 5).unwrap();

    assert_eq!(bytes, b"World");
    let file_offset = 32 * 512 + 512;
    let file_data_reads: Vec<_> = reads
        .lock()
        .unwrap()
        .iter()
        .copied()
        .filter(|(offset, _)| *offset >= file_offset && *offset < file_offset + 512)
        .collect();
    assert_eq!(file_data_reads, vec![(file_offset + 6, 5)]);
}

#[test]
fn exfat_open_nonexistent() {
    let img = build_exfat_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    assert!(fat.open_file("NOFILE.TXT").is_err());
}

#[test]
fn exfat_root_properties() {
    let img = build_exfat_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    let root = fat.root().unwrap();
    assert_eq!(root.name, "\\");
    assert!(root.is_dir);
    assert_eq!(root.size, 0);
}

#[test]
fn exfat_open_file_honors_no_fat_chain() {
    let mut img = build_exfat_fixture();
    let fat_offset = 24 * 512;
    img[fat_offset + 3 * 4..fat_offset + 3 * 4 + 4].copy_from_slice(&0u32.to_le_bytes());
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    let mut file = fat.open_file("TEST.TXT").unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    assert_eq!(content, "Hello World");
}

#[test]
fn exfat_data_source_name() {
    let img = build_exfat_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    assert_eq!(fat.data_source_name(), "exFAT");
}

#[test]
fn exfat_open_file_errors_on_out_of_range_file_cluster() {
    let mut img = build_exfat_fixture();
    // TEST.TXT stream extension first_cluster field.
    img[16384 + 32 + 20..16384 + 32 + 24].copy_from_slice(&200u32.to_le_bytes());
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    let Err(err) = fat.open_file("TEST.TXT") else {
        panic!("expected out-of-range file cluster to fail");
    };
    assert!(err.to_string().contains("out of range"));
}

#[test]
fn exfat_open_file_errors_on_no_fat_chain_extent_past_cluster_count() {
    let mut img = build_exfat_fixture();
    img[16384 + 32 + 20..16384 + 32 + 24].copy_from_slice(&100u32.to_le_bytes());
    img[16384 + 32 + 24..16384 + 32 + 32].copy_from_slice(&2048u64.to_le_bytes());
    img[16384 + 32 + 8..16384 + 32 + 16].copy_from_slice(&2048u64.to_le_bytes());
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    let Err(err) = fat.open_file("TEST.TXT") else {
        panic!("expected overflowing NoFatChain extent to fail");
    };
    assert!(err.to_string().contains("NoFatChain run"));
}

#[test]
fn exfat_open_file_errors_on_chain_longer_than_cluster_count() {
    let mut img = build_exfat_fixture();
    let fat_offset = 24 * 512;
    img[16384 + 32 + 1] = 0;
    for cluster in 3u32..=102 {
        let next = cluster + 1;
        let entry_offset = fat_offset + cluster as usize * 4;
        img[entry_offset..entry_offset + 4].copy_from_slice(&next.to_le_bytes());
    }
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = ExfatReader::open(reader, 0).unwrap();

    let Err(err) = fat.open_file("TEST.TXT") else {
        panic!("expected overlong cluster chain to fail");
    };
    let err = err.to_string();
    assert!(err.contains("declared cluster count") || err.contains("out of range"));
}

#[test]
#[ignore = "requires an administrator-readable exFAT physical volume"]
fn exfat_real_physical_volume_is_read_only_parseable() {
    let device = std::env::var("FORENSICS_EXFAT_DEVICE")
        .expect("set FORENSICS_EXFAT_DEVICE to \\\\.\\PhysicalDriveN");
    let offset = std::env::var("FORENSICS_EXFAT_OFFSET")
        .expect("set FORENSICS_EXFAT_OFFSET to the exFAT partition byte offset")
        .parse::<u64>()
        .expect("FORENSICS_EXFAT_OFFSET must be an unsigned byte offset");
    let reader = LocalDiskReader::open(std::path::Path::new(&device)).unwrap();
    let filesystem = ExfatReader::open(Box::new(reader), offset).unwrap();
    let children = filesystem.list_children("").unwrap();
    assert!(
        !children.is_empty(),
        "the exFAT root directory is unexpectedly empty"
    );
}
