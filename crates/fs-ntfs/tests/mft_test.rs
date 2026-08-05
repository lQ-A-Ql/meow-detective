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
    info: evidence_core::ReaderInfo,
}
impl FakeReader {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            pos: 0,
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("fake-ntfs"),
                size: 0,
                kind: "fake-ntfs".to_string(),
            },
        }
    }
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
        &self.info
    }
}

#[test]
fn list_root_children_returns_3_nodes() {
    let img = build_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
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
fn list_children_subdir_returns_empty() {
    let img = build_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let result = ntfs.list_subdir_children("subdir").unwrap();
    assert!(result.is_empty());
}

/// Build a fixture with nested directories: root → Windows → System32
/// Tests top-down path resolution eliminates same-name collisions.
fn build_nested_fixture() -> Vec<u8> {
    let bps = 512u16;
    let spc = 1u8;
    let mft_cluster = 2u64;
    let mft_record_size = 1024usize;

    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size; // 6144
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size; // 7168
    let rec7_off = mft_cluster as usize * 512 + 7 * mft_record_size; // 8192
    let total = rec7_off + mft_record_size + 512;
    let mut data = vec![0u8; total];

    // Boot sector
    let boot = &mut data[0..512];
    boot[0] = 0xEB;
    boot[1] = 0x52;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&bps.to_le_bytes());
    boot[13] = spc;
    boot[0x30..0x38].copy_from_slice(&mft_cluster.to_le_bytes());
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes());

    // Helper: write MFT record with $INDEX_ROOT and named children
    fn write_dir_record(rec: &mut [u8], children: &[(&str, u64)]) {
        rec[0..4].copy_from_slice(b"FILE");
        rec[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
        // $STANDARD_INFORMATION (0x10) placeholder
        rec[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
        rec[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

        // $INDEX_ROOT (0x90) at offset 0x68
        let iro = 0x68usize;
        rec[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
        rec[iro + 4..iro + 8].copy_from_slice(&0u32.to_le_bytes()); // patched later
        rec[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());

        let mut off = iro + 0x20;
        for &(name, mft_ref) in children {
            let utf16: Vec<u16> = name.encode_utf16().collect();
            let name_bytes = utf16.len() * 2;
            let entry_size = 0x52 + name_bytes;
            rec[off..off + 8].copy_from_slice(&mft_ref.to_le_bytes());
            rec[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
            rec[off + 0x50] = utf16.len() as u8;
            rec[off + 0x48..off + 0x4C].copy_from_slice(&0x10000000u32.to_le_bytes());
            for (i, c) in utf16.iter().enumerate() {
                rec[off + 0x52 + i * 2..off + 0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
            }
            off += entry_size;
        }
        rec[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        rec[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());
    }

    // Root → [Windows (inode 6)]
    write_dir_record(
        &mut data[rec5_off..rec5_off + mft_record_size],
        &[("Windows", 6)],
    );
    // Windows → [System32 (inode 7)]
    write_dir_record(
        &mut data[rec6_off..rec6_off + mft_record_size],
        &[("System32", 7)],
    );
    // System32 → [ntdll.dll]
    write_dir_record(
        &mut data[rec7_off..rec7_off + mft_record_size],
        &[("ntdll.dll", 100)],
    );

    data
}

#[test]
fn resolve_nested_path_lists_children() {
    let img = build_nested_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();

    let nodes = ntfs.list_subdir_children("\\Windows\\System32").unwrap();
    assert_eq!(nodes.len(), 1, "System32 should have 1 child");
    assert_eq!(nodes[0].name, "ntdll.dll");
}

#[test]
fn wrong_path_returns_empty() {
    let img = build_nested_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();

    // "System32" exists but only under "Windows" — bare name should fail
    let nodes = ntfs.list_subdir_children("System32").unwrap();
    assert!(nodes.is_empty(), "bare System32 should not resolve");
}

#[test]
fn later_file_name_parent_match_resolves_directory() {
    let mft_record_size = 1024usize;
    let mft_cluster = 2u64;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    let total = rec6_off + mft_record_size + 1024;
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let (dir_entry, _) = build_indx_entries(&["System Volume Information"], 6, true);
    let mut off = iro + 0x20;
    rec5[off..off + dir_entry.len()].copy_from_slice(&dir_entry);
    off += dir_entry.len();
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    let rec6 = &mut data[rec6_off..rec6_off + mft_record_size];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let mut attr_off = write_resident_file_name(rec6, 0x68, 99, "SYSTEM~1");
    attr_off = write_resident_file_name(rec6, attr_off, 5, "System Volume Information");
    rec6[attr_off..attr_off + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec6[attr_off + 0x10..attr_off + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let (child_entry, _) = build_indx_entries(&["tracking.log"], 100, false);
    off = attr_off + 0x20;
    rec6[off..off + child_entry.len()].copy_from_slice(&child_entry);
    off += child_entry.len();
    rec6[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec6[attr_off + 4..attr_off + 8].copy_from_slice(&((off - attr_off) as u32).to_le_bytes());

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let nodes = ntfs
        .list_subdir_children("System Volume Information")
        .unwrap();

    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, "tracking.log");
}

// --- Phase 16: $INDEX_ALLOCATION tests ---

/// Build INDX entries as bytes. Returns (data, entry_count).
/// If `is_dir` is true, sets the directory flag (0x10000000).
fn build_indx_entries(names: &[&str], base_mft_ref: u64, is_dir: bool) -> (Vec<u8>, usize) {
    let mut data = Vec::new();
    for (i, &name) in names.iter().enumerate() {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let name_bytes = utf16.len() * 2;
        let entry_size = 0x52 + name_bytes;
        let mut entry = vec![0u8; entry_size];
        let mft_ref = base_mft_ref + i as u64;
        entry[0..8].copy_from_slice(&mft_ref.to_le_bytes());
        entry[8..10].copy_from_slice(&(entry_size as u16).to_le_bytes());
        if is_dir {
            entry[0x48..0x4C].copy_from_slice(&0x10000000u32.to_le_bytes());
        }
        entry[0x50] = utf16.len() as u8;
        for (j, c) in utf16.iter().enumerate() {
            entry[0x52 + j * 2..0x52 + j * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        data.extend_from_slice(&entry);
    }
    (data, names.len())
}

/// Build a complete INDX record with fixup array.
/// Sector size = 512. Record will be exactly `sectors * 512` bytes.
fn build_indx_record(entries_data: &[u8], sectors: usize) -> Vec<u8> {
    let rec_size = sectors * 512;
    let mut rec = vec![0u8; rec_size];

    // INDX magic
    rec[0..4].copy_from_slice(&0x58444E49u32.to_le_bytes());
    // update sequence offset = 0x28, count = sectors + 1
    let upd_off = 0x28u16;
    let upd_cnt = (sectors + 1) as u16;
    rec[4..6].copy_from_slice(&upd_off.to_le_bytes());
    rec[6..8].copy_from_slice(&upd_cnt.to_le_bytes());

    // Index entry list header at +0x18
    let list_off = 0x18usize;
    let ent_start = 0x18u32; // entries start at list + 0x18
    let ent_total = ent_start + entries_data.len() as u32;
    rec[list_off..list_off + 4].copy_from_slice(&ent_start.to_le_bytes());
    rec[list_off + 4..list_off + 8].copy_from_slice(&ent_total.to_le_bytes());
    rec[list_off + 8..list_off + 12].copy_from_slice(&ent_total.to_le_bytes());

    // Copy entries
    let ent_abs = list_off + ent_start as usize;
    if ent_abs + entries_data.len() <= rec.len() {
        rec[ent_abs..ent_abs + entries_data.len()].copy_from_slice(entries_data);
    }

    // Set up fixup: last 2 bytes of each sector = upd_seq[0] value
    let upd_seq_val = 0xABCDu16;
    rec[upd_off as usize..upd_off as usize + 2].copy_from_slice(&upd_seq_val.to_le_bytes());
    // For each sector i (1..upd_cnt), save the original last 2 bytes into
    // upd_seq[i], then overwrite sector end with upd_seq_val.
    for i in 1..upd_cnt as usize {
        let sec_end = i * 512;
        if sec_end >= 2 {
            let orig = u16::from_le_bytes([rec[sec_end - 2], rec[sec_end - 1]]);
            let repl_idx = upd_off as usize + 2 + (i - 1) * 2;
            if repl_idx + 2 <= rec.len() {
                rec[repl_idx..repl_idx + 2].copy_from_slice(&orig.to_le_bytes());
            }
            rec[sec_end - 2..sec_end].copy_from_slice(&upd_seq_val.to_le_bytes());
        }
    }

    rec
}

fn write_resident_index_bitmap(
    record: &mut [u8],
    offset: usize,
    name: Option<&str>,
    attribute_id: u16,
) -> usize {
    let name_units = name
        .map(|value| value.encode_utf16().collect::<Vec<_>>())
        .unwrap_or_default();
    let name_offset = 0x18usize;
    let content_offset = (name_offset + name_units.len() * 2 + 7) & !7;
    let attr_len = content_offset + 8;
    record[offset..offset + 4].copy_from_slice(&0xB0u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    record[offset + 9] = name_units.len() as u8;
    record[offset + 0x0A..offset + 0x0C].copy_from_slice(&(name_offset as u16).to_le_bytes());
    record[offset + 0x0E..offset + 0x10].copy_from_slice(&attribute_id.to_le_bytes());
    record[offset + 0x10..offset + 0x14].copy_from_slice(&1u32.to_le_bytes());
    record[offset + 0x14..offset + 0x16].copy_from_slice(&(content_offset as u16).to_le_bytes());
    for (index, character) in name_units.iter().enumerate() {
        let start = offset + name_offset + index * 2;
        record[start..start + 2].copy_from_slice(&character.to_le_bytes());
    }
    record[offset + content_offset] = 1;
    let end = offset + attr_len;
    record[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    end
}

/// Build a fixture with root directory containing:
/// $INDEX_ROOT → 3 entries
/// $INDEX_ALLOCATION → INDX record with 5 more entries (non-resident, data run)
fn build_index_alloc_fixture() -> Vec<u8> {
    let bps = 512u16;
    let spc = 1u8; // 1 sector per cluster
    let mft_cluster = 2u64;
    let mft_record_size = 1024usize;
    let cluster_size = bps as usize * spc as usize; // 512

    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size; // 6144

    // INDX buffer at cluster 32 (offset 16384)
    let indx_cluster = 32u64;
    let indx_offset = indx_cluster as usize * cluster_size; // 16384

    let (root_ent_data, _) = build_indx_entries(&["Alpha", "Beta", "Gamma"], 10, true);
    let (alloc_ent_data, _) =
        build_indx_entries(&["One", "Two", "Three", "Four", "Five"], 100, true);
    let indx_rec = build_indx_record(&alloc_ent_data, 3); // 3 sectors = 1536 bytes

    let total = indx_offset + indx_rec.len().max(2048);
    let mut data = vec![0u8; total];

    // Boot sector
    let boot = &mut data[0..512];
    boot[0] = 0xEB;
    boot[1] = 0x52;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&bps.to_le_bytes());
    boot[13] = spc;
    boot[0x30..0x38].copy_from_slice(&mft_cluster.to_le_bytes());
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes());
    boot[0x44] = 3;

    // MFT record 5 — root directory with $INDEX_ROOT + $INDEX_ALLOCATION
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());

    // $STANDARD_INFORMATION (0x10) — placeholder
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    // $INDEX_ROOT (0x90) at 0x68
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    // length placeholder — will fix after $INDEX_ALLOCATION
    let iro_len_pos = iro + 4;

    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes()); // entries_off
                                                                          // Copy root entries
    let mut off = iro + 0x20;
    rec5[off..off + root_ent_data.len()].copy_from_slice(&root_ent_data);
    off += root_ent_data.len();
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // end marker
    off += 4;
    // Patch $INDEX_ROOT length
    let iro_len = (off - iro) as u32;
    rec5[iro_len_pos..iro_len_pos + 4].copy_from_slice(&iro_len.to_le_bytes());

    // $INDEX_ALLOCATION (0xA0) — non-resident
    let idxa = off;
    rec5[idxa..idxa + 4].copy_from_slice(&0xA0u32.to_le_bytes()); // type
                                                                  // length placeholder
    let idxa_len_pos = idxa + 4;
    rec5[idxa + 8] = 1; // non-resident flag
    rec5[idxa + 0x18..idxa + 0x20].copy_from_slice(&2u64.to_le_bytes());
    // data_run_offset = 0x40
    rec5[idxa + 0x20..idxa + 0x22].copy_from_slice(&0x40u16.to_le_bytes());
    // allocated_size = 1536
    rec5[idxa + 0x28..idxa + 0x30].copy_from_slice(&1536u64.to_le_bytes());
    // real_size = 1536
    rec5[idxa + 0x30..idxa + 0x38].copy_from_slice(&1536u64.to_le_bytes());

    // Data run at idxa + 0x40:
    //   header: 0x31 → size=1 byte, offset=3 bytes
    //   length: 3 clusters (1536 bytes / 512)
    //   offset: LCN 32 (relative to LCN 0)
    let run_pos = idxa + 0x40;
    rec5[run_pos] = 0x31; // size_bytes=1, offset_bytes=3
    rec5[run_pos + 1] = 3; // 3 clusters
    rec5[run_pos + 2..run_pos + 5].copy_from_slice(&32u64.to_le_bytes()[..3]);

    // End marker after data run
    rec5[run_pos + 5] = 0x00;
    // Patch $INDEX_ALLOCATION length
    let idxa_len = (run_pos + 6 - idxa) as u32;
    rec5[idxa_len_pos..idxa_len_pos + 4].copy_from_slice(&idxa_len.to_le_bytes());
    write_resident_index_bitmap(rec5, run_pos + 6, None, 0);

    // Write INDX record at the data run target
    data[indx_offset..indx_offset + indx_rec.len()].copy_from_slice(&indx_rec);

    data
}

#[test]
fn list_root_with_index_alloc_returns_all_entries() {
    let img = build_index_alloc_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let nodes = ntfs.list_root_children().unwrap();

    // 3 from $INDEX_ROOT + 5 from $INDEX_ALLOCATION = 8 total
    assert_eq!(
        nodes.len(),
        8,
        "expected 8 entries, got {}: {:?}",
        nodes.len(),
        nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
    );
}

#[test]
fn index_alloc_entries_are_directories() {
    let img = build_index_alloc_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let nodes = ntfs.list_root_children().unwrap();

    let alloc_names: Vec<_> = nodes.iter().map(|n| n.name.clone()).collect();
    assert!(alloc_names.contains(&"One".to_string()));
    assert!(alloc_names.contains(&"Five".to_string()));
    assert!(alloc_names.contains(&"Alpha".to_string()));
    // All should be directories
    for n in &nodes {
        assert!(n.is_dir, "{} should be a directory", n.name);
    }
}

#[test]
fn data_run_parse_zero_length_runs_skipped() {
    // Build a fixture where the data run has a zero-length (sparse) run
    let bps = 512u16;
    let spc = 1u8;
    let mft_cluster = 2u64;
    let mft_record_size = 1024usize;
    let cluster_size = 512;

    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    // INDX data at cluster 32
    let indx_cluster = 32u64;
    let indx_offset = indx_cluster as usize * cluster_size;
    let (entries_data, _) = build_indx_entries(&["FileA"], 1, true);
    let indx_rec = build_indx_record(&entries_data, 1);

    let mut data = vec![0u8; indx_offset + 1024];
    let boot = &mut data[0..512];
    boot[0] = 0xEB;
    boot[1] = 0x52;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&bps.to_le_bytes());
    boot[13] = spc;
    boot[0x30..0x38].copy_from_slice(&mft_cluster.to_le_bytes());
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes());
    boot[0x44] = 1;

    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    // $INDEX_ROOT with 0 entries (just end marker)
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    let iro_len = 0x20u32 + 4; // header + end marker
    rec5[iro + 4..iro + 8].copy_from_slice(&iro_len.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[iro + 0x30..iro + 0x34].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());

    // $INDEX_ALLOCATION with data run that has a zero-length run first
    let idxa = iro + iro_len as usize;
    rec5[idxa..idxa + 4].copy_from_slice(&0xA0u32.to_le_bytes());
    rec5[idxa + 8] = 1;
    rec5[idxa + 0x20..idxa + 0x22].copy_from_slice(&0x40u16.to_le_bytes());
    rec5[idxa + 0x28..idxa + 0x30].copy_from_slice(&512u64.to_le_bytes());
    rec5[idxa + 0x30..idxa + 0x38].copy_from_slice(&512u64.to_le_bytes());
    // Data runs: sparse run (length=0, skip), then real run
    let run = idxa + 0x40;
    rec5[run] = 0x11; // size=1, offset=1 → sparse marker
    rec5[run + 1] = 0; // zero clusters
    rec5[run + 2] = 0; // zero offset
    rec5[run + 3] = 0x31; // size=1, offset=3
    rec5[run + 4] = 1; // 1 cluster
    rec5[run + 5..run + 8].copy_from_slice(&32u64.to_le_bytes()[..3]);
    rec5[run + 8] = 0x00; // end of runs
    let idxa_len = (run + 9 - idxa) as u32;
    rec5[idxa + 4..idxa + 8].copy_from_slice(&idxa_len.to_le_bytes());
    write_resident_index_bitmap(rec5, run + 9, None, 0);

    data[indx_offset..indx_offset + indx_rec.len()].copy_from_slice(&indx_rec);

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let nodes = ntfs.list_root_children().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, "FileA");
}

// --- Phase 17: $DATA attribute + open_file tests ---

/// Build a resident $DATA attribute for a file record.
fn write_resident_data(rec: &mut [u8], offset: usize, content: &[u8]) -> usize {
    rec[offset..offset + 4].copy_from_slice(&0x80u32.to_le_bytes()); // type $DATA
    let content_off = 0x18u16; // content starts at attr + 0x18
    let attr_len = 0x18 + content.len() as u32;
    rec[offset + 4..offset + 8].copy_from_slice(&attr_len.to_le_bytes());
    rec[offset + 8] = 0; // resident flag
                         // content_size @ +0x10
    rec[offset + 0x10..offset + 0x14].copy_from_slice(&(content.len() as u32).to_le_bytes());
    // content_offset @ +0x14
    rec[offset + 0x14..offset + 0x16].copy_from_slice(&content_off.to_le_bytes());
    // Copy content
    let data_start = offset + content_off as usize;
    rec[data_start..data_start + content.len()].copy_from_slice(content);
    offset + attr_len as usize
}

fn write_resident_file_name(rec: &mut [u8], offset: usize, parent_ref: u64, name: &str) -> usize {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let content_len = 0x42 + utf16.len() * 2;
    let content_off = 0x18u16;
    let attr_len = content_off as usize + content_len;

    rec[offset..offset + 4].copy_from_slice(&0x30u32.to_le_bytes());
    rec[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    rec[offset + 8] = 0;
    rec[offset + 0x10..offset + 0x14].copy_from_slice(&(content_len as u32).to_le_bytes());
    rec[offset + 0x14..offset + 0x16].copy_from_slice(&content_off.to_le_bytes());

    let content_start = offset + content_off as usize;
    rec[content_start..content_start + 8].copy_from_slice(&parent_ref.to_le_bytes());
    rec[content_start + 0x40] = utf16.len() as u8;
    rec[content_start + 0x41] = 1;
    for (i, c) in utf16.iter().enumerate() {
        let char_off = content_start + 0x42 + i * 2;
        rec[char_off..char_off + 2].copy_from_slice(&c.to_le_bytes());
    }

    offset + attr_len
}

/// Build a fixture with a root directory containing a file "README.TXT"
/// that has resident $DATA with the string "Hello NTFS!".
fn build_resident_data_fixture() -> Vec<u8> {
    let mft_cluster = 2u64;
    let mft_record_size = 1024usize;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    let total = rec6_off + mft_record_size + 512;
    let mut data = vec![0u8; total];

    // Boot
    make_boot(&mut data[0..512]);

    // Root directory (inode 5) — one child: README.TXT (inode 6)
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let (ent, _) = build_indx_entries(&["README.TXT"], 6, false);
    let mut off = iro + 0x20;
    rec5[off..off + ent.len()].copy_from_slice(&ent);
    off += ent.len();
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    // File record (inode 6) — with resident $DATA
    let rec6 = &mut data[rec6_off..rec6_off + mft_record_size];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    write_resident_data(rec6, 0x68, b"Hello NTFS!");

    data
}

/// Build a fixture with a non-resident $DATA attribute for "large.bin".
fn build_nonresident_data_fixture() -> Vec<u8> {
    let cluster_size = 512usize;
    let mft_cluster = 2u64;
    let mft_record_size = 1024usize;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;

    // File data at cluster 32
    let data_cluster = 32u64;
    let data_offset = data_cluster as usize * cluster_size;
    let file_content = b"This is non-resident data spanning one cluster.";
    let total = (data_offset + cluster_size).max(rec6_off + mft_record_size + 512);
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    // Root → large.bin (inode 6)
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let (ent, _) = build_indx_entries(&["large.bin"], 6, false);
    let mut off = iro + 0x20;
    rec5[off..off + ent.len()].copy_from_slice(&ent);
    off += ent.len();
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    // File record (inode 6) with non-resident $DATA
    let rec6 = &mut data[rec6_off..rec6_off + mft_record_size];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    let idxa = 0x68usize;
    rec6[idxa..idxa + 4].copy_from_slice(&0x80u32.to_le_bytes()); // $DATA
    rec6[idxa + 8] = 1; // non-resident
    rec6[idxa + 0x20..idxa + 0x22].copy_from_slice(&0x40u16.to_le_bytes());
    rec6[idxa + 0x28..idxa + 0x30].copy_from_slice(&(cluster_size as u64).to_le_bytes());
    rec6[idxa + 0x30..idxa + 0x38].copy_from_slice(&(file_content.len() as u64).to_le_bytes());
    // Data run: 1 cluster at LCN 32
    let run = idxa + 0x40;
    rec6[run] = 0x31; // size=1, offset=3
    rec6[run + 1] = 1; // 1 cluster
    rec6[run + 2..run + 5].copy_from_slice(&32u64.to_le_bytes()[..3]);
    rec6[run + 5] = 0x00;
    let idxa_len = (run + 6 - idxa) as u32;
    rec6[idxa + 4..idxa + 8].copy_from_slice(&idxa_len.to_le_bytes());

    // Write file content at the data cluster
    data[data_offset..data_offset + file_content.len()].copy_from_slice(file_content);

    data
}

/// Build a fixture where the base record has only $ATTRIBUTE_LIST and the
/// unnamed $DATA stream lives in an extension MFT record.
fn build_attribute_list_external_data_fixture() -> Vec<u8> {
    let cluster_size = 512usize;
    let mft_cluster = 2u64;
    let mft_record_size = 1024usize;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    let rec7_off = mft_cluster as usize * 512 + 7 * mft_record_size;

    let data_cluster = 40u64;
    let data_offset = data_cluster as usize * cluster_size;
    let file_content = b"external-attribute-list-data-across-extension-record";
    let total = (data_offset + cluster_size).max(rec7_off + mft_record_size + 512);
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    // Root -> listed.bin (inode 6)
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let (ent, _) = build_indx_entries(&["listed.bin"], 6, false);
    let mut off = iro + 0x20;
    rec5[off..off + ent.len()].copy_from_slice(&ent);
    off += ent.len();
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    // Base file record (inode 6) with resident $ATTRIBUTE_LIST only.
    let rec6 = &mut data[rec6_off..rec6_off + mft_record_size];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    let attr_list = 0x68usize;
    let content_off = 0x18u16;
    let entry_len = 0x20u16;
    let attr_len = content_off as usize + entry_len as usize;
    rec6[attr_list..attr_list + 4].copy_from_slice(&0x20u32.to_le_bytes());
    rec6[attr_list + 4..attr_list + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    rec6[attr_list + 8] = 0;
    rec6[attr_list + 0x10..attr_list + 0x14].copy_from_slice(&(entry_len as u32).to_le_bytes());
    rec6[attr_list + 0x14..attr_list + 0x16].copy_from_slice(&content_off.to_le_bytes());
    let list_entry = attr_list + content_off as usize;
    rec6[list_entry..list_entry + 4].copy_from_slice(&0x80u32.to_le_bytes());
    rec6[list_entry + 4..list_entry + 6].copy_from_slice(&entry_len.to_le_bytes());
    rec6[list_entry + 0x10..list_entry + 0x18].copy_from_slice(&7u64.to_le_bytes());
    rec6[list_entry + 0x18..list_entry + 0x1a].copy_from_slice(&1u16.to_le_bytes());
    rec6[attr_list + attr_len..attr_list + attr_len + 4]
        .copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());

    // Extension record (inode 7) points back to base inode 6 and owns $DATA.
    let rec7 = &mut data[rec7_off..rec7_off + mft_record_size];
    rec7[0..4].copy_from_slice(b"FILE");
    rec7[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec7[0x20..0x28].copy_from_slice(&6u64.to_le_bytes());
    rec7[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec7[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    let data_attr = 0x68usize;
    rec7[data_attr..data_attr + 4].copy_from_slice(&0x80u32.to_le_bytes());
    rec7[data_attr + 8] = 1;
    rec7[data_attr + 0x0E..data_attr + 0x10].copy_from_slice(&1u16.to_le_bytes());
    rec7[data_attr + 0x20..data_attr + 0x22].copy_from_slice(&0x40u16.to_le_bytes());
    rec7[data_attr + 0x28..data_attr + 0x30].copy_from_slice(&(cluster_size as u64).to_le_bytes());
    rec7[data_attr + 0x30..data_attr + 0x38]
        .copy_from_slice(&(file_content.len() as u64).to_le_bytes());
    let run = data_attr + 0x40;
    rec7[run] = 0x31;
    rec7[run + 1] = 1;
    rec7[run + 2..run + 5].copy_from_slice(&data_cluster.to_le_bytes()[..3]);
    rec7[run + 5] = 0;
    rec7[data_attr + 4..data_attr + 8]
        .copy_from_slice(&((run + 6 - data_attr) as u32).to_le_bytes());
    rec7[run + 6..run + 10].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());

    data[data_offset..data_offset + file_content.len()].copy_from_slice(file_content);

    data
}

/// Build a fixture where root's named `$I30/$INDEX_ALLOCATION` stream lives
/// in an extension MFT record referenced by `$ATTRIBUTE_LIST`.
fn build_external_index_allocation_fixture() -> Vec<u8> {
    let cluster_size = 512usize;
    let mft_cluster = 2u64;
    let record_size = 1024usize;
    let root_offset = mft_cluster as usize * cluster_size + 5 * record_size;
    let extension_offset = mft_cluster as usize * cluster_size + 7 * record_size;
    let index_cluster = 40u64;
    let index_offset = index_cluster as usize * cluster_size;
    let (entries, _) = build_indx_entries(&["SYSTEM", "SOFTWARE"], 100, false);
    let index_record = build_indx_record(&entries, 2);
    let mut data = vec![0u8; index_offset + index_record.len()];
    make_boot(&mut data[0..512]);

    let root = &mut data[root_offset..root_offset + record_size];
    root[0..4].copy_from_slice(b"FILE");
    root[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    root[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    root[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());

    let attr_list = 0x68usize;
    let list_content_offset = 0x18usize;
    let list_entry_len = 0x28usize;
    let list_attr_len = list_content_offset + list_entry_len * 2;
    root[attr_list..attr_list + 4].copy_from_slice(&0x20u32.to_le_bytes());
    root[attr_list + 4..attr_list + 8].copy_from_slice(&(list_attr_len as u32).to_le_bytes());
    root[attr_list + 0x10..attr_list + 0x14]
        .copy_from_slice(&((list_entry_len * 2) as u32).to_le_bytes());
    root[attr_list + 0x14..attr_list + 0x16]
        .copy_from_slice(&(list_content_offset as u16).to_le_bytes());
    let entry = attr_list + list_content_offset;
    root[entry..entry + 4].copy_from_slice(&0xA0u32.to_le_bytes());
    root[entry + 4..entry + 6].copy_from_slice(&(list_entry_len as u16).to_le_bytes());
    root[entry + 6] = 4;
    root[entry + 7] = 0x1A;
    root[entry + 0x10..entry + 0x18].copy_from_slice(&7u64.to_le_bytes());
    for (index, ch) in "$I30".encode_utf16().enumerate() {
        root[entry + 0x1A + index * 2..entry + 0x1C + index * 2].copy_from_slice(&ch.to_le_bytes());
    }
    let bitmap_entry = entry + list_entry_len;
    root[bitmap_entry..bitmap_entry + 4].copy_from_slice(&0xB0u32.to_le_bytes());
    root[bitmap_entry + 4..bitmap_entry + 6]
        .copy_from_slice(&(list_entry_len as u16).to_le_bytes());
    root[bitmap_entry + 6] = 4;
    root[bitmap_entry + 7] = 0x1A;
    root[bitmap_entry + 0x10..bitmap_entry + 0x18].copy_from_slice(&7u64.to_le_bytes());
    root[bitmap_entry + 0x18..bitmap_entry + 0x1A].copy_from_slice(&1u16.to_le_bytes());
    for (index, ch) in "$I30".encode_utf16().enumerate() {
        root[bitmap_entry + 0x1A + index * 2..bitmap_entry + 0x1C + index * 2]
            .copy_from_slice(&ch.to_le_bytes());
    }
    root[attr_list + list_attr_len..attr_list + list_attr_len + 4]
        .copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let extension = &mut data[extension_offset..extension_offset + record_size];
    extension[0..4].copy_from_slice(b"FILE");
    extension[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    extension[0x20..0x28].copy_from_slice(&5u64.to_le_bytes());
    extension[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    extension[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let allocation = 0x68usize;
    extension[allocation..allocation + 4].copy_from_slice(&0xA0u32.to_le_bytes());
    extension[allocation + 8] = 1;
    extension[allocation + 9] = 4;
    extension[allocation + 0x0A..allocation + 0x0C].copy_from_slice(&0x40u16.to_le_bytes());
    extension[allocation + 0x18..allocation + 0x20].copy_from_slice(&1u64.to_le_bytes());
    extension[allocation + 0x20..allocation + 0x22].copy_from_slice(&0x48u16.to_le_bytes());
    extension[allocation + 0x28..allocation + 0x30]
        .copy_from_slice(&(index_record.len() as u64).to_le_bytes());
    extension[allocation + 0x30..allocation + 0x38]
        .copy_from_slice(&(index_record.len() as u64).to_le_bytes());
    for (index, ch) in "$I30".encode_utf16().enumerate() {
        extension[allocation + 0x40 + index * 2..allocation + 0x42 + index * 2]
            .copy_from_slice(&ch.to_le_bytes());
    }
    let run = allocation + 0x48;
    extension[run] = 0x11;
    extension[run + 1] = 2;
    extension[run + 2] = index_cluster as u8;
    extension[run + 3] = 0;
    extension[allocation + 4..allocation + 8]
        .copy_from_slice(&((run + 4 - allocation) as u32).to_le_bytes());
    write_resident_index_bitmap(extension, run + 4, Some("$I30"), 1);
    data[index_offset..index_offset + index_record.len()].copy_from_slice(&index_record);
    data
}

/// Boot sector helper for Phase 17 fixtures.
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

#[test]
fn read_resident_file_data() {
    let img = build_resident_data_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let mut file = ntfs.open_file("README.TXT").unwrap();
    let mut buf = String::new();
    file.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "Hello NTFS!");
}

#[test]
fn read_nonresident_file_data() {
    let img = build_nonresident_data_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let mut file = ntfs.open_file("large.bin").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"This is non-resident data spanning one cluster.");
}

#[test]
fn stream_nonresident_file_supports_bounded_reads_and_seeks() {
    use std::io::{Read, Seek, SeekFrom};

    let img = build_nonresident_data_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    assert!(ntfs.supports_file_stream_by_inode(6).unwrap());

    let mut file = ntfs.into_file_stream_by_inode(6).unwrap();
    let mut prefix = [0_u8; 7];
    file.read_exact(&mut prefix).unwrap();
    assert_eq!(&prefix, b"This is");

    file.seek(SeekFrom::End(-8)).unwrap();
    let mut suffix = Vec::new();
    file.read_to_end(&mut suffix).unwrap();
    assert_eq!(suffix, b"cluster.");
}

#[test]
fn read_external_attribute_list_data_by_inode_and_range() {
    let img = build_attribute_list_external_data_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let expected = b"external-attribute-list-data-across-extension-record";
    assert_eq!(
        ntfs.file_size_by_inode(6).unwrap(),
        Some(expected.len() as u64)
    );

    let mut file = ntfs.open_file("mft:6").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, expected);

    let first = ntfs.read_file_range_by_inode(6, 0, 8).unwrap();
    assert_eq!(first, b"external");

    let middle = ntfs.read_file_range_by_inode(6, 19, 9).unwrap();
    assert_eq!(middle, b"list-data");
}

#[test]
fn attribute_list_sequence_mismatch_is_not_treated_as_empty_data() {
    let mut img = build_attribute_list_external_data_fixture();
    let rec7_offset = 2 * 512 + 7 * 1024;
    img[rec7_offset + 0x10..rec7_offset + 0x12].copy_from_slice(&1u16.to_le_bytes());
    let ntfs = NtfsReader::open(Box::new(FakeReader::new(img)), 0).unwrap();

    let error = ntfs.file_size_by_inode(6).unwrap_err();

    assert!(error.to_string().contains("FILE sequence mismatch"));
}

#[test]
fn attribute_list_base_reference_mismatch_is_not_treated_as_empty_data() {
    let mut img = build_attribute_list_external_data_fixture();
    let rec7_offset = 2 * 512 + 7 * 1024;
    img[rec7_offset + 0x20..rec7_offset + 0x28].copy_from_slice(&99u64.to_le_bytes());
    let ntfs = NtfsReader::open(Box::new(FakeReader::new(img)), 0).unwrap();

    let error = ntfs.file_size_by_inode(6).unwrap_err();

    assert!(error.to_string().contains("mismatched base reference"));
}

#[test]
fn attribute_list_instance_mismatch_is_not_treated_as_empty_data() {
    let mut img = build_attribute_list_external_data_fixture();
    let rec7_offset = 2 * 512 + 7 * 1024;
    let data_attribute = rec7_offset + 0x68;
    img[data_attribute + 0x0e..data_attribute + 0x10].copy_from_slice(&2u16.to_le_bytes());
    let ntfs = NtfsReader::open(Box::new(FakeReader::new(img)), 0).unwrap();

    let error = ntfs.file_size_by_inode(6).unwrap_err();

    assert!(error.to_string().contains("identity was not found"));
}

#[test]
fn malformed_attribute_list_is_not_treated_as_absent_data() {
    let mut img = build_attribute_list_external_data_fixture();
    let rec6_offset = 2 * 512 + 6 * 1024;
    let list_entry = rec6_offset + 0x68 + 0x18;
    img[list_entry + 4..list_entry + 6].copy_from_slice(&0u16.to_le_bytes());
    let ntfs = NtfsReader::open(Box::new(FakeReader::new(img)), 0).unwrap();

    let error = ntfs.file_size_by_inode(6).unwrap_err();

    assert!(error.to_string().contains("entry length"));
}

#[test]
fn malformed_attribute_list_content_range_is_rejected() {
    let mut img = build_attribute_list_external_data_fixture();
    let rec6_offset = 2 * 512 + 6 * 1024;
    let attr_list = rec6_offset + 0x68;
    img[attr_list + 0x14..attr_list + 0x16].copy_from_slice(&0xFFFFu16.to_le_bytes());
    let ntfs = NtfsReader::open(Box::new(FakeReader::new(img)), 0).unwrap();

    let error = ntfs.file_size_by_inode(6).unwrap_err();

    assert!(error.to_string().contains("content range"));
}

#[test]
fn truncated_nonresident_attribute_list_header_is_rejected() {
    let mut img = build_attribute_list_external_data_fixture();
    let rec6_offset = 2 * 512 + 6 * 1024;
    let attr_list = rec6_offset + 0x68;
    img[attr_list + 8] = 1;
    let ntfs = NtfsReader::open(Box::new(FakeReader::new(img)), 0).unwrap();

    let error = ntfs.file_size_by_inode(6).unwrap_err();

    assert!(error.to_string().contains("header is truncated"));
}

#[test]
fn resident_empty_data_is_distinct_from_absent_data() {
    let mut img = build_resident_data_fixture();
    let rec6_offset = 2 * 512 + 6 * 1024;
    let record = &mut img[rec6_offset..rec6_offset + 1024];
    let mut position = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while u32::from_le_bytes(record[position..position + 4].try_into().unwrap()) != 0x80 {
        position +=
            u32::from_le_bytes(record[position + 4..position + 8].try_into().unwrap()) as usize;
    }
    record[position + 0x10..position + 0x14].copy_from_slice(&0u32.to_le_bytes());
    let ntfs = NtfsReader::open(Box::new(FakeReader::new(img)), 0).unwrap();

    assert_eq!(ntfs.file_size_by_inode(6).unwrap(), Some(0));
    assert_eq!(ntfs.file_size_by_inode(5).unwrap(), None);
}

#[test]
fn list_external_named_index_allocation_entries() {
    let img = build_external_index_allocation_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let entries = ntfs.list_root_directory_entries().unwrap();
    let names = entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["SYSTEM", "SOFTWARE"]);
}

#[test]
fn open_file_nested_path() {
    // Build: root → Windows (6) → System32 (7) → ntdll.dll (8)
    let _cluster_size = 512;
    let mft_record_size = 1024usize;
    let mft_cluster = 2u64;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    let rec7_off = mft_cluster as usize * 512 + 7 * mft_record_size;
    let rec8_off = mft_cluster as usize * 512 + 8 * mft_record_size;
    let total = rec8_off + mft_record_size + 1024;
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    // Helper: directory record
    let mut write_dir = |rec_off: usize, children: &[(&str, u64)]| {
        let rec = &mut data[rec_off..rec_off + mft_record_size];
        rec[0..4].copy_from_slice(b"FILE");
        rec[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
        rec[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
        rec[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
        let iro = 0x68usize;
        rec[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
        rec[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
        let mut off = iro + 0x20;
        for &(name, mft_ref) in children {
            let utf16: Vec<u16> = name.encode_utf16().collect();
            let name_bytes = utf16.len() * 2;
            let entry_size = 0x52 + name_bytes;
            rec[off..off + 8].copy_from_slice(&mft_ref.to_le_bytes());
            rec[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
            rec[off + 0x48..off + 0x4C].copy_from_slice(&0x10000000u32.to_le_bytes());
            rec[off + 0x50] = utf16.len() as u8;
            for (i, c) in utf16.iter().enumerate() {
                rec[off + 0x52 + i * 2..off + 0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
            }
            off += entry_size;
        }
        rec[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        off += 4;
        rec[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());
    };

    write_dir(rec5_off, &[("Windows", 6)]);
    write_dir(rec6_off, &[("System32", 7)]);
    // System32 dir → ntdll.dll (file, not dir)
    {
        let rec7 = &mut data[rec7_off..rec7_off + mft_record_size];
        rec7[0..4].copy_from_slice(b"FILE");
        rec7[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
        rec7[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
        rec7[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
        let iro = 0x68usize;
        rec7[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
        rec7[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
        // file entry: no directory flag
        let utf16: Vec<u16> = "ntdll.dll".encode_utf16().collect();
        let name_bytes = utf16.len() * 2;
        let entry_size = 0x52 + name_bytes;
        let mut off = iro + 0x20;
        rec7[off..off + 8].copy_from_slice(&8u64.to_le_bytes()); // mft_ref
        rec7[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
        rec7[off + 0x50] = utf16.len() as u8;
        for (i, c) in utf16.iter().enumerate() {
            rec7[off + 0x52 + i * 2..off + 0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        off += entry_size;
        rec7[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        off += 4;
        rec7[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());
    }

    // File record (inode 8) with resident $DATA
    let rec8 = &mut data[rec8_off..rec8_off + mft_record_size];
    rec8[0..4].copy_from_slice(b"FILE");
    rec8[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec8[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec8[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    write_resident_data(rec8, 0x68, b"MZ\x90\x00");

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let mut file = ntfs.open_file("\\Windows\\System32\\ntdll.dll").unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    assert_eq!(buf, b"MZ\x90\x00");
}

#[test]
fn open_nonexistent_file_errors() {
    let img = build_resident_data_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let result = ntfs.open_file("NONEXIST.TXT");
    assert!(result.is_err());
}

#[test]
fn list_children_returns_files_and_dirs() {
    // Root with one file and one subdirectory
    let mft_record_size = 1024usize;
    let mft_cluster = 2u64;
    let _cluster_size = 512usize;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    let rec7_off = mft_cluster as usize * 512 + 7 * mft_record_size;
    let total = rec7_off + mft_record_size + 1024;
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    // Write directory with one file entry (not is_dir)
    let write_entry = |rec: &mut [u8], name: &str, mft_ref: u64, is_dir: bool| {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let name_bytes = utf16.len() * 2;
        let entry_size = 0x52 + name_bytes;
        rec[0..8].copy_from_slice(&mft_ref.to_le_bytes());
        rec[8..10].copy_from_slice(&(entry_size as u16).to_le_bytes());
        if is_dir {
            rec[0x48..0x4C].copy_from_slice(&0x10000000u32.to_le_bytes());
        }
        rec[0x50] = utf16.len() as u8;
        for (i, c) in utf16.iter().enumerate() {
            rec[0x52 + i * 2..0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
    };

    // Root directory: "SubDir" (dir, inode 6) + "notes.txt" (file, inode 7)
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let mut off = iro + 0x20;
    write_entry(&mut rec5[off..], "SubDir", 6, true);
    off += 0x52 + "SubDir".encode_utf16().count() * 2;
    write_entry(&mut rec5[off..], "notes.txt", 7, false);
    off += 0x52 + "notes.txt".encode_utf16().count() * 2;
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    // SubDir record (inode 6) — with one child file
    let rec6 = &mut data[rec6_off..rec6_off + mft_record_size];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro6 = 0x68usize;
    rec6[iro6..iro6 + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec6[iro6 + 0x10..iro6 + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let (ent, _) = build_indx_entries(&["deep.txt"], 100, true);
    off = iro6 + 0x20;
    rec6[off..off + ent.len()].copy_from_slice(&ent);
    off += ent.len();
    rec6[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec6[iro6 + 4..iro6 + 8].copy_from_slice(&((off - iro6) as u32).to_le_bytes());

    // notes.txt record (inode 7) with resident $DATA
    let rec7 = &mut data[rec7_off..rec7_off + mft_record_size];
    rec7[0..4].copy_from_slice(b"FILE");
    rec7[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec7[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec7[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    write_resident_data(rec7, 0x68, b"file content here");

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data));
    let ntfs = NtfsReader::open(reader, 0).unwrap();

    // List root: should have 2 children (SubDir + notes.txt)
    let root_children = ntfs.list_children("").unwrap();
    assert_eq!(root_children.len(), 2);
    let subdir = root_children.iter().find(|n| n.name == "SubDir").unwrap();
    assert!(subdir.is_dir);
    let notes = root_children
        .iter()
        .find(|n| n.name == "notes.txt")
        .unwrap();
    assert!(!notes.is_dir);

    // Open notes.txt
    let mut file = ntfs.open_file("notes.txt").unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    assert_eq!(content, "file content here");

    // List SubDir children
    let sub_children = ntfs.list_children("SubDir").unwrap();
    assert_eq!(sub_children.len(), 1);
    assert_eq!(sub_children[0].name, "deep.txt");
    assert!(sub_children[0].is_dir);
}

// --- Phase 19: robustness tests ---

#[test]
fn malformed_record_no_panic() {
    // Feed random/garbage bytes to list_root_children — must not panic
    let garbage = vec![0xFFu8; 4096];
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(garbage));
    // open may fail (no valid boot sector), which is OK
    if let Ok(ntfs) = NtfsReader::open(reader, 0) {
        let _ = ntfs.list_root_children();
        let _ = ntfs.list_subdir_children("anything");
    }
}

#[test]
fn par_ref_mismatch_returns_none() {
    // Build: root → "DirA" (inode 6, but par_ref says parent is 99)
    let mft_record_size = 1024usize;
    let mft_cluster = 2u64;
    let _cluster_size = 512usize;
    let rec5_off = mft_cluster as usize * 512 + 5 * mft_record_size;
    let rec6_off = mft_cluster as usize * 512 + 6 * mft_record_size;
    let total = rec6_off + mft_record_size + 1024;
    let mut data = vec![0u8; total];

    make_boot(&mut data[0..512]);

    // Root → DirA (inode 6)
    let rec5 = &mut data[rec5_off..rec5_off + mft_record_size];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    // INDX entry for DirA
    let utf16: Vec<u16> = "DirA".encode_utf16().collect();
    let name_bytes = utf16.len() * 2;
    let entry_size = 0x52 + name_bytes;
    let mut off = iro + 0x20;
    rec5[off..off + 8].copy_from_slice(&6u64.to_le_bytes());
    rec5[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
    rec5[off + 0x48..off + 0x4C].copy_from_slice(&0x10000000u32.to_le_bytes());
    rec5[off + 0x50] = utf16.len() as u8;
    for (i, c) in utf16.iter().enumerate() {
        rec5[off + 0x52 + i * 2..off + 0x52 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    off += entry_size;
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    // DirA record (inode 6) — $FILE_NAME with par_ref=99 (WRONG: should be 5)
    let rec6 = &mut data[rec6_off..rec6_off + mft_record_size];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let end = write_resident_file_name(rec6, 0x68, 99, "DirA");
    rec6[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(data));
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    // resolve_path should detect the par_ref mismatch and return None
    let result = ntfs.list_subdir_children("DirA").unwrap();
    assert!(
        result.is_empty(),
        "par_ref mismatch should make DirA unreachable"
    );
}

#[test]
fn open_file_truncated_record_no_panic() {
    // Corrupted fixture — truncated MFT record for README.TXT
    // Read should gracefully fail instead of panicking
    let img = build_resident_data_fixture();
    // Corrupt: shorten the file to truncate the README.TXT record
    let corrupted: Vec<u8> = img[..img.len() - 900].to_vec();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(corrupted));
    if let Ok(ntfs) = NtfsReader::open(reader, 0) {
        let _ = ntfs.open_file("README.TXT");
        let _ = ntfs.list_root_children();
    }
}

/// Build a minimal NTFS fixture with an EFS-encrypted file.
/// Sets FILE_ATTRIBUTE_ENCRYPTED (0x4000) on the INDX entry flags.
fn build_efs_fixture() -> Vec<u8> {
    let bps = 512u16;
    let spc = 1u8;
    let mft_cluster = 2u64;

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
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes());

    // MFT record 5 — root directory
    let rec5 = &mut data[rec5_offset..];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());

    // $STANDARD_INFORMATION (0x10) — minimal
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    rec5[0x68..0x6C].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());

    // $INDEX_ROOT (0x90)
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 4..iro + 8].copy_from_slice(&0u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());

    // INDX entries: one plain text file + one EFS-encrypted file
    let entries: &[(&str, u32)] = &[("plain.txt", 0), ("secret.docx", 0x4000)];
    let mut off = iro + 0x20;
    for (i, &(name, extra_flags)) in entries.iter().enumerate() {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let name_bytes = utf16.len() * 2;
        let entry_size = 0x52 + name_bytes;
        let mft_ref = 100u64 + i as u64;

        rec5[off..off + 8].copy_from_slice(&mft_ref.to_le_bytes());
        rec5[off + 8..off + 10].copy_from_slice(&(entry_size as u16).to_le_bytes());
        rec5[off + 0x50] = utf16.len() as u8;
        // Set flags: not a directory, but with extra_flags (e.g. 0x4000 for EFS)
        rec5[off + 0x48..off + 0x4C].copy_from_slice(&extra_flags.to_le_bytes());
        for (j, c) in utf16.iter().enumerate() {
            rec5[off + 0x52 + j * 2..off + 0x52 + j * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        off += entry_size;
    }
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    data
}

#[test]
fn efs_encrypted_file_detected() {
    let img = build_efs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let ntfs = NtfsReader::open(reader, 0).expect("open NTFS");
    let nodes = ntfs.list_root_children().expect("list_root_children");

    assert_eq!(nodes.len(), 2, "expected 2 nodes");

    let plain = nodes
        .iter()
        .find(|n| n.name == "plain.txt")
        .expect("plain.txt missing");
    assert!(!plain.encrypted, "plain.txt should not be encrypted");

    let secret = nodes
        .iter()
        .find(|n| n.name == "secret.docx")
        .expect("secret.docx missing");
    assert!(
        secret.encrypted,
        "secret.docx should be flagged as encrypted"
    );
}
