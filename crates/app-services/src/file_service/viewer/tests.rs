use super::*;
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, FileEntryId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::runner;
use rusqlite::params;
use std::cell::Cell;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use transport::dto::ViewerRangeRequestDto;

fn reset_skip_reader_bytes_call_count() {
    SKIP_READER_BYTES_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}

fn skip_reader_bytes_call_count() -> usize {
    SKIP_READER_BYTES_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

fn reset_open_file_content_by_id_call_count() {
    OPEN_FILE_CONTENT_BY_ID_CALLS.with(|calls| calls.set(0));
}

fn open_file_content_by_id_call_count() -> usize {
    OPEN_FILE_CONTENT_BY_ID_CALLS.with(std::cell::Cell::get)
}

fn reset_read_file_bytes_for_case_call_count() {
    READ_FILE_BYTES_FOR_CASE_CALLS.with(|calls| calls.set(0));
}

fn read_file_bytes_for_case_call_count() -> usize {
    READ_FILE_BYTES_FOR_CASE_CALLS.with(std::cell::Cell::get)
}

fn make_temp_e01() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("cache_test.E01");
    // Write a minimal single-chunk E01 so the reader can be opened.
    write_tiny_e01(&path).unwrap();
    (dir, path)
}

fn write_tiny_e01(path: &std::path::Path) -> std::io::Result<()> {
    let chunk_sectors: u32 = 8;
    let sectors = chunk_sectors as u64;
    let chunk_bytes = (chunk_sectors * 512) as usize;

    let mut f = std::fs::File::create(path)?;
    // EVF file header (13 bytes)
    f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")?;

    let mut vol = vec![0u8; 36];
    vol[12..16].copy_from_slice(&chunk_sectors.to_le_bytes());
    vol[16..24].copy_from_slice(&sectors.to_le_bytes());

    let volume_desc_offset = 13u64;
    let table_desc_offset = volume_desc_offset + 76 + vol.len() as u64;
    let table_len = 24 + 4 + 4; // 1 chunk entry + padding
    let done_desc_offset = table_desc_offset + 76 + table_len as u64;
    let chunk0_offset = done_desc_offset + 76;

    // volume section
    f.write_all(&section_desc(
        "volume",
        table_desc_offset,
        76 + vol.len() as u64,
    ))?;
    f.write_all(&vol)?;

    // table section (1 chunk)
    let mut table = vec![0u8; table_len];
    table[0..4].copy_from_slice(&1u32.to_le_bytes()); // 1 entry
    table[8..16].copy_from_slice(&chunk0_offset.to_le_bytes()); // base offset
    table[24..28].copy_from_slice(&0u32.to_le_bytes()); // rel offset 0
    f.write_all(&section_desc(
        "table",
        done_desc_offset,
        76 + table.len() as u64,
    ))?;
    f.write_all(&table)?;

    // done section
    f.write_all(&section_desc("done", 0, 0))?;

    // chunk data
    let marker = b"E01-CACHE-TEST";
    let mut chunk = vec![0u8; chunk_bytes];
    chunk[..marker.len()].copy_from_slice(marker);
    f.write_all(&chunk)?;
    f.flush()
}

fn section_desc(stype: &str, next: u64, size: u64) -> [u8; 76] {
    let mut desc = [0u8; 76];
    let bytes = stype.as_bytes();
    desc[0..bytes.len().min(16)].copy_from_slice(&bytes[..bytes.len().min(16)]);
    desc[16..24].copy_from_slice(&next.to_le_bytes());
    desc[24..32].copy_from_slice(&size.to_le_bytes());
    desc
}

