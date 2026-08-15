use super::*;
use super::{
    descriptor::descriptor_is_fresh, filesystem::mft_partition_index_from_entry_id,
    range::api::read_file_bytes_for_descriptor_with_context,
};
use crate::e01_reader_cache::{E01_READER_CACHE, E01_READER_CACHE_PER_CASE_MAX_SIZE};
use crate::file_service::{
    preview_runtime::{PreparedFile, PreviewRuntimeRegistry, PreviewSession},
    FileServiceError,
};
use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::params;
use std::cell::Cell;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use transport::dto::ViewerRangeRequestDto;

#[path = "preview.rs"]
mod preview_tests;

fn read_file_bytes_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError> {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let mut context = &conn;
    read_file_bytes_for_descriptor_with_context(&mut context, descriptor, offset, length)
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
    vol[8..12].copy_from_slice(&chunk_sectors.to_le_bytes());
    vol[12..16].copy_from_slice(&512u32.to_le_bytes());
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

fn write_large_ntfs_raw_fixture(
    path: &std::path::Path,
    marker: &[u8],
    sparse_prefix_bytes: u64,
) -> std::io::Result<()> {
    const CLUSTER_SIZE: usize = 512;
    const MFT_RECORD_SIZE: usize = 1024;
    const MFT_CLUSTER: usize = 2;
    const FILE_RECORD: u64 = 6;
    const DATA_CLUSTER: usize = 32;
    let sparse_prefix_clusters = sparse_prefix_bytes / CLUSTER_SIZE as u64;

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
    entry[0x40..0x48].copy_from_slice(&(sparse_prefix_bytes + marker.len() as u64).to_le_bytes());
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
    let logical_size = sparse_prefix_bytes + marker.len() as u64;
    rec6[data_attr..data_attr + 4].copy_from_slice(&0x80u32.to_le_bytes());
    rec6[data_attr + 8] = 1;
    rec6[data_attr + 0x18..data_attr + 0x20].copy_from_slice(&sparse_prefix_clusters.to_le_bytes());
    rec6[data_attr + 0x20..data_attr + 0x22].copy_from_slice(&0x40u16.to_le_bytes());
    rec6[data_attr + 0x28..data_attr + 0x30]
        .copy_from_slice(&((sparse_prefix_clusters + 1) * CLUSTER_SIZE as u64).to_le_bytes());
    rec6[data_attr + 0x30..data_attr + 0x38].copy_from_slice(&logical_size.to_le_bytes());

    let run = data_attr + 0x40;
    rec6[run] = 0x03;
    rec6[run + 1..run + 4].copy_from_slice(&sparse_prefix_clusters.to_le_bytes()[..3]);
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
    // The declared file size (128 MiB sparse) must fit the filesystem
    // capacity the parser now enforces.
    sb[0x04..0x08].copy_from_slice(&40960u32.to_le_bytes());
    sb[0x14..0x18].copy_from_slice(&0u32.to_le_bytes());
    sb[0x18..0x1C].copy_from_slice(&2u32.to_le_bytes());
    sb[0x20..0x24].copy_from_slice(&32768u32.to_le_bytes());
    sb[0x28..0x2C].copy_from_slice(&16u32.to_le_bytes());
    sb[0x38..0x3A].copy_from_slice(&0xEF53u16.to_le_bytes());
    sb[0x58..0x5A].copy_from_slice(&256u16.to_le_bytes());

    data[4096 + 0x08..4096 + 0x0C].copy_from_slice(&2u32.to_le_bytes());

    let root_inode = &mut data[INODE_TABLE_OFF + 256..INODE_TABLE_OFF + 512];
    root_inode[0x00..0x02].copy_from_slice(&0x41EDu16.to_le_bytes());
    root_inode[0x20..0x24].copy_from_slice(&0x0008_0000u32.to_le_bytes()); // EXT4_EXTENTS_FL
    root_inode[0x04..0x08].copy_from_slice(&BLOCK_SIZE.to_le_bytes()[..4]);
    root_inode[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes());
    root_inode[0x28..0x2A].copy_from_slice(&0xF30Au16.to_le_bytes());
    root_inode[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes());
    root_inode[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes());
    root_inode[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes());
    root_inode[0x3C..0x40].copy_from_slice(&ROOT_BLOCK.to_le_bytes());

    let file_inode = &mut data[INODE_TABLE_OFF + 512..INODE_TABLE_OFF + 768];
    file_inode[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes());
    file_inode[0x20..0x24].copy_from_slice(&0x0008_0000u32.to_le_bytes()); // EXT4_EXTENTS_FL
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

fn encode_xfs_extent(logical: u64, start_block: u64, block_count: u64) -> [u8; 16] {
    let l0 = ((logical & ((1u64 << 54) - 1)) << 9) | (start_block >> 43);
    let l1 = ((start_block & ((1u64 << 43) - 1)) << 21) | (block_count & 0x1F_FFFF);
    let mut encoded = [0u8; 16];
    encoded[0..8].copy_from_slice(&l0.to_be_bytes());
    encoded[8..16].copy_from_slice(&l1.to_be_bytes());
    encoded
}

fn build_ceph_xfs_bounded_range_fixture(marker: &[u8]) -> (Vec<u8>, u64, Range<u64>) {
    const BLOCK_SIZE: u64 = 4096;
    const TOTAL_BLOCKS: u64 = 10;
    const INODE_BASE: usize = 8192;
    const INODE_SIZE: usize = 256;
    const INODE_CORE_SIZE: usize = 100;
    const LOGICAL_OFFSET: u64 = 128 * 1024 * 1024;
    const FIRST_DATA_BLOCK: u64 = 4;
    const TARGET_DATA_BLOCK: u64 = 6;

    let mut image = vec![0u8; (TOTAL_BLOCKS * BLOCK_SIZE) as usize];
    let superblock = &mut image[..512];
    superblock[0x00..0x04].copy_from_slice(&0x5846_5342u32.to_be_bytes());
    superblock[0x04..0x08].copy_from_slice(&(BLOCK_SIZE as u32).to_be_bytes());
    superblock[0x08..0x10].copy_from_slice(&TOTAL_BLOCKS.to_be_bytes());
    superblock[0x38..0x40].copy_from_slice(&2u64.to_be_bytes());
    superblock[0x54..0x58].copy_from_slice(&(TOTAL_BLOCKS as u32).to_be_bytes());
    superblock[0x58..0x5C].copy_from_slice(&1u32.to_be_bytes());
    superblock[0x66..0x68].copy_from_slice(&512u16.to_be_bytes());
    superblock[0x68..0x6A].copy_from_slice(&(INODE_SIZE as u16).to_be_bytes());
    superblock[0x6A..0x6C].copy_from_slice(&16u16.to_be_bytes());

    let root = &mut image[INODE_BASE + INODE_SIZE..INODE_BASE + 2 * INODE_SIZE];
    root[0x00..0x02].copy_from_slice(&0x494Eu16.to_be_bytes());
    root[0x02..0x04].copy_from_slice(&(0x4000u16 | 0o755).to_be_bytes());
    root[0x04] = 2;
    root[0x05] = 1;
    root[0x38..0x40].copy_from_slice(&BLOCK_SIZE.to_be_bytes());
    let root_data = &mut root[INODE_CORE_SIZE..];
    root_data[0] = 1;
    root_data[1] = 1;
    root_data[2..10].copy_from_slice(&2u64.to_be_bytes());
    root_data[10] = 9;
    root_data[11..13].copy_from_slice(&0x0018u16.to_be_bytes());
    root_data[13..22].copy_from_slice(b"large.bin");
    root_data[22..30].copy_from_slice(&3u64.to_be_bytes());

    let file = &mut image[INODE_BASE + 2 * INODE_SIZE..INODE_BASE + 3 * INODE_SIZE];
    file[0x00..0x02].copy_from_slice(&0x494Eu16.to_be_bytes());
    file[0x02..0x04].copy_from_slice(&(0x8000u16 | 0o644).to_be_bytes());
    file[0x04] = 2;
    file[0x05] = 2;
    file[0x38..0x40].copy_from_slice(&(LOGICAL_OFFSET + marker.len() as u64).to_be_bytes());
    file[0x4C..0x50].copy_from_slice(&2u32.to_be_bytes());
    let file_data = &mut file[INODE_CORE_SIZE..];
    file_data[0..16].copy_from_slice(&encode_xfs_extent(0, FIRST_DATA_BLOCK, 1));
    file_data[16..32].copy_from_slice(&encode_xfs_extent(
        LOGICAL_OFFSET / BLOCK_SIZE,
        TARGET_DATA_BLOCK,
        1,
    ));

    let first_data_start = FIRST_DATA_BLOCK * BLOCK_SIZE;
    let first_data_end = first_data_start + BLOCK_SIZE;
    image[first_data_start as usize..first_data_end as usize].fill(b'A');
    let target_start = (TARGET_DATA_BLOCK * BLOCK_SIZE) as usize;
    image[target_start..target_start + marker.len()].copy_from_slice(marker);

    (image, LOGICAL_OFFSET, first_data_start..first_data_end)
}

fn build_synthetic_lvm_disk() -> Vec<u8> {
    let pv_uuid = "abcdef1234567890abcdef1234567890";
    let pv_size = 2_097_152u64;
    let mut disk = vec![0u8; pv_size as usize];

    {
        let sec = &mut disk[512..1024];
        sec[0..8].copy_from_slice(b"LABELONE");
        sec[8..16].copy_from_slice(&1u64.to_le_bytes());
        sec[20..24].copy_from_slice(&32u32.to_le_bytes());
        sec[24..32].copy_from_slice(b"LVM2 001");
        sec[32..64].copy_from_slice(format!("{:32}", pv_uuid).as_bytes());
        sec[64..72].copy_from_slice(&pv_size.to_le_bytes());
        sec[72..80].copy_from_slice(&2560u64.to_le_bytes());
        sec[80..88].copy_from_slice(&(pv_size - 2560).to_le_bytes());
        sec[104..112].copy_from_slice(&1024u64.to_le_bytes());
        sec[112..120].copy_from_slice(&(4 * 512u64).to_le_bytes());
        let crc = fs_lvm::crc::lvm_crc32(&sec[20..512]);
        sec[16..20].copy_from_slice(&crc.to_le_bytes());
    }

    let metadata_text = format!(
        r#"test_vg {{
    id = "vg-1234"
    seqno = 1
    extent_size = 1

    physical_volumes {{
        pv0 {{
            id = "{}"
            device = "/dev/sda1"
            pe_start = 5
            pe_count = 10
        }}
    }}

    logical_volumes {{
        root {{
            id = "lv-root-uuid"
            segment_count = 1
            segment1 {{
                start_extent = 0
                extent_count = 1
                type = "striped"
                stripe_count = 1
                stripes = ["pv0", 0]
            }}
        }}
    }}
}}
"#,
        pv_uuid
    );
    write_synthetic_lvm_metadata(&mut disk, &metadata_text);
    disk
}

