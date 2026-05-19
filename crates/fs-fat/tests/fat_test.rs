//! FAT32 synthetic fixture tests.

use evidence_core::EvidenceReader;
use evidence_core::filesystem::FileSystemReader;
use fs_fat::FatReader;
use std::io::{self, Read, Seek, SeekFrom};

struct FakeReader {
    data: Vec<u8>,
    pos: u64,
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
        unimplemented!()
    }
}

/// Build a minimal FAT32 image:
/// - Boot sector with BPB
/// - 2 FAT tables (1 sector each)
/// - Root directory in cluster 2 with 2 entries
/// - Data clusters
fn build_fat32_fixture() -> Vec<u8> {
    let bps = 512;
    let spc = 1;
    let reserved = 2;
    let fats = 2;
    let sectors_per_fat = 1;
    // Root dir entries: 0 (FAT32 uses cluster chain)
    let total_sectors = 6;
    let total = total_sectors * bps;
    let mut data = vec![0u8; total];

    // BPB
    let boot = &mut data[0..512];
    boot[0..3].copy_from_slice(&[0xEB, 0x3C, 0x90]); // jmp
    boot[3..11].copy_from_slice(b"MSDOS5.0");
    boot[11..13].copy_from_slice(&(bps as u16).to_le_bytes());
    boot[13] = spc as u8;
    boot[14..16].copy_from_slice(&(reserved as u16).to_le_bytes());
    boot[16] = fats as u8;
    boot[17..19].copy_from_slice(&0u16.to_le_bytes()); // root entries = 0 (FAT32)
    boot[19..21].copy_from_slice(&0u16.to_le_bytes()); // total16 = 0
    boot[21] = 0xF8; // media
    boot[22..24].copy_from_slice(&0u16.to_le_bytes()); // sectors_per_fat16 = 0
    boot[24..26].copy_from_slice(&(sectors_per_fat as u16).to_le_bytes()); // sectors_per_track? skip
    // FAT32 extended BPB
    boot[36..40].copy_from_slice(&(sectors_per_fat as u32).to_le_bytes()); // sectors_per_fat32
    boot[44..48].copy_from_slice(&2u32.to_le_bytes()); // root_cluster = 2
    boot[32..36].copy_from_slice(&(total_sectors as u32).to_le_bytes()); // total_sectors32
    boot[510] = 0x55;
    boot[511] = 0xAA;
    boot[0x42] = 0x29; // FAT32 extended boot signature

    // FAT table at sector 2 (offset 1024):
    // Entry 0: media (F8 FF FF 0F)
    // Entry 1: EOC (FF FF FF 0F)
    // Entry 2: EOC (FF FF FF 0F)
    data[1024..1028].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0x0F]);
    data[1028..1032].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x0F]);
    data[1032..1036].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x0F]);
    // FAT table 2 at sector 3 (offset 1536): same
    data[1536..1540].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0x0F]);
    data[1540..1544].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x0F]);
    data[1544..1548].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x0F]);

    // Root dir in cluster 2 (sector 4, offset 2048)
    // Entry 1: "README  TXT" (SFN), 0 bytes
    let root = &mut data[2048..];
    let e1 = &mut root[0..32];
    e1[0..8].copy_from_slice(b"README  ");
    e1[8..11].copy_from_slice(b"TXT");
    e1[11] = 0x20; // archive attribute
    // Entry 2: "SUBDIR" (dir), cluster 3
    let e2 = &mut root[32..64];
    e2[0..8].copy_from_slice(b"SUBDIR  ");
    e2[8..11].copy_from_slice(b"   ");
    e2[11] = 0x10; // directory
    e2[26..28].copy_from_slice(&3u16.to_le_bytes()); // starting cluster = 3

    data
}

#[test]
fn fat32_list_root() {
    let img = build_fat32_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
    let fat = FatReader::open(reader, 0).unwrap();

    let nodes = fat.list_children("").unwrap();
    assert_eq!(nodes.len(), 2);
    let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"README.TXT"));
    assert!(names.contains(&"SUBDIR"));
}