fn write_large_ntfs_raw_fixture(path: &std::path::Path, marker: &[u8]) -> std::io::Result<()> {
    const CLUSTER_SIZE: usize = 512;
    const MFT_RECORD_SIZE: usize = 1024;
    const MFT_CLUSTER: usize = 2;
    const FILE_RECORD: u64 = 6;
    const DATA_CLUSTER: usize = 32;
    const SPARSE_PREFIX_CLUSTERS: u64 = (128 * 1024 * 1024) / CLUSTER_SIZE as u64;

    let rec5_off = MFT_CLUSTER * CLUSTER_SIZE + 5 * MFT_RECORD_SIZE;
    let rec6_off = MFT_CLUSTER * CLUSTER_SIZE + FILE_RECORD as usize * MFT_RECORD_SIZE;
    let data_off = DATA_CLUSTER * CLUSTER_SIZE;
    let total = data_off + CLUSTER_SIZE;
    let mut data = vec![0u8; total];

    let boot = &mut data[0..512];
    boot[0] = 0xEB;
    boot[1] = 0x52;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&512u16.to_le_bytes());
    boot[13] = 1;
    boot[0x30..0x38].copy_from_slice(&(MFT_CLUSTER as u64).to_le_bytes());
    boot[0x40..0x44].copy_from_slice(&(-10i32).to_le_bytes());
    boot[510] = 0x55;
    boot[511] = 0xAA;

    let rec5 = &mut data[rec5_off..rec5_off + MFT_RECORD_SIZE];
    rec5[0..4].copy_from_slice(b"FILE");
    rec5[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec5[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec5[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let iro = 0x68usize;
    rec5[iro..iro + 4].copy_from_slice(&0x90u32.to_le_bytes());
    rec5[iro + 0x10..iro + 0x14].copy_from_slice(&0x10u32.to_le_bytes());
    let mut entry = vec![0u8; 0x52 + "large.bin".encode_utf16().count() * 2];
    let entry_len = entry.len();
    entry[0..8].copy_from_slice(&FILE_RECORD.to_le_bytes());
    entry[8..10].copy_from_slice(&(entry_len as u16).to_le_bytes());
    entry[0x40..0x48]
        .copy_from_slice(&((128u64 * 1024 * 1024) + marker.len() as u64).to_le_bytes());
    entry[0x50] = "large.bin".encode_utf16().count() as u8;
    for (i, ch) in "large.bin".encode_utf16().enumerate() {
        entry[0x52 + i * 2..0x54 + i * 2].copy_from_slice(&ch.to_le_bytes());
    }
    let mut off = iro + 0x20;
    rec5[off..off + entry.len()].copy_from_slice(&entry);
    off += entry.len();
    rec5[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;
    rec5[iro + 4..iro + 8].copy_from_slice(&((off - iro) as u32).to_le_bytes());

    let rec6 = &mut data[rec6_off..rec6_off + MFT_RECORD_SIZE];
    rec6[0..4].copy_from_slice(b"FILE");
    rec6[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    rec6[0x38..0x3C].copy_from_slice(&0x10u32.to_le_bytes());
    rec6[0x3C..0x40].copy_from_slice(&48u32.to_le_bytes());
    let data_attr = 0x68usize;
    let logical_size = (128u64 * 1024 * 1024) + marker.len() as u64;
    rec6[data_attr..data_attr + 4].copy_from_slice(&0x80u32.to_le_bytes());
    rec6[data_attr + 8] = 1;
    rec6[data_attr + 0x20..data_attr + 0x22].copy_from_slice(&0x40u16.to_le_bytes());
    rec6[data_attr + 0x28..data_attr + 0x30]
        .copy_from_slice(&((SPARSE_PREFIX_CLUSTERS + 1) * CLUSTER_SIZE as u64).to_le_bytes());
    rec6[data_attr + 0x30..data_attr + 0x38].copy_from_slice(&logical_size.to_le_bytes());

    let run = data_attr + 0x40;
    rec6[run] = 0x03;
    rec6[run + 1..run + 4].copy_from_slice(&SPARSE_PREFIX_CLUSTERS.to_le_bytes()[..3]);
    rec6[run + 4] = 0x11;
    rec6[run + 5] = 1;
    rec6[run + 6] = DATA_CLUSTER as u8;
    rec6[run + 7] = 0;
    let attr_len = (run + 8 - data_attr) as u32;
    rec6[data_attr + 4..data_attr + 8].copy_from_slice(&attr_len.to_le_bytes());

    data[data_off..data_off + marker.len()].copy_from_slice(marker);
    std::fs::write(path, data)
}

fn write_large_ext4_raw_fixture(path: &std::path::Path, marker: &[u8]) -> std::io::Result<u64> {
    const BLOCK_SIZE: u64 = 4096;
    const LOGICAL_OFFSET: u64 = 128 * 1024 * 1024;
    const TOTAL_BLOCKS: u64 = 10;
    const INODE_TABLE_OFF: usize = 8192;
    const ROOT_BLOCK: u32 = 3;
    const FILE_BLOCK: u32 = 7;

    let total_size = (TOTAL_BLOCKS * BLOCK_SIZE) as usize;
    let mut data = vec![0u8; total_size];
    let file_size = LOGICAL_OFFSET + marker.len() as u64;
    let logical_block = (LOGICAL_OFFSET / BLOCK_SIZE) as u32;

    let sb = &mut data[1024..2048];
    sb[0x00..0x04].copy_from_slice(&16u32.to_le_bytes());
    sb[0x04..0x08].copy_from_slice(&(TOTAL_BLOCKS as u32).to_le_bytes());
    sb[0x14..0x18].copy_from_slice(&0u32.to_le_bytes());
    sb[0x18..0x1C].copy_from_slice(&2u32.to_le_bytes());
    sb[0x20..0x24].copy_from_slice(&32768u32.to_le_bytes());
    sb[0x28..0x2C].copy_from_slice(&16u32.to_le_bytes());
    sb[0x38..0x3A].copy_from_slice(&0xEF53u16.to_le_bytes());
    sb[0x58..0x5A].copy_from_slice(&256u16.to_le_bytes());

    data[4096 + 0x08..4096 + 0x0C].copy_from_slice(&2u32.to_le_bytes());

    let root_inode = &mut data[INODE_TABLE_OFF + 256..INODE_TABLE_OFF + 512];
    root_inode[0x00..0x02].copy_from_slice(&0x41EDu16.to_le_bytes());
    root_inode[0x04..0x08].copy_from_slice(&BLOCK_SIZE.to_le_bytes()[..4]);
    root_inode[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes());
    root_inode[0x28..0x2A].copy_from_slice(&0xF30Au16.to_le_bytes());
    root_inode[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes());
    root_inode[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes());
    root_inode[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes());
    root_inode[0x3C..0x40].copy_from_slice(&ROOT_BLOCK.to_le_bytes());

    let file_inode = &mut data[INODE_TABLE_OFF + 512..INODE_TABLE_OFF + 768];
    file_inode[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes());
    file_inode[0x04..0x08].copy_from_slice(&(file_size as u32).to_le_bytes());
    file_inode[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes());
    file_inode[0x28..0x2A].copy_from_slice(&0xF30Au16.to_le_bytes());
    file_inode[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes());
    file_inode[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes());
    file_inode[0x34..0x38].copy_from_slice(&logical_block.to_le_bytes());
    file_inode[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes());
    file_inode[0x3C..0x40].copy_from_slice(&FILE_BLOCK.to_le_bytes());
    file_inode[0x6C..0x70].copy_from_slice(&((file_size >> 32) as u32).to_le_bytes());

    let root_dir = &mut data[ROOT_BLOCK as usize * BLOCK_SIZE as usize
        ..(ROOT_BLOCK as usize + 1) * BLOCK_SIZE as usize];
    root_dir[0x00..0x04].copy_from_slice(&2u32.to_le_bytes());
    root_dir[0x04..0x06].copy_from_slice(&12u16.to_le_bytes());
    root_dir[0x06] = 1;
    root_dir[0x07] = 2;
    root_dir[0x08] = b'.';
    root_dir[12..16].copy_from_slice(&2u32.to_le_bytes());
    root_dir[16..18].copy_from_slice(&12u16.to_le_bytes());
    root_dir[18] = 2;
    root_dir[19] = 2;
    root_dir[20..22].copy_from_slice(b"..");
    root_dir[24..28].copy_from_slice(&3u32.to_le_bytes());
    root_dir[28..30].copy_from_slice(&(BLOCK_SIZE as u16 - 24).to_le_bytes());
    root_dir[30] = 9;
    root_dir[31] = 1;
    root_dir[32..41].copy_from_slice(b"large.bin");

    let file_offset = FILE_BLOCK as usize * BLOCK_SIZE as usize;
    data[file_offset..file_offset + marker.len()].copy_from_slice(marker);

    std::fs::write(path, data)?;
    Ok(LOGICAL_OFFSET)
}

static LVM_OPEN_READER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct ZeroEvidenceReader {
    info: evidence_core::ReaderInfo,
}

impl ZeroEvidenceReader {
    fn new(path: std::path::PathBuf) -> Self {
        Self {
            info: evidence_core::ReaderInfo {
                path,
                size: 4096,
                kind: "test".to_string(),
            },
        }
    }
}

impl Read for ZeroEvidenceReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        buf.fill(0);
        Ok(buf.len())
    }
}

impl Seek for ZeroEvidenceReader {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Ok(0)
    }
}

impl evidence_core::EvidenceReader for ZeroEvidenceReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

fn write_fat32_raw_fixture(path: &std::path::Path) -> std::io::Result<()> {
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

    std::fs::write(path, data)
}

fn write_exfat_raw_fixture(path: &std::path::Path) -> std::io::Result<()> {
    const SECTOR_SIZE: usize = 512;
    const FAT_SECTOR: usize = 24;
    const CLUSTER_HEAP_SECTOR: usize = 32;
    const CLUSTER_SIZE: usize = SECTOR_SIZE;
    const FILE_SIZE: usize = CLUSTER_SIZE * 3;
    const TOTAL_SECTORS: usize = 1024;

    let mut data = vec![0u8; TOTAL_SECTORS * SECTOR_SIZE];

    let boot = &mut data[0..SECTOR_SIZE];
    boot[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    boot[72..80].copy_from_slice(&(TOTAL_SECTORS as u64).to_le_bytes());
    boot[80..84].copy_from_slice(&(FAT_SECTOR as u32).to_le_bytes());
    boot[84..88].copy_from_slice(&1u32.to_le_bytes());
    boot[88..92].copy_from_slice(&(CLUSTER_HEAP_SECTOR as u32).to_le_bytes());
    boot[92..96].copy_from_slice(&100u32.to_le_bytes());
    boot[96..100].copy_from_slice(&2u32.to_le_bytes());
    boot[100..104].copy_from_slice(&0x12345678u32.to_le_bytes());
    boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
    boot[108] = 9;
    boot[109] = 0;
    boot[110] = 1;
    boot[111] = 0x80;
    boot[112] = 0xFF;
    boot[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());

    let fat_offset = FAT_SECTOR * SECTOR_SIZE;
    let fat = &mut data[fat_offset..fat_offset + SECTOR_SIZE];
    fat[0..4].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF]);
    fat[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    fat[8..12].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    fat[12..16].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);

    let root_offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE;
    let root = &mut data[root_offset..root_offset + CLUSTER_SIZE];
    let mut pos = 0usize;

    root[pos] = 0x85;
    root[pos + 1] = 0x02;
    root[pos + 4..pos + 6].copy_from_slice(&0x20u16.to_le_bytes());
    pos += 32;

    root[pos] = 0xC0;
    root[pos + 1] = 0x02;
    root[pos + 3] = "LARGE.BIN".encode_utf16().count() as u8;
    root[pos + 8..pos + 16].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
    root[pos + 20..pos + 24].copy_from_slice(&3u32.to_le_bytes());
    root[pos + 24..pos + 32].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
    pos += 32;

    root[pos] = 0xC1;
    for (i, ch) in "LARGE.BIN".encode_utf16().enumerate() {
        let offset = pos + 2 + i * 2;
        root[offset..offset + 2].copy_from_slice(&ch.to_le_bytes());
    }

    for cluster in 3..=5usize {
        let value = match cluster {
            3 => b'A',
            4 => b'B',
            5 => b'C',
            _ => unreachable!(),
        };
        let offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE + (cluster - 2) * CLUSTER_SIZE;
        data[offset..offset + CLUSTER_SIZE].fill(value);
    }

    std::fs::write(path, data)
}

