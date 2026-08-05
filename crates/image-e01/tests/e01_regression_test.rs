use evidence_core::EvidenceReader;
use image_e01::E01Reader;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

fn real_e01_path() -> PathBuf {
    std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set FORENSICS_E01_FIXTURE to run ignored real E01 tests"))
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn open_real_file() {
    let r = E01Reader::open(&real_e01_path()).unwrap();
    assert!(r.info().size > 0);
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn read_first_sector() {
    let mut r = E01Reader::open(&real_e01_path()).unwrap();
    let mut buf = [0u8; 512];
    r.read_exact(&mut buf).unwrap();
    // Verify we got non-zero data (first 4 bytes not all zero)
    let non_zero = buf[0..4].iter().any(|&b| b != 0);
    assert!(non_zero, "first sector is all zeros");
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn cross_chunk_4k() {
    let mut r = E01Reader::open(&real_e01_path()).unwrap();
    let mut s0 = [0u8; 512];
    r.read_exact(&mut s0).unwrap();
    r.seek(SeekFrom::Start(0)).unwrap();
    let mut cross = [0u8; 4096];
    let n = r.read(&mut cross).unwrap();
    assert_eq!(n, 4096);
    assert_eq!(&cross[0..4], &s0[0..4]);
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn seek_end_read_last() {
    let mut r = E01Reader::open(&real_e01_path()).unwrap();
    r.seek(SeekFrom::End(-512)).unwrap();
    let mut buf = [0u8; 512];
    r.read_exact(&mut buf).unwrap();
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn read_mid_image_block() {
    let mut r = E01Reader::open(&real_e01_path()).unwrap();
    let target = 4u64 * 1024 * 1024 * 1024;
    r.seek(SeekFrom::Start(target)).unwrap();
    let mut buf = vec![0u8; 4096];
    r.read_exact(&mut buf).unwrap();
    assert!(buf.iter().any(|&byte| byte != 0));
}

#[test]
fn opens_committed_tiny_e01_fixture() {
    let mut reader = E01Reader::open(&testing::fixtures::tiny_e01_image()).unwrap();
    assert_eq!(reader.info().kind, "e01");
    assert_eq!(reader.info().size, 8 * 512);

    let mut marker = [0u8; 12];
    reader.read_exact(&mut marker).unwrap();
    assert_eq!(&marker, b"FWB-TINY-E01");

    reader.seek(SeekFrom::Start(510)).unwrap();
    let mut signature = [0u8; 2];
    reader.read_exact(&mut signature).unwrap();
    assert_eq!(signature, [0x55, 0xAA]);
}

#[test]
fn tiny_e01_fixture_supports_seek_and_eof() {
    let mut reader = E01Reader::open(&testing::fixtures::tiny_e01_image()).unwrap();

    reader.seek(SeekFrom::End(-4)).unwrap();
    let mut tail = [0xFF; 4];
    reader.read_exact(&mut tail).unwrap();
    assert_eq!(tail, [0, 0, 0, 0]);

    let pos = reader.seek(SeekFrom::End(10)).unwrap();
    assert_eq!(pos, 8 * 512);
    let mut byte = [0u8; 1];
    let read = reader.read(&mut byte).unwrap();
    assert_eq!(read, 0);
}

// --- Phase 18: multi-segment detection + regression ---

#[test]
fn multi_segment_with_only_first_file_works() {
    // Verify that when only .E01 exists (no .E02+), single-segment
    // mode still works correctly — the segment detection logic
    // assigns all chunks to segment 0.
    use std::io::Write;

    let dir = std::env::temp_dir().join("e01_single_seg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let e01_path = dir.join("test.E01");

    let chunk_sectors: u32 = 8;
    let sectors: u64 = 8;
    let chunk_bytes = (chunk_sectors * 512) as usize;

    let mut f = std::fs::File::create(&e01_path).unwrap();
    f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")
        .unwrap();

    // volume section: desc at 13, content at 89, next desc at 89+36=125
    let mut vol = vec![0u8; 36];
    vol[8..12].copy_from_slice(&chunk_sectors.to_le_bytes());
    vol[12..16].copy_from_slice(&512u32.to_le_bytes());
    vol[16..24].copy_from_slice(&sectors.to_le_bytes());
    f.write_all(&sdesc("volume", 125, 76 + 36)).unwrap();
    f.write_all(&vol).unwrap();

    // table (1 chunk): v1 header 24 bytes + 1 entry 4 bytes + footer 4 bytes
    // desc at 125, content 32 bytes, next desc at 125+76+32=233
    let mut tbl = vec![0u8; 32];
    // number_of_entries
    tbl[0..4].copy_from_slice(&(1u32).to_le_bytes());
    // data starts after done desc: 233 + 76 = 309
    let data_start: u64 = 233 + 76;
    // base_offset
    tbl[8..16].copy_from_slice(&data_start.to_le_bytes());
    // entry 0 at offset 24 = relative 0
    // footer checksum at 28..32 left zero for fixture simplicity
    f.write_all(&sdesc("table", 233, 76 + tbl.len() as u64))
        .unwrap();
    f.write_all(&tbl).unwrap();

    // done: desc at 233, next=0 (end of section chain)
    f.write_all(&sdesc("done", 0, 0)).unwrap();

    // chunk 0 at offset 309+: raw data after all sections
    let mut c0 = vec![0u8; chunk_bytes];
    c0[..5].copy_from_slice(b"HELLO");
    f.write_all(&c0).unwrap();
    f.flush().unwrap();
    drop(f);

    let mut r = E01Reader::open(&e01_path).unwrap();
    assert_eq!(r.info().size, sectors * 512);
    let mut buf = [0u8; 5];
    r.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"HELLO");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn prefers_primary_table_over_table2_duplicates() {
    use std::io::Write;

    let dir = std::env::temp_dir().join("e01_primary_table_only");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let e01_path = dir.join("test.E01");

    let chunk_sectors: u32 = 8;
    let sectors: u64 = 16;
    let chunk_bytes = (chunk_sectors * 512) as usize;

    let mut f = std::fs::File::create(&e01_path).unwrap();
    f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")
        .unwrap();

    let mut disk = vec![0u8; 1052];
    disk[0] = 1;
    disk[4..8].copy_from_slice(&(2u32).to_le_bytes());
    disk[8..12].copy_from_slice(&chunk_sectors.to_le_bytes());
    disk[12..16].copy_from_slice(&(512u32).to_le_bytes());
    disk[16..24].copy_from_slice(&sectors.to_le_bytes());

    let disk_desc_off = 13u64;
    let table_len = 24 + 2 * 4 + 4;
    let table_desc_off = disk_desc_off + 76 + disk.len() as u64;
    let table2_desc_off = table_desc_off + 76 + table_len;
    let done_desc_off = table2_desc_off + 76 + table_len;
    let chunk0_off = done_desc_off + 76;
    f.write_all(&sdesc("disk", table_desc_off, 76 + disk.len() as u64))
        .unwrap();
    f.write_all(&disk).unwrap();

    let mut table = vec![0u8; table_len as usize];
    table[0..4].copy_from_slice(&(2u32).to_le_bytes());
    table[8..16].copy_from_slice(&(chunk0_off).to_le_bytes());
    table[24..28].copy_from_slice(&(0u32).to_le_bytes());
    table[28..32].copy_from_slice(&(chunk_bytes as u32).to_le_bytes());
    f.write_all(&sdesc("table", table2_desc_off, 76 + table.len() as u64))
        .unwrap();
    f.write_all(&table).unwrap();

    let mut table2 = vec![0u8; table_len as usize];
    table2[0..4].copy_from_slice(&(2u32).to_le_bytes());
    table2[8..16].copy_from_slice(&(chunk0_off).to_le_bytes());
    table2[24..28].copy_from_slice(&(0u32).to_le_bytes());
    table2[28..32].copy_from_slice(&(chunk_bytes as u32).to_le_bytes());
    f.write_all(&sdesc("table2", done_desc_off, 76 + table2.len() as u64))
        .unwrap();
    f.write_all(&table2).unwrap();

    f.write_all(&sdesc("done", 0, 0)).unwrap();

    let mut c0 = vec![0u8; chunk_bytes];
    c0[..4].copy_from_slice(b"AAAA");
    let mut c1 = vec![0u8; chunk_bytes];
    c1[..4].copy_from_slice(b"BBBB");
    f.write_all(&c0).unwrap();
    f.write_all(&c1).unwrap();
    f.flush().unwrap();
    drop(f);

    let mut r = E01Reader::open(&e01_path).unwrap();
    let mut buf = vec![0u8; chunk_bytes * 2];
    r.read_exact(&mut buf).unwrap();
    assert_eq!(&buf[..4], b"AAAA");
    assert_eq!(&buf[chunk_bytes..chunk_bytes + 4], b"BBBB");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reads_chunk_tables_from_each_segment() {
    let dir = std::env::temp_dir().join(format!("e01_two_segments_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let e01_path = dir.join("split.E01");
    let e02_path = dir.join("split.E02");

    write_single_chunk_segment(&e01_path, Some((8, 16)), b"FIRST").unwrap();
    write_single_chunk_segment(&e02_path, None, b"SECOND").unwrap();

    let mut reader = E01Reader::open(&e01_path).unwrap();
    let mut bytes = vec![0u8; 2 * 8 * 512];
    reader.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes[..5], b"FIRST");
    assert_eq!(&bytes[8 * 512..8 * 512 + 6], b"SECOND");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejects_incomplete_segment_chunk_table() {
    let dir = std::env::temp_dir().join(format!("e01_missing_segment_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let e01_path = dir.join("split.E01");
    write_single_chunk_segment(&e01_path, Some((8, 16)), b"FIRST").unwrap();

    let error = match E01Reader::open(&e01_path) {
        Ok(_) => panic!("incomplete chunk table was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .contains("chunk table count mismatch: expected 2, found 1"));

    let _ = std::fs::remove_dir_all(&dir);
}

fn write_single_chunk_segment(
    path: &std::path::Path,
    geometry: Option<(u32, u64)>,
    marker: &[u8],
) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;
    file.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")?;
    let first_section_offset = 13u64;
    let table_offset = first_section_offset + geometry.map(|_| 76 + 36).unwrap_or_default();
    let table_len = 24 + 4 + 4;
    let done_offset = table_offset + 76 + table_len;
    let chunk_offset = done_offset + 76;

    if let Some((chunk_sectors, sector_count)) = geometry {
        let mut volume = vec![0u8; 36];
        volume[8..12].copy_from_slice(&chunk_sectors.to_le_bytes());
        volume[12..16].copy_from_slice(&512u32.to_le_bytes());
        volume[16..24].copy_from_slice(&sector_count.to_le_bytes());
        file.write_all(&sdesc("volume", table_offset, 76 + volume.len() as u64))?;
        file.write_all(&volume)?;
    }

    let mut table = vec![0u8; table_len as usize];
    table[0..4].copy_from_slice(&1u32.to_le_bytes());
    table[8..16].copy_from_slice(&chunk_offset.to_le_bytes());
    file.write_all(&sdesc("table", done_offset, 76 + table_len))?;
    file.write_all(&table)?;
    file.write_all(&sdesc("done", 0, 0))?;

    let mut chunk = vec![0u8; 8 * 512];
    chunk[..marker.len()].copy_from_slice(marker);
    file.write_all(&chunk)?;
    file.flush()
}

fn sdesc(stype: &str, next: u64, size: u64) -> [u8; 76] {
    let mut d = [0u8; 76];
    let b = stype.as_bytes();
    let n = b.len().min(16);
    d[0..n].copy_from_slice(&b[..n]);
    d[16..24].copy_from_slice(&next.to_le_bytes());
    d[24..32].copy_from_slice(&size.to_le_bytes());
    d
}
