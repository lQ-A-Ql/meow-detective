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
fn list_children_subdir_returns_empty() {
    let img = build_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
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
    boot[0] = 0xEB; boot[1] = 0x52; boot[2] = 0x90;
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
        rec[iro..iro+4].copy_from_slice(&0x90u32.to_le_bytes());
        rec[iro+4..iro+8].copy_from_slice(&0u32.to_le_bytes()); // patched later
        rec[iro+0x10..iro+0x14].copy_from_slice(&0x10u32.to_le_bytes());

        let mut off = iro + 0x20;
        for &(name, mft_ref) in children {
            let utf16: Vec<u16> = name.encode_utf16().collect();
            let name_bytes = utf16.len() * 2;
            let entry_size = 0x52 + name_bytes;
            rec[off..off+8].copy_from_slice(&mft_ref.to_le_bytes());
            rec[off+8..off+10].copy_from_slice(&(entry_size as u16).to_le_bytes());
            rec[off+0x50] = utf16.len() as u8;
            rec[off+0x48..off+0x4C].copy_from_slice(&0x10000000u32.to_le_bytes());
            for (i, c) in utf16.iter().enumerate() {
                rec[off+0x52+i*2..off+0x52+i*2+2].copy_from_slice(&c.to_le_bytes());
            }
            off += entry_size;
        }
        rec[off..off+4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        rec[iro+4..iro+8].copy_from_slice(&((off - iro) as u32).to_le_bytes());
    }

    // Root → [Windows (inode 6)]
    write_dir_record(&mut data[rec5_off..rec5_off+mft_record_size], &[("Windows", 6)]);
    // Windows → [System32 (inode 7)]
    write_dir_record(&mut data[rec6_off..rec6_off+mft_record_size], &[("System32", 7)]);
    // System32 → [ntdll.dll]
    write_dir_record(&mut data[rec7_off..rec7_off+mft_record_size], &[("ntdll.dll", 100)]);

    data
}

#[test]
fn resolve_nested_path_lists_children() {
    let img = build_nested_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
    let ntfs = NtfsReader::open(reader, 0).unwrap();

    let nodes = ntfs.list_subdir_children("\\Windows\\System32").unwrap();
    assert_eq!(nodes.len(), 1, "System32 should have 1 child");
    assert_eq!(nodes[0].name, "ntdll.dll");
}

#[test]
fn wrong_path_returns_empty() {
    let img = build_nested_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
    let ntfs = NtfsReader::open(reader, 0).unwrap();

    // "System32" exists but only under "Windows" — bare name should fail
    let nodes = ntfs.list_subdir_children("System32").unwrap();
    assert!(nodes.is_empty(), "bare System32 should not resolve");
}

// --- Phase 16: $INDEX_ALLOCATION tests ---

/// Build INDX entries as bytes. Returns (data, entry_count).
fn build_indx_entries(names: &[&str], base_mft_ref: u64) -> (Vec<u8>, usize) {
    let mut data = Vec::new();
    for (i, &name) in names.iter().enumerate() {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let name_bytes = utf16.len() * 2;
        let entry_size = 0x52 + name_bytes;
        let mut entry = vec![0u8; entry_size];
        // MFT ref: lower 48 bits
        let mft_ref = base_mft_ref + i as u64;
        entry[0..8].copy_from_slice(&mft_ref.to_le_bytes());
        // entry size @ +0x08
        entry[8..10].copy_from_slice(&(entry_size as u16).to_le_bytes());
        // flags @ +0x48 (directory)
        entry[0x48..0x4C].copy_from_slice(&0x10000000u32.to_le_bytes());
        // name_len @ +0x50
        entry[0x50] = utf16.len() as u8;
        // name @ +0x52
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
    let ent_total = entries_data.len() as u32;
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

    let (root_ent_data, _) = build_indx_entries(&["Alpha", "Beta", "Gamma"], 10);
    let (alloc_ent_data, _) = build_indx_entries(
        &["One", "Two", "Three", "Four", "Five"],
        100,
    );
    let indx_rec = build_indx_record(&alloc_ent_data, 3); // 3 sectors = 1536 bytes

    let total = indx_offset + indx_rec.len().max(2048);
    let mut data = vec![0u8; total];

    // Boot sector
    let boot = &mut data[0..512];
    boot[0] = 0xEB; boot[1] = 0x52; boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&bps.to_le_bytes());
    boot[13] = spc;
    boot[0x30..0x38].copy_from_slice(&mft_cluster.to_le_bytes());
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes());

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

    // Write INDX record at the data run target
    data[indx_offset..indx_offset + indx_rec.len()].copy_from_slice(&indx_rec);

    data
}

#[test]
fn list_root_with_index_alloc_returns_all_entries() {
    let img = build_index_alloc_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
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
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data: img, pos: 0 });
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
    let (entries_data, _) = build_indx_entries(&["FileA"], 1);
    let indx_rec = build_indx_record(&entries_data, 1);

    let mut data = vec![0u8; indx_offset + 1024];
    let boot = &mut data[0..512];
    boot[0] = 0xEB; boot[1] = 0x52; boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&bps.to_le_bytes());
    boot[13] = spc;
    boot[0x30..0x38].copy_from_slice(&mft_cluster.to_le_bytes());
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes());

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

    data[indx_offset..indx_offset + indx_rec.len()].copy_from_slice(&indx_rec);

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader { data, pos: 0 });
    let ntfs = NtfsReader::open(reader, 0).unwrap();
    let nodes = ntfs.list_root_children().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, "FileA");
}