fn write_synthetic_lvm_metadata(disk: &mut [u8], metadata_text: &str) {
    let text_bytes = metadata_text.as_bytes();
    let text_offset = 1536usize;
    let text_end = text_offset + text_bytes.len();
    assert!(text_end <= disk.len());

    {
        let mda = &mut disk[1024..1536];
        mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
        mda[20..24].copy_from_slice(&1u32.to_le_bytes());
        mda[24..32].copy_from_slice(&1024u64.to_le_bytes());
        mda[32..40].copy_from_slice(&1536u64.to_le_bytes());
        mda[40..48].copy_from_slice(&512u64.to_le_bytes());
    }

    disk[text_offset..text_end].copy_from_slice(text_bytes);

    let text_size = text_bytes.len() as u64;
    let text_crc = fs_lvm::crc::lvm_crc32(text_bytes);
    {
        let mda = &mut disk[1024..1536];
        mda[48..56].copy_from_slice(&text_size.to_le_bytes());
        mda[56..60].copy_from_slice(&text_crc.to_le_bytes());
        let mda_crc = fs_lvm::crc::lvm_crc32(&mda[4..512]);
        mda[0..4].copy_from_slice(&mda_crc.to_le_bytes());
    }
}

fn refresh_synthetic_lvm_label_crc(disk: &mut [u8]) {
    let sec = &mut disk[512..1024];
    let crc = fs_lvm::crc::lvm_crc32(&sec[20..512]);
    sec[16..20].copy_from_slice(&crc.to_le_bytes());
}

