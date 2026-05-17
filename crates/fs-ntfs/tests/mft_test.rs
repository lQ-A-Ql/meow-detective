use evidence_core::EvidenceReader;
use evidence_core::filesystem::FileSystemReader;
use fs_ntfs::NtfsReader;
use std::io::Cursor;

/// Build a minimal synthetic NTFS filesystem for testing.
/// Contains boot sector + $MFT record 0 ($MFT itself) + $MFT record 5 (root directory)
/// with an $INDEX_ROOT attribute containing INDX entries.
fn build_ntfs_fixture() -> Vec<u8> {
    let bps = 512u16;
    let spc = 1u8;
    let mft_cluster = 2u64;

    let mut data = vec![0u8; 2048];

    // Boot sector at offset 0
    let boot = &mut data[0..512];
    boot[0] = 0xEB; boot[1] = 0x52; boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&bps.to_le_bytes());
    boot[13] = spc;
    boot[0x30..0x38].copy_from_slice(&mft_cluster.to_le_bytes());
    boot[0x2C..0x34].copy_from_slice(&(5u64 | (5 << 48)).to_le_bytes()); // root dir = MFT ref 5, seq 5
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes()); // 2^-10 clusters = 1024 bytes

    // $MFT record 0 at cluster 2 (offset 1024)
    let mft0 = &mut data[1024..];
    mft0[0..4].copy_from_slice(b"FILE");
    let attr_off = 0x38u16;
    mft0[0x14..0x16].copy_from_slice(&attr_off.to_le_bytes());
    // $FILE_NAME attribute at offset 0x38
    let fna = &mut mft0[0x38..];
    fna[0..4].copy_from_slice(&0x30u32.to_le_bytes()); // type = $FILE_NAME
    fna[4..8].copy_from_slice(&68u32.to_le_bytes()); // length = 68 (fake, enough space)
    fna[0x40] = 4; // name length in chars (8 bytes)
    let name = "$MFT".encode_utf16().collect::<Vec<_>>();
    fna[0x5A..0x62].copy_from_slice(&bytemuck_u16(&name)); // name UTF-16LE
    fna[68..72].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // end marker

    // $MFT record 5 (root dir) at cluster 2 + 5*1024 = offset 1024+5120 = 6144
    let rec5_offset = 1024 + 5 * 1024;
    if rec5_offset + 600 > data.len() {
        data.resize(rec5_offset + 1024, 0);
    }
    let rec5 = &mut data[rec5_offset..];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes()); // attribute offset
    // $STANDARD_INFORMATION at 0x38 (just enough to have a valid record)
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    // $INDEX_ROOT at offset 0x38+48 = 0x68
    let iro = 0x68usize;
    rec5[iro..iro+4].copy_from_slice(&0x90u32.to_le_bytes()); // type = $INDEX_ROOT
    // Build INDX entries in the space after INDEX_ROOT header
    let header_size = 0x10usize; // 16 bytes header
    let entries_start = iro + header_size;
    rec5[iro+4..iro+8].copy_from_slice(&200u32.to_le_bytes()); // attribute length
    rec5[iro+0x10..iro+0x14].copy_from_slice(&(header_size as u32).to_le_bytes()); // entries offset = 0x10

    // Build 3 INDX entries
    let entries = &[
        ("$AttrDef", true),
        ("$BadClus", true),
        ("README.TXT", false),
    ];
    let mut eoff = entries_start;
    for (ename, is_dir) in entries {
        let name_utf16: Vec<u16> = ename.encode_utf16().collect();
        let name_bytes = name_utf16.len() * 2;
        let entry_size = 0x52 + name_bytes;
        if eoff + entry_size > rec5.len() { break; }
        rec5[eoff..eoff+8].copy_from_slice(&100u64.to_le_bytes()); // MFT ref
        rec5[eoff+8..eoff+10].copy_from_slice(&(entry_size as u16).to_le_bytes());
        rec5[eoff+0x10] = name_utf16.len() as u8; // name length
        if *is_dir {
            rec5[eoff+0x38..eoff+0x3C].copy_from_slice(&0x10000000u32.to_le_bytes()); // dir flag
        }
        for (i, c) in name_utf16.iter().enumerate() {
            rec5[eoff+0x52 + i*2..eoff+0x52 + i*2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        eoff += entry_size;
    }
    // end marker
    rec5[eoff..eoff+4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    rec5[iro+4..iro+8].copy_from_slice(&((eoff - iro) as u32).to_le_bytes()); // fix attribute length

    data
}

fn bytemuck_u16(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|c| c.to_le_bytes()).collect()
}

#[test]
fn list_root_children() {
    let img = build_ntfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
    let mut ntfs = NtfsReader::open(reader, 0).unwrap();
    let nodes = ntfs.list_root_children().unwrap();
    assert!(nodes.len() >= 3, "got {} nodes", nodes.len());
    assert!(nodes.iter().any(|n| n.is_dir && n.name == "$AttrDef"));
    assert!(nodes.iter().any(|n| n.is_dir && n.name == "$BadClus"));
    assert!(nodes.iter().any(|n| !n.is_dir && n.name == "README.TXT"));
}

#[test]
fn non_root_path_unsupported() {
    let img = build_ntfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let result = ntfs.list_children("Windows");
    assert!(result.is_err());
}

/// Simple EvidenceReader wrapping a Vec<u8>
struct FakeReader { data: Vec<u8>, pos: u64 }

impl std::io::Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = self.pos as usize;
        let end = (start + buf.len()).min(self.data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&self.data[start..end]);
        self.pos += n as u64;
        Ok(n)
    }
}
impl std::io::Seek for FakeReader {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.pos = match pos {
            std::io::SeekFrom::Start(p) => p,
            std::io::SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
            std::io::SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
        };
        Ok(self.pos)
    }
}
impl EvidenceReader for FakeReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        unreachable!()
    }
}