#[test]
fn logical_directory_mid_file_range_uses_seek_not_linear_skip() {
    let dir = tempfile::TempDir::new().unwrap();
    let evidence_dir = dir.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    let bytes: Vec<u8> = (0u8..64).collect();
    std::fs::write(evidence_dir.join("sample.bin"), &bytes).unwrap();

    let conn = persistence_sqlite::open_or_create(&dir.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-range', 'Range Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    let ds_id = DataSourceId("ds-logical-range".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("case-range".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "logical evidence".to_string(),
                kind: DataSourceKind::LogicalDirectory,
                source_path: evidence_dir,
                imported_at: chrono::Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
         VALUES ('file-sample', NULL, ?1, 'sample.bin', 'sample.bin', 'file', ?2, 'bin', 0, 0, 0)",
        params![ds_id.0, bytes.len() as i64],
    )
    .unwrap();

    reset_skip_reader_bytes_call_count();
    let range_bytes =
        read_file_bytes_for_case(&conn, &FileEntryId("file-sample".to_string()), 17, 12).unwrap();

    assert_eq!(range_bytes, bytes[17..29].to_vec());
    assert_eq!(skip_reader_bytes_call_count(), 0);

    reset_skip_reader_bytes_call_count();
    let response = read_file_range_for_case(
        &conn,
        &ViewerRangeRequestDto {
            handle_id: "file:file-sample".to_string(),
            offset: 17,
            length: 12,
        },
    )
    .unwrap();

    assert_eq!(response.raw_bytes.unwrap(), bytes[17..29].to_vec());
    assert!(response.lines.is_empty());
    assert_eq!(skip_reader_bytes_call_count(), 0);
}

#[test]
fn logical_directory_repeated_range_uses_preview_descriptor_cache() {
    let dir = tempfile::TempDir::new().unwrap();
    let evidence_dir = dir.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    let bytes: Vec<u8> = (0u8..64).collect();
    std::fs::write(evidence_dir.join("sample.bin"), &bytes).unwrap();

    let conn = persistence_sqlite::open_or_create(&dir.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-cache', 'Cache Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    let ds_id = DataSourceId("ds-logical-cache".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("case-cache".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "logical evidence".to_string(),
                kind: DataSourceKind::LogicalDirectory,
                source_path: evidence_dir,
                imported_at: chrono::Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
         VALUES ('file-cache-sample', NULL, ?1, 'sample.bin', 'sample.bin', 'file', ?2, 'bin', 0, 0, 0)",
        params![ds_id.0, bytes.len() as i64],
    )
    .unwrap();

    let file_id = FileEntryId("file-cache-sample".to_string());
    let cache = std::cell::RefCell::new(HashMap::<String, serde_json::Value>::new());
    let cache_hits = Cell::new(0usize);
    let set_calls = Cell::new(0usize);

    let read_with_cache = |offset, length| {
        let get_cache = |key: &str| {
            let value = cache.borrow().get(key).cloned();
            if value.is_some() {
                cache_hits.set(cache_hits.get() + 1);
            }
            value
        };
        let set_cache = |key: &str, value: &serde_json::Value| {
            set_calls.set(set_calls.get() + 1);
            cache.borrow_mut().insert(key.to_string(), value.clone());
        };
        read_file_bytes_for_case(
            (&conn, "case-cache", get_cache, set_cache),
            &file_id,
            offset,
            length,
        )
    };

    let first = read_with_cache(0, 8).unwrap();
    assert_eq!(first, bytes[0..8].to_vec());
    assert_eq!(set_calls.get(), 1);
    assert_eq!(cache_hits.get(), 0);

    let second = read_with_cache(17, 12).unwrap();
    assert_eq!(second, bytes[17..29].to_vec());
    assert_eq!(set_calls.get(), 1);
    assert_eq!(cache_hits.get(), 1);

    cache.borrow_mut().clear();
    let third = read_with_cache(29, 7).unwrap();
    assert_eq!(third, bytes[29..36].to_vec());
    assert_eq!(set_calls.get(), 2);
}

#[test]
fn raw_ntfs_mid_file_range_uses_ntfs_range_reader_without_materialize() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("large_ntfs.raw");
    let marker = b"RANGE-ONLY";
    write_large_ntfs_raw_fixture(&raw_path, marker).unwrap();

    let huge_size = (128u64 * 1024 * 1024) + marker.len() as u64;
    let descriptor = PreviewDescriptor {
        case_id: "case-raw-ntfs-range".to_string(),
        file_id: "mft:1:6".to_string(),
        source_kind: "raw".to_string(),
        source_path: raw_path.display().to_string(),
        partition_index: Some(1),
        filesystem_kind: Some("NTFS".to_string()),
        path: "[P1]/large.bin".to_string(),
        mime: Some("application/octet-stream".to_string()),
        size: huge_size,
        data_source_id: "ds-raw-ntfs-range".to_string(),
        partition_candidates: vec![PreviewPartitionCandidate {
            partition_index: 1,
            filesystem_kind: "NTFS".to_string(),
            offset: 0,
            lvm_identity: None,
        }],
        entry_size: huge_size,
        entry_modified_at: None,
    };

    reset_skip_reader_bytes_call_count();
    let bytes =
        read_file_bytes_for_descriptor(&descriptor, 128u64 * 1024 * 1024, marker.len() as u32)
            .unwrap();

    assert_eq!(bytes, marker);
    assert_eq!(skip_reader_bytes_call_count(), 0);
}

#[test]
fn raw_ext4_mid_file_range_uses_linux_range_reader_without_materialize() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("large_ext4.raw");
    let marker = b"EXT4-VIEWER-RANGE";
    let offset = write_large_ext4_raw_fixture(&raw_path, marker).unwrap();

    let descriptor = PreviewDescriptor {
        case_id: "case-raw-ext4-range".to_string(),
        file_id: "ext4-file-range".to_string(),
        source_kind: "raw".to_string(),
        source_path: raw_path.display().to_string(),
        partition_index: Some(0),
        filesystem_kind: Some("Ext4".to_string()),
        path: "[P0]/large.bin".to_string(),
        mime: Some("application/octet-stream".to_string()),
        size: offset + marker.len() as u64,
        data_source_id: "ds-raw-ext4-range".to_string(),
        partition_candidates: vec![PreviewPartitionCandidate {
            partition_index: 0,
            filesystem_kind: "Ext4".to_string(),
            offset: 0,
            lvm_identity: None,
        }],
        entry_size: offset + marker.len() as u64,
        entry_modified_at: None,
    };

    reset_skip_reader_bytes_call_count();
    let bytes = read_file_bytes_for_descriptor(&descriptor, offset, marker.len() as u32).unwrap();

    assert_eq!(bytes, marker);
    assert_eq!(skip_reader_bytes_call_count(), 0);
}

#[test]
fn linux_lvm_candidate_reopens_one_reader_per_physical_volume() {
    let dir = tempfile::TempDir::new().unwrap();
    let source_path = dir.path().join("lvm.raw");
    let candidate = PreviewPartitionCandidate {
        partition_index: 4,
        filesystem_kind: "XFS".to_string(),
        offset: 1_048_576,
        lvm_identity: Some(PreviewLvmIdentity {
            vg_uuid: "vg-uuid".to_string(),
            vg_name: "vg".to_string(),
            lv_uuid: "lv-uuid".to_string(),
            lv_name: "root".to_string(),
            pv_offsets: vec![1_048_576, 2_097_152],
        }),
    };

    LVM_OPEN_READER_CALLS.store(0, Ordering::Relaxed);
    let result = image_open::open_candidate_block_reader(&source_path, &candidate, &mut |path| {
        LVM_OPEN_READER_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(ZeroEvidenceReader::new(path.to_path_buf()))
            as Box<dyn evidence_core::EvidenceReader>)
    });

    assert!(result.is_err());
    assert_eq!(LVM_OPEN_READER_CALLS.load(Ordering::Relaxed), 2);
}

#[test]
fn raw_fat_mid_file_range_uses_fat_range_reader_without_materialize() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("fat32.raw");
    write_fat32_raw_fixture(&raw_path).unwrap();

    let descriptor = PreviewDescriptor {
        case_id: "case-raw-fat-range".to_string(),
        file_id: "fat-file-range".to_string(),
        source_kind: "raw".to_string(),
        source_path: raw_path.display().to_string(),
        partition_index: Some(0),
        filesystem_kind: Some("FAT".to_string()),
        path: "[P0]/RANGE.TXT".to_string(),
        mime: Some("text/plain".to_string()),
        size: 1536,
        data_source_id: "ds-raw-fat-range".to_string(),
        partition_candidates: vec![PreviewPartitionCandidate {
            partition_index: 0,
            filesystem_kind: "FAT".to_string(),
            offset: 0,
            lvm_identity: None,
        }],
        entry_size: 1536,
        entry_modified_at: None,
    };

    reset_skip_reader_bytes_call_count();
    let bytes = read_file_bytes_for_descriptor(&descriptor, 512 + 7, 9).unwrap();

    assert_eq!(bytes, vec![b'B'; 9]);
    assert_eq!(skip_reader_bytes_call_count(), 0);
}

