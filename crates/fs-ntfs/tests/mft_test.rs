//! NTFS synthetic fixture test — list_root_children with correct INDX offsets.
use evidence_core::filesystem::FileSystemReader;
use evidence_core::EvidenceReader;
use fs_ntfs::NtfsReader;
use std::io;

/// Build minimal NTFS: boot sector + $MFT record 5 (root dir) with $INDEX_ROOT.
/// INDX entry layout (corrected):
///   +0x00: MFT ref (8B)   +0x08: entry_size (2B)  +0x0A: padding (6B)
///   +0x10: $FILE_NAME start
///   +0x48: flags (4B)     +0x50: name_len (1B)    +0x52: name (UTF-16LE)
fn build_fixture() -> Vec<u8> {
    let bps = 512u16;
    let spc = 1u8;
    let mft_cluster = 2u64;

    // Boot @ 0, $MFT record 5 @ 2*512 + 5*1024 = 6144
    let rec5_offset = mft_cluster as usize * 512 + 5 * 1024;
    let mut data = vec![0u8; rec5_offset + 2048];

    // Boot sector
    let boot = &mut data[0..512];
    boot[0] = 0xEB;
    boot[1] = 0x52;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&bps.to_le_bytes());
    boot[13] = spc;
    boot[0x30..0x38].copy_from_slice(&mft_cluster.to_le_bytes());
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes()); // 2^-10 clusters = 1024-byte MFT records

    // MFT record 5 — root directory
    let rec5 = &mut data[rec5_offset..];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes()); // first attribute offset

    // $STANDARD_INFORMATION (0x10) — minimal
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    rec5[0x68..0x6C].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // end marker (optional)

    // $INDEX_ROOT (0x90) at offset 0x68 + 0x10 for attribute header
    let iro = 0x68usize; // position 0x68 within record
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes()); // type
    rec5[iro + 4..iro + 8].copy_from_slice(&0u32.to_le_bytes()); // len placeholder
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes()); // entries offset = 16

    // INDX entries start at iro + 0x20
    let entries_start = iro + 0x20;
    let entries = &[
        ("$AttrDef", true),
        ("$BadClus", true),
        ("README.TXT", false),
    ];

    let mut off = entries_start;
    for &(name, is_dir) in entries {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let name_bytes = utf16.len() * 2;
        let entry_size = 0x52 + name_bytes; // 0x52 header + name

        rec5[off..off + 8].copy_from_slice(&(100u64 + off as u64).to_le_bytes()); // MFT ref
        rec5[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
        rec5[off + 0x50] = utf16.len() as u8; // name_len @ +0x50
        if is_dir {
            rec5[off + 0x48..off + 0x4C].copy_from_slice(&0x10000000u32.to_le_bytes());
            // flags @ +0x48
        }
        for (i, c) in utf16.iter().enumerate() {
            rec5[off + 0x52 + i * 2..off + 0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        off += entry_size;
    }
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes()); // fix length

    data
}

struct FakeReader {
    data: Vec<u8>,
    pos: u64,
}
impl io::Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = self.pos.min(self.data.len() as u64) as usize;
        let end = (start + buf.len()).min(self.data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&self.data[start..end]);
        self.pos += n as u64;
        Ok(n)
    }
}
impl io::Seek for FakeReader {
    fn seek(&mut self, p: io::SeekFrom) -> io::Result<u64> {
        self.pos = match p {
            io::SeekFrom::Start(p) => p,
            io::SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
            io::SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
        };
        Ok(self.pos)
    }
}
impl EvidenceReader for FakeReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        unimplemented!()
    }
}

#[test]
fn list_root_children_returns_3_nodes() {
    let img = build_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");
    let nodes = ntfs.list_root_children().expect("list_root_children");
    assert_eq!(
        nodes.len(),
        3,
        "expected 3 nodes, got {}: {:?}",
        nodes.len(),
        nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
    );
    assert!(
        nodes.iter().any(|n| n.is_dir && n.name == "$AttrDef"),
        "missing $AttrDef dir"
    );
    assert!(
        nodes.iter().any(|n| n.is_dir && n.name == "$BadClus"),
        "missing $BadClus dir"
    );
    assert!(
        nodes.iter().any(|n| !n.is_dir && n.name == "README.TXT"),
        "missing README.TXT file"
    );
}

#[test]
fn non_root_path_unsupported() {
    let img = build_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    assert!(ntfs.list_children("subdir").is_err());
}