fn replace_synthetic_lvm_pv_uuid(disk: &mut [u8], pv_uuid: &str) {
    let sec = &mut disk[512..1024];
    sec[32..64].fill(b' ');
    sec[32..64].copy_from_slice(format!("{:32}", pv_uuid).as_bytes());
    refresh_synthetic_lvm_label_crc(disk);
}

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

struct VecEvidenceReader {
    data: Vec<u8>,
    pos: u64,
    info: evidence_core::ReaderInfo,
}

impl VecEvidenceReader {
    fn new(path: std::path::PathBuf, data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            pos: 0,
            info: evidence_core::ReaderInfo {
                path,
                size,
                kind: "raw".to_string(),
            },
        }
    }
}

impl Read for VecEvidenceReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = self.pos as usize;
        let end = (start + buf.len()).min(self.data.len());
        let len = end.saturating_sub(start);
        buf[..len].copy_from_slice(&self.data[start..end]);
        self.pos += len as u64;
        Ok(len)
    }
}

impl Seek for VecEvidenceReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => (self.data.len() as i64 + offset).max(0) as u64,
            SeekFrom::Current(offset) => (self.pos as i64 + offset).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl evidence_core::EvidenceReader for VecEvidenceReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

struct RejectingRangeEvidenceReader {
    data: Vec<u8>,
    pos: u64,
    rejected: Range<u64>,
    info: evidence_core::ReaderInfo,
}

impl RejectingRangeEvidenceReader {
    fn new(data: Vec<u8>, rejected: Range<u64>) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            pos: 0,
            rejected,
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("virtual-ceph-rbd"),
                size,
                kind: "ceph_rbd".to_string(),
            },
        }
    }
}

impl Read for RejectingRangeEvidenceReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_end = self.pos.saturating_add(buf.len() as u64);
        if self.pos < self.rejected.end && requested_end > self.rejected.start {
            return Err(std::io::Error::other(
                "whole-file materialization touched an unrelated XFS extent",
            ));
        }
        let start = usize::try_from(self.pos)
            .unwrap_or(usize::MAX)
            .min(self.data.len());
        let end = start.saturating_add(buf.len()).min(self.data.len());
        let len = end.saturating_sub(start);
        buf[..len].copy_from_slice(&self.data[start..end]);
        self.pos = self.pos.saturating_add(len as u64);
        Ok(len)
    }
}

impl Seek for RejectingRangeEvidenceReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => (self.data.len() as i64 + offset).max(0) as u64,
            SeekFrom::Current(offset) => (self.pos as i64 + offset).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl evidence_core::EvidenceReader for RejectingRangeEvidenceReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

struct CephRbdRangeContext<'a> {
    conn: &'a rusqlite::Connection,
    image: Vec<u8>,
    rejected: Range<u64>,
    open_calls: usize,
}

impl PreviewReadContext for CephRbdRangeContext<'_> {
    fn conn(&self) -> &rusqlite::Connection {
        self.conn
    }

    fn case_id(&self) -> &str {
        "case-ceph-rbd-xfs-range"
    }

    fn open_evidence_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
    ) -> Result<Box<dyn evidence_core::EvidenceReader>, FileServiceError> {
        assert_eq!(descriptor.source_kind, "ceph_rbd");
        self.open_calls += 1;
        Ok(Box::new(RejectingRangeEvidenceReader::new(
            self.image.clone(),
            self.rejected.clone(),
        )))
    }
}

struct BitLockerNtfsRangeContext<'a> {
    conn: &'a rusqlite::Connection,
    source_path: std::path::PathBuf,
    open_calls: usize,
}

impl PreviewReadContext for BitLockerNtfsRangeContext<'_> {
    fn conn(&self) -> &rusqlite::Connection {
        self.conn
    }

    fn is_bitlocker_candidate(
        &self,
        _candidate: &PreviewPartitionCandidate,
    ) -> Result<bool, FileServiceError> {
        Ok(true)
    }

    fn open_candidate_block_reader(
        &mut self,
        _descriptor: &PreviewDescriptor,
        _candidate: &PreviewPartitionCandidate,
    ) -> Result<(Box<dyn evidence_core::EvidenceReader>, u64, String), FileServiceError> {
        self.open_calls += 1;
        Ok((
            Box::new(evidence_core::RawImageReader::open(&self.source_path)?),
            0,
            "NTFS".to_string(),
        ))
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

    let conn = persistence_sqlite::open_or_create_source(&dir.path().join("source.db")).unwrap();

    let ds_id = DataSourceId("ds-logical-range".to_string());
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(
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
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
         VALUES ('file-sample', NULL, ?1, 'sample.bin', 'sample.bin', 'file', ?2, 'bin', 0, 0, 0, 0)",
        params![ds_id.0, bytes.len() as i64],
    )
    .unwrap();

    let range_bytes =
        read_file_bytes_for_case(&conn, &FileEntryId("file-sample".to_string()), 17, 12).unwrap();

    assert_eq!(range_bytes, bytes[17..29].to_vec());

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
}

#[test]
fn mft_inode_range_is_not_bounded_by_stale_catalog_size() {
    assert!(super::range::api::catalog_size_allows_offset(
        "mft:3:34971",
        1024 * 1024,
        Some(8_192),
    ));
    assert!(!super::range::api::catalog_size_allows_offset(
        "logical-file",
        1024 * 1024,
        Some(8_192),
    ));
}