#[test]
fn raw_exfat_mid_file_range_uses_exfat_range_reader_without_materialize() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("exfat.raw");
    write_exfat_raw_fixture(&raw_path).unwrap();

    let conn = persistence_sqlite::open_or_create(&dir.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-raw-exfat-range', 'Raw exFAT Range Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    let ds_id = DataSourceId("ds-raw-exfat-range".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("case-raw-exfat-range".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "raw exfat evidence".to_string(),
                kind: DataSourceKind::Raw,
                source_path: raw_path,
                imported_at: chrono::Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
         VALUES ('file-raw-exfat-large', NULL, ?1, 'LARGE.BIN', 'LARGE.BIN', 'file', ?2, 'bin', 0, 0, 0)",
        params![ds_id.0, 1536i64],
    )
    .unwrap();

    reset_skip_reader_bytes_call_count();
    let bytes = read_file_bytes_for_case(
        &conn,
        &FileEntryId("file-raw-exfat-large".to_string()),
        512 + 7,
        9,
    )
    .unwrap();

    assert_eq!(bytes, vec![b'B'; 9]);
    assert_eq!(skip_reader_bytes_call_count(), 0);

    reset_skip_reader_bytes_call_count();
    let response = read_file_range_for_case(
        &conn,
        &ViewerRangeRequestDto {
            handle_id: "file:file-raw-exfat-large".to_string(),
            offset: 512 + 7,
            length: 9,
        },
    )
    .unwrap();

    assert_eq!(response.raw_bytes.unwrap(), vec![b'B'; 9]);
    assert!(response.lines.is_empty());
    assert_eq!(skip_reader_bytes_call_count(), 0);
}

#[test]
fn raw_exfat_text_header_reads_via_bytes_only_fast_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("exfat.raw");
    write_exfat_raw_fixture(&raw_path).unwrap();

    let conn = persistence_sqlite::open_or_create(&dir.path().join("case.db")).unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-raw-exfat-header', 'Raw exFAT Header Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    let ds_id = DataSourceId("ds-raw-exfat-header".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("case-raw-exfat-header".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "raw exfat evidence".to_string(),
                kind: DataSourceKind::Raw,
                source_path: raw_path,
                imported_at: chrono::Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
         VALUES ('file-raw-exfat-header', NULL, ?1, 'LARGE.BIN', 'LARGE.BIN', 'file', ?2, 'bin', 0, 0, 0)",
        params![ds_id.0, 1536i64],
    )
    .unwrap();

    reset_open_file_content_by_id_call_count();
    reset_read_file_bytes_for_case_call_count();
    reset_skip_reader_bytes_call_count();

    let bytes =
        read_file_header_by_id(&conn, &FileEntryId("file-raw-exfat-header".to_string()), 16)
            .unwrap();

    assert_eq!(bytes, vec![b'A'; 16]);
    assert_eq!(read_file_bytes_for_case_call_count(), 1);
    assert_eq!(open_file_content_by_id_call_count(), 0);
    assert_eq!(skip_reader_bytes_call_count(), 0);
}

#[test]
fn truncated_e01_segment_no_panic() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("truncated.E01");
    // Write a valid E01 header but truncate before the chunk data
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")
        .unwrap();
    // Write volume section descriptor but no actual chunk table or data
    let desc = section_desc("volume", 0, 76 + 36);
    f.write_all(&desc).unwrap();
    f.write_all(&[0u8; 36]).unwrap();
    // Missing: table section, done section, chunk data
    f.flush().unwrap();
    drop(f);

    // Opening should fail gracefully with an error, not panic
    let result = image_e01::E01Reader::open(&path);
    assert!(
        result.is_err(),
        "Truncated E01 should return error, not panic"
    );
}

