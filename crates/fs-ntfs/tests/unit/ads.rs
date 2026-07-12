use super::*;
use crate::NtfsReader;
use evidence_core::EvidenceReader;
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
                path: std::path::PathBuf::from("fake-ntfs-ads"),
                size: 0,
                kind: "fake-ntfs-ads".to_string(),
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

/// Build a minimal NTFS boot sector.
fn make_boot(boot: &mut [u8]) {
    boot[0] = 0xEB;
    boot[1] = 0x52;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&512u16.to_le_bytes());
    boot[13] = 1; // 1 sector per cluster
    boot[0x30..0x38].copy_from_slice(&2u64.to_le_bytes()); // MFT at cluster 2
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes()); // 1024-byte records
}

/// Write a resident, named $DATA attribute at `offset` in the record.
/// Returns the byte offset after the attribute.
fn write_named_data_attr(rec: &mut [u8], offset: usize, name: &str, content: &[u8]) -> usize {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let name_bytes = utf16.len() * 2;
    let name_off = 0x18u16; // name starts after the resident content header
    let content_off = name_off as usize + name_bytes;
    // Align content start to 4-byte boundary
    let content_off_aligned = (content_off + 3) & !3;
    let attr_len = content_off_aligned + content.len();

    rec[offset..offset + 4].copy_from_slice(&0x80u32.to_le_bytes()); // type $DATA
    rec[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    rec[offset + 8] = 0; // resident flag
    rec[offset + 9] = utf16.len() as u8; // name length
    rec[offset + 0x0A..offset + 0x0C].copy_from_slice(&name_off.to_le_bytes());
    // content_size @ +0x10
    rec[offset + 0x10..offset + 0x14].copy_from_slice(&(content.len() as u32).to_le_bytes());
    // content_offset @ +0x14
    rec[offset + 0x14..offset + 0x16].copy_from_slice(&(content_off_aligned as u16).to_le_bytes());
    // Write the name
    for (i, c) in utf16.iter().enumerate() {
        let name_pos = offset + name_off as usize + i * 2;
        rec[name_pos..name_pos + 2].copy_from_slice(&c.to_le_bytes());
    }
    // Write the content
    let data_start = offset + content_off_aligned;
    rec[data_start..data_start + content.len()].copy_from_slice(content);
    offset + attr_len
}

/// Build a fixture with a file "test.txt" (inode 6) that has:
/// - unnamed $DATA: "main content"
/// - named $DATA "Zone.Identifier": "[ZoneTransfer]\r\nZoneId=3"
fn build_ads_fixture() -> Vec<u8> {
    let mft_record_size = 1024usize;
    let mft_cluster = 2u64;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    let total = rec6_off + mft_record_size + 512;
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    // Root: test.txt → inode 6
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());

    // INDX entry for test.txt
    let utf16: Vec<u16> = "test.txt".encode_utf16().collect();
    let name_bytes = utf16.len() * 2;
    let entry_size = 0x52 + name_bytes;
    let mut off = iro + 0x20;
    rec5[off..off + 8].copy_from_slice(&6u64.to_le_bytes()); // mft_ref=6
    rec5[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
    rec5[off + 0x50] = utf16.len() as u8;
    for (i, c) in utf16.iter().enumerate() {
        rec5[off + 0x52 + i * 2..off + 0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    off += entry_size;
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    // File record (inode 6) with unnamed $DATA + named $DATA
    let rec6 = &mut data[rec6_off..rec6_off + mft_record_size];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());

    let _si_end = {
        rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
        rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
        0x68usize
    };

    // Unnamed $DATA (resident)
    let unnamed_off = 0x68usize;
    rec6[unnamed_off..unnamed_off + 4].copy_from_slice(&0x80u32.to_le_bytes());
    rec6[unnamed_off + 4..unnamed_off + 8].copy_from_slice(&0x30u32.to_le_bytes()); // len=0x30
    rec6[unnamed_off + 8] = 0; // resident
    rec6[unnamed_off + 0x10..unnamed_off + 0x14].copy_from_slice(&12u32.to_le_bytes()); // content_size
    rec6[unnamed_off + 0x14..unnamed_off + 0x16].copy_from_slice(&0x18u16.to_le_bytes()); // content_off
    let main_content = b"main content";
    let main_start = unnamed_off + 0x18;
    rec6[main_start..main_start + main_content.len()].copy_from_slice(main_content);

    // Named $DATA: Zone.Identifier
    write_named_data_attr(
        rec6,
        unnamed_off + 0x30,
        "Zone.Identifier",
        b"[ZoneTransfer]\r\nZoneId=3",
    );

    data
}

#[test]
fn list_ads_returns_named_stream() {
    let img = build_ads_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");

    let streams = list_alternate_streams(&ntfs, "test.txt").expect("list_ads");
    assert_eq!(streams.len(), 1, "expected 1 alternate stream");
    assert_eq!(streams[0].name, "Zone.Identifier");
    assert!(streams[0].size > 0, "stream should have content");
}

#[test]
fn read_ads_returns_stream_content() {
    let img = build_ads_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");

    let content = read_alternate_stream(&ntfs, "test.txt", "Zone.Identifier").expect("read_ads");
    let text = String::from_utf8_lossy(&content);
    assert!(text.contains("[ZoneTransfer]"), "expected ZoneTransfer");
    assert!(text.contains("ZoneId=3"), "expected ZoneId=3");
}

#[test]
fn read_ads_nonexistent_returns_empty() {
    let img = build_ads_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");

    let content = read_alternate_stream(&ntfs, "test.txt", "NoSuchStream").expect("read_ads");
    assert!(content.is_empty());
}

#[test]
fn list_ads_no_streams_returns_empty() {
    // Build a fixture with a file that has only unnamed $DATA
    let mft_record_size = 1024usize;
    let mft_cluster = 2u64;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    let total = rec6_off + mft_record_size + 512;
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    // Root: plain.txt → inode 6
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let utf16: Vec<u16> = "plain.txt".encode_utf16().collect();
    let name_bytes = utf16.len() * 2;
    let entry_size = 0x52 + name_bytes;
    let mut off = iro + 0x20;
    rec5[off..off + 8].copy_from_slice(&6u64.to_le_bytes());
    rec5[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
    rec5[off + 0x50] = utf16.len() as u8;
    for (i, c) in utf16.iter().enumerate() {
        rec5[off + 0x52 + i * 2..off + 0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    off += entry_size;
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    // File record: only unnamed $DATA
    let rec6 = &mut data[rec6_off..rec6_off + mft_record_size];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    rec6[0x68..0x6C].copy_from_slice(&0x80u32.to_le_bytes());
    rec6[0x6C..0x70].copy_from_slice(&0x30u32.to_le_bytes());
    rec6[0x70] = 0; // resident + unnamed
    rec6[0x78..0x7C].copy_from_slice(&4u32.to_le_bytes());
    rec6[0x7C..0x7E].copy_from_slice(&0x18u16.to_le_bytes());
    b"data".iter().enumerate().for_each(|(i, &b)| {
        rec6[0x80 + i] = b;
    });

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data));
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");

    let streams = list_alternate_streams(&ntfs, "plain.txt").expect("list_ads");
    assert!(streams.is_empty(), "expected no alternate streams");
}

#[test]
fn list_ads_multiple_streams() {
    // Build a file with two named $DATA streams
    let mft_record_size = 1024usize;
    let mft_cluster = 2u64;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    // Need a larger record to hold two named attributes
    let total = rec6_off + 2048 + 512;
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    // Root: multi.txt → inode 6
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let utf16: Vec<u16> = "multi.txt".encode_utf16().collect();
    let name_bytes = utf16.len() * 2;
    let entry_size = 0x52 + name_bytes;
    let mut off = iro + 0x20;
    rec5[off..off + 8].copy_from_slice(&6u64.to_le_bytes());
    rec5[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
    rec5[off + 0x50] = utf16.len() as u8;
    for (i, c) in utf16.iter().enumerate() {
        rec5[off + 0x52 + i * 2..off + 0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    off += entry_size;
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    // File record with two named $DATA
    let rec6 = &mut data[rec6_off..rec6_off + 2048];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());

    // $STANDARD_INFORMATION
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    // Unnamed $DATA
    rec6[0x68..0x6C].copy_from_slice(&0x80u32.to_le_bytes());
    rec6[0x6C..0x70].copy_from_slice(&0x30u32.to_le_bytes());
    rec6[0x70] = 0;
    rec6[0x78..0x7C].copy_from_slice(&1u32.to_le_bytes());
    rec6[0x7C..0x7E].copy_from_slice(&0x18u16.to_le_bytes());
    rec6[0x80] = b'X';

    let off1 = 0x98usize;
    let next = write_named_data_attr(rec6, off1, "stream-one", b"AAA");

    write_named_data_attr(rec6, next, "stream-two", b"BBBB");

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data));
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");

    let streams = list_alternate_streams(&ntfs, "multi.txt").expect("list_ads");
    assert_eq!(streams.len(), 2, "expected 2 alternate streams");
    let names: Vec<&str> = streams.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"stream-one"));
    assert!(names.contains(&"stream-two"));
}

#[test]
fn read_ads_case_insensitive_name() {
    let img = build_ads_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");

    // NTFS stream names are case-insensitive
    let content = read_alternate_stream(&ntfs, "test.txt", "zone.identifier").expect("read_ads");
    assert!(!content.is_empty(), "case-insensitive lookup should work");
}

#[test]
fn list_ads_nonexistent_path_returns_empty() {
    let img = build_ads_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");

    let streams = list_alternate_streams(&ntfs, "nonexistent.txt").expect("list_ads");
    assert!(streams.is_empty());
}