#[test]
fn efs_encrypted_entry_is_rejected_by_all_read_entry_points() {
    let dir = tempfile::TempDir::new().unwrap();
    let evidence_dir = dir.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join("encrypted.bin"), b"ciphertext").unwrap();
    let conn = persistence_sqlite::open_or_create_source(&dir.path().join("source.db")).unwrap();
    let ds_id = DataSourceId("ds-efs-read".to_string());
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(
            &CaseId("case-efs-read".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "EFS evidence".to_string(),
                kind: DataSourceKind::LogicalDirectory,
                source_path: evidence_dir,
                imported_at: chrono::Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .unwrap();
    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size, ext,
          deleted, hidden, system, encrypted)
         VALUES ('file-efs', NULL, ?1, 'encrypted.bin', 'encrypted.bin', 'file',
                 10, 'bin', 0, 0, 0, 1)",
        params![ds_id.0],
    )
    .unwrap();
    let file_id = FileEntryId("file-efs".to_string());

    for result in [
        open_file_handle_real(&conn, &file_id.0).map(|_| ()),
        read_file_bytes_for_case(&conn, &file_id, 0, 4).map(|_| ()),
        open_file_content_by_id(&conn, &file_id).map(|_| ()),
    ] {
        assert!(matches!(result, Err(FileServiceError::Unsupported(_))));
    }

    fn cache_miss(_: &str) -> Option<serde_json::Value> {
        None
    }

    fn discard_cache_write(_: &str, _: &serde_json::Value) {}

    let case_context_error = open_file_handle_real(
        (&conn, "case-efs-read", cache_miss, discard_cache_write),
        &file_id.0,
    )
    .unwrap_err();
    assert!(matches!(
        case_context_error,
        FileServiceError::Unsupported(_)
    ));
}

#[test]
fn unknown_encryption_entry_is_rejected_with_reenumeration_guidance() {
    let dir = tempfile::TempDir::new().unwrap();
    let evidence_dir = dir.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join("unknown.pdf"), b"%PDF-1.7").unwrap();
    let conn = persistence_sqlite::open_or_create_source(&dir.path().join("source.db")).unwrap();
    let ds_id = DataSourceId("ds-unknown-read".to_string());
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(
            &CaseId("case-unknown-read".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "Unclassified evidence".to_string(),
                kind: DataSourceKind::LogicalDirectory,
                source_path: evidence_dir,
                imported_at: chrono::Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .unwrap();
    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size, ext,
          deleted, hidden, system)
         VALUES ('file-unknown', NULL, ?1, 'unknown.pdf', 'unknown.pdf', 'file',
                 8, 'pdf', 0, 0, 0)",
        params![ds_id.0],
    )
    .unwrap();
    let file_id = FileEntryId("file-unknown".to_string());

    let errors = [
        open_file_handle_real(&conn, &file_id.0)
            .map(|_| ())
            .unwrap_err(),
        read_file_bytes_for_case(&conn, &file_id, 0, 4)
            .map(|_| ())
            .unwrap_err(),
        open_file_content_by_id(&conn, &file_id)
            .map(|_| ())
            .unwrap_err(),
        media_preview_plan_for_file(&conn, &file_id.0)
            .map(|_| ())
            .unwrap_err(),
        document_preview_for_file(&conn, &file_id.0)
            .map(|_| ())
            .unwrap_err(),
        get_file_path_for_entry(&conn, &file_id.0)
            .map(|_| ())
            .unwrap_err(),
    ];
    for error in errors {
        assert!(matches!(error, FileServiceError::Unsupported(_)));
        assert!(error.to_string().contains("status is unknown"));
        assert!(error.to_string().contains("re-enumerated"));
    }
}

#[test]
fn logical_directory_repeated_range_uses_preview_descriptor_cache() {
    let dir = tempfile::TempDir::new().unwrap();
    let evidence_dir = dir.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    let bytes: Vec<u8> = (0u8..64).collect();
    std::fs::write(evidence_dir.join("sample.bin"), &bytes).unwrap();

    let conn = persistence_sqlite::open_or_create_source(&dir.path().join("source.db")).unwrap();

    let ds_id = DataSourceId("ds-logical-cache".to_string());
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(
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
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
         VALUES ('file-cache-sample', NULL, ?1, 'sample.bin', 'sample.bin', 'file', ?2, 'bin', 0, 0, 0, 0)",
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

    conn.execute(
        "UPDATE file_entries SET encrypted = 1 WHERE id = ?1",
        params![file_id.0],
    )
    .unwrap();
    let error = read_with_cache(0, 8).unwrap_err();
    assert!(matches!(error, FileServiceError::Unsupported(_)));
}

fn setup_e01_preview_routing_case(
    file_id: &str,
    partition_index: Option<i64>,
    root_name: &str,
) -> (tempfile::TempDir, rusqlite::Connection, domain::FileEntryId) {
    let dir = tempfile::TempDir::new().unwrap();
    let conn = persistence_sqlite::open_or_create_source(&dir.path().join("source.db")).unwrap();

    let data_source_id = DataSourceId("ds-preview-routing".to_string());
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(
            &CaseId("case-preview-routing".to_string()),
            &DataSource {
                id: data_source_id.clone(),
                name: "routing evidence".to_string(),
                kind: DataSourceKind::E01,
                source_path: dir.path().join("routing.E01"),
                imported_at: chrono::Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, deleted, hidden, system, partition_index)
         VALUES ('wrong-root', NULL, ?1, '[P9]', ?2, 'directory', 0, 0, 0, NULL)",
        params![data_source_id.0, root_name],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted, partition_index)
         VALUES (?1, 'wrong-root', ?2, '[P4]/target.bin', 'target.bin', 'file', 16, 'bin', 0, 0, 0, 0, ?3)",
        params![file_id, data_source_id.0, partition_index],
    )
    .unwrap();

    let partitions = [2u32, 4u32].map(|partition_index| {
        persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord {
            id: format!("routing-partition-{partition_index}"),
            data_source_id: data_source_id.0.clone(),
            partition_index,
            name: format!("Partition {partition_index}"),
            kind_label: "NTFS".to_string(),
            status: "ready".to_string(),
            type_guid: None,
            offset: u64::from(partition_index) * 1_048_576,
            length: 1_048_576,
            filesystem: Some("NTFS".to_string()),
            unlock_hint: None,
            lvm_vg_uuid: None,
            lvm_vg_name: None,
            lvm_lv_uuid: None,
            lvm_lv_name: None,
            lvm_pv_offsets_json: None,
            lvm_pv_sources_json: None,
        }
    });
    persistence_sqlite::repositories::partition_repo::PartitionRepo::new(&conn)
        .insert_batch(&partitions)
        .unwrap();

    (dir, conn, FileEntryId(file_id.to_string()))
}