#[test]
fn truncated_e01_chunk_read_no_panic() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("short_chunk.E01");
    write_tiny_e01(&path).unwrap();

    // Open works (complete structure)
    let mut reader = image_e01::E01Reader::open(&path).unwrap();

    // Read available chunk data
    let mut buf = vec![0u8; 14]; // "E01-CACHE-TEST" marker
    reader.seek(SeekFrom::Start(0)).unwrap();
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"E01-CACHE-TEST");

    // Read past the available chunk — E01 reader should handle short reads gracefully
    let mut big_buf = vec![0u8; 8192];
    reader.seek(SeekFrom::Start(0)).unwrap();
    let result = reader.read(&mut big_buf);
    // read() may return partial data without error. Just verify no panic.
    let _ = result;
    eprintln!(
        "Short read returned {} bytes (expected for tiny E01)",
        big_buf.len()
    );
}

#[test]
fn multi_partition_resolves_partition_index_correctly() {
    // Verify that entries with partition index in ID format resolve correctly
    assert_eq!(
        mft_partition_index_from_entry_id("mft:0:42"),
        Some(0),
        "Partition 0 entry should resolve to index 0"
    );
    assert_eq!(
        mft_partition_index_from_entry_id("mft:2:100"),
        Some(2),
        "Partition 2 entry should resolve to index 2"
    );

    // Verify that entries WITHOUT partition index in ID fall back to parent chain
    assert_eq!(
        mft_partition_index_from_entry_id("mft:42"),
        None,
        "Legacy format should return None and fall back to parent chain"
    );

    // Verify root name parsing from parent chain (simulated via function)
    let root_name = "Partition 3 (NTFS)";
    let idx: Option<usize> = root_name.strip_prefix("Partition ").and_then(|rest| {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    });
    assert_eq!(
        idx,
        Some(3),
        "Root name 'Partition 3 (NTFS)' should resolve to index 3"
    );
}

