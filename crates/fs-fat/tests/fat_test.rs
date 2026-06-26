//! FAT32 synthetic fixture tests.

use evidence_core::filesystem::FileSystemReader;
use evidence_core::EvidenceReader;
use fs_fat::FatReader;
use std::io::{self, Read, Seek, SeekFrom};

struct FakeReader {
    data: Vec<u8>,
    pos: u64,
    info: evidence_core::ReaderInfo,
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
        }
    }
}
impl Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = self.pos.min(self.data.len() as u64) as usize;
        let end = (start + buf.len()).min(self.data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&self.data[start..end]);
        self.pos += n as u64;
        Ok(n)
    }
}
impl Seek for FakeReader {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.pos = match p {
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

fn write_fat32_entry(img: &mut [u8], cluster: u32, value: u32) {
    for fat_base in [1024usize, 1536usize] {
        let offset = fat_base + cluster as usize * 4;
        img[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

#[test]
fn fat32_open_file_errors_on_cycle() {
    let mut img = build_fat32_fixture();
    write_fat32_entry(&mut img, 4, 5);
    write_fat32_entry(&mut img, 5, 4);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();

    let Err(err) = fat.open_file("README.TXT") else {
        panic!("expected FAT cycle to fail");
    };
    assert!(err.to_string().contains("cycle"));
}

#[test]
fn fat32_open_file_errors_on_unexpected_free_cluster() {
    let mut img = build_fat32_fixture();
    write_fat32_entry(&mut img, 4, 0);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();

    let Err(err) = fat.open_file("README.TXT") else {
        panic!("expected unexpected free cluster to fail");
    };
    assert!(err.to_string().contains("unexpected free cluster"));
}

#[test]
fn fat32_open_file_errors_on_bad_cluster_marker() {
    let mut img = build_fat32_fixture();
    write_fat32_entry(&mut img, 4, 0x0FFF_FFF7);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();

    let Err(err) = fat.open_file("README.TXT") else {
        panic!("expected bad cluster marker to fail");
    };
    assert!(err.to_string().contains("bad cluster marker"));
}

#[test]
fn fat32_list_subdir_errors_on_directory_chain_cycle() {
    let mut img = build_fat32_fixture();
    write_fat32_entry(&mut img, 3, 3);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();

    let Err(err) = fat.list_children("SUBDIR") else {
        panic!("expected directory cluster cycle to fail");
    };
    assert!(err.to_string().contains("cycle"));
}
fn build_fat32_fixture() -> Vec<u8> {
    let bps = 512;
    let spc = 1;
    let reserved = 2;
    let fats = 2;
    let sectors_per_fat = 1;
    // Data area: 6 sectors (sectors 4-9)
    // Sector 4 (cluster 2): root dir
    // Sector 5 (cluster 3): SUBDIR
    // Sector 6 (cluster 4): file data "HELLO.TXT"
    let total_sectors = 2 + 2 + 6;
    let total = total_sectors * bps;
    let mut data = vec![0u8; total];

    // BPB
    let boot = &mut data[0..512];
    boot[0..3].copy_from_slice(&[0xEB, 0x3C, 0x90]);
    boot[3..11].copy_from_slice(b"MSDOS5.0");
    boot[11..13].copy_from_slice(&(bps as u16).to_le_bytes());
    boot[13] = spc as u8;
    boot[14..16].copy_from_slice(&(reserved as u16).to_le_bytes());
    boot[16] = fats as u8;
    boot[17..19].copy_from_slice(&0u16.to_le_bytes());
    boot[19..21].copy_from_slice(&0u16.to_le_bytes());
    boot[21] = 0xF8;
    boot[22..24].copy_from_slice(&0u16.to_le_bytes());
    boot[36..40].copy_from_slice(&(sectors_per_fat as u32).to_le_bytes());
    boot[44..48].copy_from_slice(&2u32.to_le_bytes());
    boot[32..36].copy_from_slice(&(total_sectors as u32).to_le_bytes());
    boot[510] = 0x55;
    boot[511] = 0xAA;
    boot[0x42] = 0x29;

    // FAT table at sector 2: entries 0-5
    let fat_entries: &[(usize, &[u8])] = &[
        (0, &[0xF8, 0xFF, 0xFF, 0x0F]),  // entry 0: media
        (4, &[0xFF, 0xFF, 0xFF, 0x0F]),  // entry 1: EOC
        (8, &[0xFF, 0xFF, 0xFF, 0x0F]),  // entry 2: EOC (root)
        (12, &[0xFF, 0xFF, 0xFF, 0x0F]), // entry 3: EOC (SUBDIR)
        (16, &[0xFF, 0xFF, 0xFF, 0x0F]), // entry 4: EOC (HELLO.TXT)
        (20, &[0xFF, 0xFF, 0xFF, 0x0F]), // entry 5: EOC (DEEP.TXT)
    ];
    for &(off, bytes) in fat_entries {
        data[1024 + off..1024 + off + bytes.len()].copy_from_slice(bytes);
        data[1536 + off..1536 + off + bytes.len()].copy_from_slice(bytes);
    }
    let root = &mut data[2048..];
    // Entry: "README  TXT" — file, cluster 4, 11 bytes
    let e1 = &mut root[0..32];
    e1[0..8].copy_from_slice(b"README  ");
    e1[8..11].copy_from_slice(b"TXT");
    e1[11] = 0x20;
    e1[26..28].copy_from_slice(&4u16.to_le_bytes()); // start cluster 4
    e1[28..32].copy_from_slice(&11u32.to_le_bytes()); // size 11
                                                      // Entry: "SUBDIR" — dir, cluster 3
    let e2 = &mut root[32..64];
    e2[0..8].copy_from_slice(b"SUBDIR  ");
    e2[8..11].copy_from_slice(b"   ");
    e2[11] = 0x10;
    e2[26..28].copy_from_slice(&3u16.to_le_bytes());

    // SUBDIR at sector 5 (cluster 3, offset 2560)
    let sub = &mut data[2560..];
    // Entry: "DEEP    TXT" — file, cluster 5, 9 bytes
    // Actually cluster 5 is sector 7. Let me allocate cluster 5 at sector 7.
    let e3 = &mut sub[0..32];
    e3[0..8].copy_from_slice(b"DEEP    ");
    e3[8..11].copy_from_slice(b"TXT");
    e3[11] = 0x20;
    e3[26..28].copy_from_slice(&5u16.to_le_bytes());
    e3[28..32].copy_from_slice(&9u32.to_le_bytes());

    // File data at sector 6 (cluster 4, offset 3072)
    data[3072..3083].copy_from_slice(b"Hello World");

    // File data at sector 7 (cluster 5, offset 3584)
    data[3584..3593].copy_from_slice(b"deep data");

    data
}

#[test]
fn fat32_list_root() {
    let img = build_fat32_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();

    let nodes = fat.list_children("").unwrap();
    assert_eq!(nodes.len(), 2);
    let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"README.TXT"));
    assert!(names.contains(&"SUBDIR"));
}

#[test]
fn fat32_open_file_reads_content() {
    let img = build_fat32_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();

    let mut file = fat.open_file("README.TXT").unwrap();
    let mut buf = String::new();
    file.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "Hello World");
}

#[test]
fn fat32_list_subdir() {
    let img = build_fat32_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();

    let nodes = fat.list_children("SUBDIR").unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, "DEEP.TXT");
}

#[test]
fn fat32_open_nested_file() {
    let img = build_fat32_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();

    let mut file = fat.open_file("\\SUBDIR\\DEEP.TXT").unwrap();
    let mut buf = String::new();
    file.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "deep data");
}

#[test]
fn fat32_open_nonexistent_errors() {
    let img = build_fat32_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let fat = FatReader::open(reader, 0).unwrap();
    assert!(fat.open_file("NOFILE.TXT").is_err());
}