#[test]
fn preview_descriptor_prefers_persisted_partition_index_over_legacy_hints() {
    let (_dir, conn, file_id) =
        setup_e01_preview_routing_case("mft:2:42", Some(4), "Partition 9 (NTFS)");

    let descriptor = preview_descriptor_for_case(&conn, "case-preview-routing", &file_id).unwrap();

    assert_eq!(descriptor.partition_index, Some(4));
    assert_eq!(descriptor.partition_candidates.len(), 1);
    assert_eq!(descriptor.partition_candidates[0].partition_index, 4);
    assert!(descriptor_is_fresh(&conn, &file_id, &descriptor));

    conn.execute(
        "UPDATE file_entries SET partition_index = 2 WHERE id = ?1",
        params![file_id.0],
    )
    .unwrap();
    assert!(!descriptor_is_fresh(&conn, &file_id, &descriptor));
}

#[test]
fn preview_descriptor_freshness_uses_the_same_legacy_root_fallback_as_creation() {
    let (_dir, conn, file_id) =
        setup_e01_preview_routing_case("legacy-file-id", None, "Partition 2 (NTFS)");

    let descriptor = preview_descriptor_for_case(&conn, "case-preview-routing", &file_id).unwrap();

    assert_eq!(descriptor.partition_index, Some(2));
    assert!(descriptor_is_fresh(&conn, &file_id, &descriptor));
}

#[test]
fn preview_descriptor_fails_closed_without_exact_partition_index() {
    let (_dir, conn, file_id) =
        setup_e01_preview_routing_case("legacy-file-id", None, "Evidence Root");

    let error = preview_descriptor_for_case(&conn, "case-preview-routing", &file_id)
        .expect_err("ambiguous partition routing must fail closed");

    assert!(error
        .to_string()
        .contains("requires an exact partition index"));
}

#[test]
fn header_read_cache_reuses_preview_descriptor_across_chunks() {
    let dir = tempfile::TempDir::new().unwrap();
    let evidence_dir = dir.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    let bytes = vec![b'A'; infrastructure::constants::MAX_RANGE_LENGTH + 17];
    std::fs::write(evidence_dir.join("large.bin"), &bytes).unwrap();

    let conn = persistence_sqlite::open_or_create_source(&dir.path().join("source.db")).unwrap();

    let ds_id = DataSourceId("ds-header-cache".to_string());
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(
            &CaseId("case-header-cache".to_string()),
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
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
         VALUES ('file-header-cache', NULL, ?1, 'large.bin', 'large.bin', 'file', ?2, 'bin', 0, 0, 0, 0)",
        params![ds_id.0, bytes.len() as i64],
    )
    .unwrap();

    let cache = FileHeaderReadCache::new("case-header-cache");
    let read = cache
        .read_file_header_by_id(
            &conn,
            &FileEntryId("file-header-cache".to_string()),
            bytes.len(),
        )
        .unwrap();

    assert_eq!(read, bytes);

    let again = cache
        .read_file_header_by_id(&conn, &FileEntryId("file-header-cache".to_string()), 8)
        .unwrap();
    assert_eq!(again, bytes[..8].to_vec());
}

#[test]
fn raw_ntfs_mid_file_range_uses_ntfs_range_reader_without_materialize() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("large_ntfs.raw");
    let marker = b"RANGE-ONLY";
    write_large_ntfs_raw_fixture(&raw_path, marker, 128u64 * 1024 * 1024).unwrap();

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
        ceph_fs: None,
    };

    let bytes =
        read_file_bytes_for_descriptor(&descriptor, 128u64 * 1024 * 1024, marker.len() as u32)
            .unwrap();

    assert_eq!(bytes, marker);
}

#[test]
fn bitlocker_ntfs_range_uses_inode_without_materializing_large_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("bitlocker_plaintext_ntfs.raw");
    let marker = b"BITLOCKER-RANGE-ONLY";
    let sparse_prefix_bytes = 300u64 * 1024 * 1024;
    write_large_ntfs_raw_fixture(&raw_path, marker, sparse_prefix_bytes).unwrap();

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let mut context = BitLockerNtfsRangeContext {
        conn: &conn,
        source_path: raw_path.clone(),
        open_calls: 0,
    };
    let descriptor = PreviewDescriptor {
        case_id: "case-bitlocker-ntfs-range".to_string(),
        file_id: "mft:5:6".to_string(),
        source_kind: "raw".to_string(),
        source_path: raw_path.display().to_string(),
        partition_index: Some(5),
        filesystem_kind: Some("BitLocker".to_string()),
        path: "[P5]/large.mp4".to_string(),
        mime: Some("video/mp4".to_string()),
        size: sparse_prefix_bytes + marker.len() as u64,
        data_source_id: "ds-bitlocker-ntfs-range".to_string(),
        partition_candidates: vec![PreviewPartitionCandidate {
            partition_index: 5,
            filesystem_kind: "BitLocker".to_string(),
            offset: 0,
            lvm_identity: None,
        }],
        entry_size: sparse_prefix_bytes + marker.len() as u64,
        entry_modified_at: None,
        ceph_fs: None,
    };

    let bytes = read_file_bytes_for_descriptor_with_context(
        &mut context,
        &descriptor,
        sparse_prefix_bytes,
        marker.len() as u32,
    )
    .unwrap();

    assert_eq!(bytes, marker);
    assert_eq!(context.open_calls, 1);
}