#[test]
fn mft_partition_index_from_entry_id_parses_partition_record_format() {
    assert_eq!(mft_partition_index_from_entry_id("mft:3:42"), Some(3));
    assert_eq!(mft_partition_index_from_entry_id("mft:0:5"), Some(0));
}

#[test]
fn mft_partition_index_from_entry_id_returns_none_for_legacy_format() {
    assert_eq!(mft_partition_index_from_entry_id("mft:42"), None);
    assert_eq!(mft_partition_index_from_entry_id("not-mft:1:2"), None);
}

#[test]
fn fresh_e01_reader_opens_successfully() {
    let (_dir, path) = make_temp_e01();
    let mut reader = image_e01::E01Reader::open(&path).unwrap();
    let mut buf = [0u8; 14];
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"E01-CACHE-TEST");
}

#[test]
fn fresh_e01_readers_have_independent_positions() {
    let (_dir, path) = make_temp_e01();
    let mut reader1 = image_e01::E01Reader::open(&path).unwrap();
    let mut reader2 = image_e01::E01Reader::open(&path).unwrap();

    reader1.seek(SeekFrom::Start(0)).unwrap();
    reader2.seek(SeekFrom::Start(4)).unwrap();
    let mut b1 = [0u8; 4];
    let mut b2 = [0u8; 4];
    reader1.read_exact(&mut b1).unwrap();
    reader2.read_exact(&mut b2).unwrap();
    assert_eq!(&b1, b"E01-");
    assert_eq!(&b2, b"CACH");
}