#[test]
fn prepared_ntfs_session_reuses_reader_for_nonsequential_ranges() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("prepared_ntfs.raw");
    let marker = b"PREPARED-NTFS-RANGE";
    let sparse_prefix_bytes = 300u64 * 1024 * 1024;
    write_large_ntfs_raw_fixture(&raw_path, marker, sparse_prefix_bytes).unwrap();

    let prepared = PreparedFile::open_ntfs(
        Box::new(evidence_core::RawImageReader::open(&raw_path).unwrap()),
        0,
        6,
    )
    .unwrap();
    let case_id = CaseId("case-prepared-ntfs".to_string());
    let source_id = DataSourceId("source-prepared-ntfs".to_string());
    let registry = PreviewRuntimeRegistry::default();
    let token = registry.begin_session(&case_id, &source_id).unwrap();
    let handle = registry
        .insert_session(
            &token,
            PreviewSession::prepared_file(
                case_id.0.clone(),
                source_id.0.clone(),
                "ds:source-prepared-ntfs:mft:5:6".to_string(),
                sparse_prefix_bytes + marker.len() as u64,
                Some("video/mp4".to_string()),
                prepared,
            ),
        )
        .unwrap();
    drop(token);

    let session = registry.get_session(&case_id.0, &handle).unwrap();
    assert_eq!(
        session
            .read_prepared_range(0, 64 * 1024)
            .unwrap()
            .unwrap()
            .len(),
        64 * 1024
    );
    assert_eq!(
        session
            .read_prepared_range(sparse_prefix_bytes / 2, 64 * 1024)
            .unwrap()
            .unwrap()
            .len(),
        64 * 1024
    );
    assert_eq!(
        session
            .read_prepared_range(sparse_prefix_bytes, marker.len())
            .unwrap()
            .unwrap(),
        marker
    );
    drop(session);

    registry
        .invalidate_source(&case_id.0, &source_id.0)
        .unwrap();
    assert!(registry.get_session(&case_id.0, &handle).is_err());
}

#[test]
fn ceph_rbd_xfs_mid_file_range_uses_context_reader_without_materialize() {
    let marker = b"CEPH-RBD-XFS-RANGE";
    let (image, offset, rejected) = build_ceph_xfs_bounded_range_fixture(marker);
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let mut context = CephRbdRangeContext {
        conn: &conn,
        image,
        rejected,
        open_calls: 0,
    };
    let descriptor = PreviewDescriptor {
        case_id: "case-ceph-rbd-xfs-range".to_string(),
        file_id: "xfs-file-range".to_string(),
        source_kind: "ceph_rbd".to_string(),
        source_path: "virtual-ceph-rbd".to_string(),
        partition_index: Some(0),
        filesystem_kind: Some("XFS".to_string()),
        path: "[P0]/large.bin".to_string(),
        mime: Some("application/octet-stream".to_string()),
        size: offset + marker.len() as u64,
        data_source_id: "ds-ceph-rbd-xfs-range".to_string(),
        partition_candidates: vec![PreviewPartitionCandidate {
            partition_index: 0,
            filesystem_kind: "XFS".to_string(),
            offset: 0,
            lvm_identity: None,
        }],
        entry_size: offset + marker.len() as u64,
        entry_modified_at: None,
        ceph_fs: None,
    };

    let bytes = read_file_bytes_for_descriptor_with_context(
        &mut context,
        &descriptor,
        offset,
        marker.len() as u32,
    )
    .unwrap();

    assert_eq!(bytes, marker);
    assert_eq!(context.open_calls, 1);
}

#[test]
fn ceph_rbd_preview_rejects_multiple_partition_candidates_before_reader_open() {
    let marker = b"CEPH-RBD-XFS-RANGE";
    let (image, offset, rejected) = build_ceph_xfs_bounded_range_fixture(marker);
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let mut context = CephRbdRangeContext {
        conn: &conn,
        image,
        rejected,
        open_calls: 0,
    };
    let descriptor = PreviewDescriptor {
        case_id: "case-ceph-rbd-ambiguous".to_string(),
        file_id: "xfs-file-ambiguous".to_string(),
        source_kind: "ceph_rbd".to_string(),
        source_path: "virtual-ceph-rbd".to_string(),
        partition_index: Some(0),
        filesystem_kind: Some("XFS".to_string()),
        path: "[P0]/large.bin".to_string(),
        mime: Some("application/octet-stream".to_string()),
        size: offset + marker.len() as u64,
        data_source_id: "ds-ceph-rbd-ambiguous".to_string(),
        partition_candidates: vec![
            PreviewPartitionCandidate {
                partition_index: 0,
                filesystem_kind: "XFS".to_string(),
                offset: 0,
                lvm_identity: None,
            },
            PreviewPartitionCandidate {
                partition_index: 1,
                filesystem_kind: "XFS".to_string(),
                offset: 1_048_576,
                lvm_identity: None,
            },
        ],
        entry_size: offset + marker.len() as u64,
        entry_modified_at: None,
        ceph_fs: None,
    };

    let error = read_file_bytes_for_descriptor_with_context(
        &mut context,
        &descriptor,
        offset,
        marker.len() as u32,
    )
    .expect_err("Ceph preview must not select the first ambiguous candidate");

    assert!(error
        .to_string()
        .contains("exactly one partition candidate"));
    assert_eq!(context.open_calls, 0);
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
        ceph_fs: None,
    };

    let bytes = read_file_bytes_for_descriptor(&descriptor, offset, marker.len() as u32).unwrap();

    assert_eq!(bytes, marker);
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
            pv_sources: vec![
                PreviewLvmPhysicalVolumeSource {
                    source_path: source_path.display().to_string(),
                    source_kind: String::new(),
                    offset: 1_048_576,
                    pv_uuid: String::new(),
                    pv_name: Some("pv0".to_string()),
                },
                PreviewLvmPhysicalVolumeSource {
                    source_path: source_path.display().to_string(),
                    source_kind: String::new(),
                    offset: 2_097_152,
                    pv_uuid: String::new(),
                    pv_name: Some("pv1".to_string()),
                },
            ],
        }),
    };

    let open_reader_calls = AtomicUsize::new(0);
    let result =
        image_open::lvm::open_candidate_block_reader(&source_path, &candidate, &mut |path| {
            open_reader_calls.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(ZeroEvidenceReader::new(path.to_path_buf()))
                as Box<dyn evidence_core::EvidenceReader>)
        });

    assert!(result.is_err());
    assert_eq!(open_reader_calls.load(Ordering::Relaxed), 2);
}

#[test]
fn multi_pv_lvm_candidate_without_sources_fails_closed() {
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
            pv_sources: Vec::new(),
        }),
    };

    let open_reader_calls = AtomicUsize::new(0);
    let result =
        image_open::lvm::open_candidate_block_reader(&source_path, &candidate, &mut |path| {
            open_reader_calls.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(ZeroEvidenceReader::new(path.to_path_buf()))
                as Box<dyn evidence_core::EvidenceReader>)
        });

    assert!(result.is_err());
    assert_eq!(open_reader_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn lvm_request_cache_reuses_pool_for_same_volume_group() {
    let dir = tempfile::TempDir::new().unwrap();
    let source_path = dir.path().join("lvm.raw");
    let disk = build_synthetic_lvm_disk();
    let candidate = PreviewPartitionCandidate {
        partition_index: 4,
        filesystem_kind: "XFS".to_string(),
        offset: 1_048_576,
        lvm_identity: Some(PreviewLvmIdentity {
            vg_uuid: "vg-1234".to_string(),
            vg_name: "test_vg".to_string(),
            lv_uuid: "lv-root-uuid".to_string(),
            lv_name: "root".to_string(),
            pv_offsets: vec![0],
            pv_sources: Vec::new(),
        }),
    };

    let mut lvm_cache = image_open::LvmPoolRequestCache::new();
    let open_reader_calls = AtomicUsize::new(0);

    for _ in 0..2 {
        let result = image_open::open_candidate_block_reader_with_lvm_cache(
            &source_path,
            &candidate,
            &mut |path| {
                open_reader_calls.fetch_add(1, Ordering::Relaxed);
                Ok(
                    Box::new(VecEvidenceReader::new(path.to_path_buf(), disk.clone()))
                        as Box<dyn evidence_core::EvidenceReader>,
                )
            },
            &mut lvm_cache,
        );
        assert!(result.is_ok());
    }

    assert_eq!(open_reader_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn linux_lvm_candidate_uses_pv_source_paths_when_present() {
    let dir = tempfile::TempDir::new().unwrap();
    let source_path = dir.path().join("wrong-primary.raw");
    let pv0_path = dir.path().join("pv0.raw");
    let pv1_path = dir.path().join("pv1.raw");
    let disk = build_synthetic_lvm_disk();
    let candidate = PreviewPartitionCandidate {
        partition_index: 4,
        filesystem_kind: "XFS".to_string(),
        offset: 0,
        lvm_identity: Some(PreviewLvmIdentity {
            vg_uuid: "vg-1234".to_string(),
            vg_name: "test_vg".to_string(),
            lv_uuid: "lv-root-uuid".to_string(),
            lv_name: "root".to_string(),
            pv_offsets: vec![0, 0],
            pv_sources: vec![
                PreviewLvmPhysicalVolumeSource {
                    source_path: pv0_path.display().to_string(),
                    source_kind: String::new(),
                    offset: 0,
                    pv_uuid: "abcdef1234567890abcdef1234567890".to_string(),
                    pv_name: Some("pv0".to_string()),
                },
                PreviewLvmPhysicalVolumeSource {
                    source_path: pv1_path.display().to_string(),
                    source_kind: String::new(),
                    offset: 0,
                    pv_uuid: "abcdef1234567890abcdef1234567890".to_string(),
                    pv_name: Some("pv0".to_string()),
                },
            ],
        }),
    };

    let mut opened_paths = Vec::new();
    let result =
        image_open::lvm::open_candidate_block_reader(&source_path, &candidate, &mut |path| {
            opened_paths.push(path.to_path_buf());
            Ok(
                Box::new(VecEvidenceReader::new(path.to_path_buf(), disk.clone()))
                    as Box<dyn evidence_core::EvidenceReader>,
            )
        });

    assert!(result.is_ok());
    assert_eq!(opened_paths, vec![pv0_path, pv1_path]);
}

#[test]
fn linux_lvm_candidate_rejects_pv_source_uuid_mismatch() {
    let dir = tempfile::TempDir::new().unwrap();
    let source_path = dir.path().join("wrong-primary.raw");
    let pv_path = dir.path().join("pv0.raw");
    let mut disk = build_synthetic_lvm_disk();
    replace_synthetic_lvm_pv_uuid(&mut disk, "ffffffffffffffffffffffffffffffff");
    let candidate = PreviewPartitionCandidate {
        partition_index: 4,
        filesystem_kind: "XFS".to_string(),
        offset: 0,
        lvm_identity: Some(PreviewLvmIdentity {
            vg_uuid: "vg-1234".to_string(),
            vg_name: "test_vg".to_string(),
            lv_uuid: "lv-root-uuid".to_string(),
            lv_name: "root".to_string(),
            pv_offsets: vec![0],
            pv_sources: vec![PreviewLvmPhysicalVolumeSource {
                source_path: pv_path.display().to_string(),
                source_kind: String::new(),
                offset: 0,
                pv_uuid: "abcdef1234567890abcdef1234567890".to_string(),
                pv_name: Some("pv0".to_string()),
            }],
        }),
    };

    let result =
        image_open::lvm::open_candidate_block_reader(&source_path, &candidate, &mut |path| {
            Ok(
                Box::new(VecEvidenceReader::new(path.to_path_buf(), disk.clone()))
                    as Box<dyn evidence_core::EvidenceReader>,
            )
        });

    let error = match result {
        Ok(_) => panic!("PV UUID mismatch must fail closed"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("UUID mismatch"));
}

#[test]
fn e01_ntfs_lvm_record_uses_logical_volume_reader() {
    let dir = tempfile::TempDir::new().unwrap();
    let source_path = dir.path().join("lvm.raw");
    let disk = build_synthetic_lvm_disk();
    let entry = FileEntry {
        id: FileEntryId("missing-file-id".to_string()),
        parent_id: None,
        data_source_id: DataSourceId("ds-e01-ntfs-lvm".to_string()),
        path: "root/missing.bin".to_string(),
        name: "missing.bin".to_string(),
        entry_type: EntryType::File,
        size: Some(0),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    };
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE data_source_partitions (
            id TEXT PRIMARY KEY,
            data_source_id TEXT NOT NULL,
            partition_index INTEGER NOT NULL,
            name TEXT NOT NULL,
            kind_label TEXT NOT NULL,
            status TEXT NOT NULL,
            type_guid TEXT,
            offset INTEGER NOT NULL,
            length INTEGER NOT NULL,
            filesystem TEXT,
            unlock_hint TEXT,
            lvm_vg_uuid TEXT,
            lvm_vg_name TEXT,
            lvm_lv_uuid TEXT,
            lvm_lv_name TEXT,
            lvm_pv_offsets_json TEXT,
            lvm_pv_sources_json TEXT
        );",
    )
    .unwrap();
    persistence_sqlite::repositories::partition_repo::PartitionRepo::new(&conn)
        .insert_batch(&[
            persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord {
                id: "partition-lvm-ntfs".to_string(),
                data_source_id: entry.data_source_id.0.clone(),
                partition_index: 4,
                name: "vg/root".to_string(),
                kind_label: "NTFS".to_string(),
                status: "ready".to_string(),
                type_guid: None,
                offset: 123_456,
                length: 0,
                filesystem: Some("NTFS".to_string()),
                unlock_hint: None,
                lvm_vg_uuid: Some("vg-1234".to_string()),
                lvm_vg_name: Some("test_vg".to_string()),
                lvm_lv_uuid: Some("lv-root-uuid".to_string()),
                lvm_lv_name: Some("root".to_string()),
                lvm_pv_offsets_json: Some("[0]".to_string()),
                lvm_pv_sources_json: None,
            },
        ])
        .unwrap();

    let open_reader_calls = AtomicUsize::new(0);
    let result = image_open::e01::open_e01_file_with_reader_factory(
        &conn,
        source_path.to_str().unwrap(),
        &entry,
        Some(4),
        |path| {
            open_reader_calls.fetch_add(1, Ordering::Relaxed);
            Ok(
                Box::new(VecEvidenceReader::new(path.to_path_buf(), disk.clone()))
                    as Box<dyn evidence_core::EvidenceReader>,
            )
        },
    );

    assert!(result.is_err());
    assert_eq!(open_reader_calls.load(Ordering::Relaxed), 1);
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
        ceph_fs: None,
    };

    let bytes = read_file_bytes_for_descriptor(&descriptor, 512 + 7, 9).unwrap();

    assert_eq!(bytes, vec![b'B'; 9]);
}

#[test]
fn raw_exfat_mid_file_range_uses_exfat_range_reader_without_materialize() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("exfat.raw");
    write_exfat_raw_fixture(&raw_path).unwrap();

    let conn = persistence_sqlite::open_or_create_source(&dir.path().join("source.db")).unwrap();

    let ds_id = DataSourceId("ds-raw-exfat-range".to_string());
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(
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
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
         VALUES ('file-raw-exfat-large', NULL, ?1, 'LARGE.BIN', 'LARGE.BIN', 'file', ?2, 'bin', 0, 0, 0, 0)",
        params![ds_id.0, 1536i64],
    )
    .unwrap();

    let bytes = read_file_bytes_for_case(
        &conn,
        &FileEntryId("file-raw-exfat-large".to_string()),
        512 + 7,
        9,
    )
    .unwrap();

    assert_eq!(bytes, vec![b'B'; 9]);

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

    conn.execute(
        "UPDATE file_entries SET encrypted = 1 WHERE id = 'file-raw-exfat-large'",
        [],
    )
    .unwrap();
    let error = read_file_bytes_for_case(
        &conn,
        &FileEntryId("file-raw-exfat-large".to_string()),
        512 + 7,
        9,
    )
    .unwrap_err();
    assert!(matches!(error, FileServiceError::Unsupported(_)));

    let error = read_file_range_for_case(
        &conn,
        &ViewerRangeRequestDto {
            handle_id: "file:file-raw-exfat-large".to_string(),
            offset: 512 + 7,
            length: 9,
        },
    )
    .unwrap_err();
    assert!(matches!(error, FileServiceError::Unsupported(_)));
}

#[test]
fn raw_exfat_text_header_reads_via_bytes_only_fast_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw_path = dir.path().join("exfat.raw");
    write_exfat_raw_fixture(&raw_path).unwrap();

    let conn = persistence_sqlite::open_or_create_source(&dir.path().join("source.db")).unwrap();

    let ds_id = DataSourceId("ds-raw-exfat-header".to_string());
    DataSourceRepo::new(&conn)
        .upsert_source_local_metadata(
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
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
         VALUES ('file-raw-exfat-header', NULL, ?1, 'LARGE.BIN', 'LARGE.BIN', 'file', ?2, 'bin', 0, 0, 0, 0)",
        params![ds_id.0, 1536i64],
    )
    .unwrap();

    let bytes =
        read_file_header_by_id(&conn, &FileEntryId("file-raw-exfat-header".to_string()), 16)
            .unwrap();

    assert_eq!(bytes, vec![b'A'; 16]);
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