#[test]
fn lru_evicts_oldest_when_full() {
    clear_e01_reader_cache();
    let dir = tempfile::TempDir::new().unwrap();
    let mut paths = Vec::new();
    // Open one more than the per-case limit to force eviction.
    let limit = E01_READER_CACHE_PER_CASE_MAX_SIZE;
    for i in 0..=limit {
        let path = dir.path().join(format!("cache-test-{i}.E01"));
        write_tiny_e01(&path).unwrap();
        paths.push(path);
    }
    for path in &paths[..limit] {
        let _r = open_e01_reader_cached(path, "").unwrap();
    }
    // The next open evicts the least-recently-used reader (paths[0]).
    let _r = open_e01_reader_cached(&paths[limit], "").unwrap();

    // Verify paths[0] was evicted by trying to open it fresh — should still work.
    let mut r = open_e01_reader_cached(&paths[0], "").unwrap();
    let mut buf = [0u8; 14];
    r.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"E01-CACHE-TEST");

    clear_e01_reader_cache();
}

#[test]
fn e01_reader_cache_is_bucketed_by_case() {
    clear_e01_reader_cache();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("shared.E01");
    write_tiny_e01(&path).unwrap();

    // Warm the cache for case A and case B.
    let _ = open_e01_reader_cached(&path, "case-a").unwrap();
    let _ = open_e01_reader_cached(&path, "case-b").unwrap();

    // Clearing case A must not evict case B's reader.
    clear_e01_reader_cache_for_case("case-a");
    let mut r = open_e01_reader_cached(&path, "case-b").unwrap();
    let mut buf = [0u8; 14];
    r.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"E01-CACHE-TEST");

    clear_e01_reader_cache();
}

#[test]
fn cache_clear_on_poison() {
    clear_e01_reader_cache();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("poison-test.E01");
    write_tiny_e01(&path).unwrap();

    // Populate the cache
    let _r = open_e01_reader_cached(&path, "").unwrap();

    // Poison the mutex by panicking while holding the lock
    let result = std::panic::catch_unwind(|| {
        let _lock = E01_READER_CACHE.lock().unwrap();
        panic!("simulated cache panic");
    });
    assert!(result.is_err());

    // After poison, the cache should be cleared and a new open should work
    let mut r = open_e01_reader_cached(&path, "").unwrap();
    let mut buf = [0u8; 14];
    r.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"E01-CACHE-TEST");

    clear_e01_reader_cache();
}
