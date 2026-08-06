use super::*;
use crate::table::{find_geometry, should_read_section_content, V1_TABLE_HEADER_SIZE};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

impl E01Reader {
    fn cached_chunk_indices_for_test(&self) -> Vec<u64> {
        self.chunk_cache.iter().map(|chunk| chunk.idx).collect()
    }

    fn cache_bytes_for_test(&self) -> usize {
        self.chunk_cache_bytes
    }
}

#[test]
fn test_build_segment_path_first() {
    let path = Path::new("/data/image.E01");
    let seg = build_segment_path(path, 1);
    assert_eq!(seg, Path::new("/data/image.E01"));
}

#[test]
fn test_build_segment_path_second() {
    let path = Path::new("/data/image.E01");
    let seg = build_segment_path(path, 2);
    assert_eq!(seg, Path::new("/data/image.E02"));
}

#[test]
fn test_build_segment_path_third() {
    let path = Path::new("/data/image.E01");
    let seg = build_segment_path(path, 3);
    assert_eq!(seg, Path::new("/data/image.E03"));
}

#[test]
fn test_build_segment_path_lowercase() {
    let path = Path::new("/data/image.e01");
    let seg = build_segment_path(path, 2);
    assert_eq!(seg, Path::new("/data/image.E02"));
}

#[test]
fn test_section_descriptor_size() {
    assert_eq!(SECTION_DESCRIPTOR_SIZE, 76);
}

#[test]
fn test_v1_table_header_size() {
    assert_eq!(crate::table::V1_TABLE_HEADER_SIZE, 24);
}

#[test]
fn large_compressed_disk_geometry_is_not_rejected_by_segment_size_ratio() {
    let mut disk = vec![0u8; 32];
    disk[8..12].copy_from_slice(&64u32.to_le_bytes());
    disk[12..16].copy_from_slice(&512u32.to_le_bytes());
    disk[16..24].copy_from_slice(&268_435_456u64.to_le_bytes());
    let sections = vec![("disk".to_string(), disk)];

    let geometry = find_geometry(&sections).unwrap();

    assert_eq!(geometry.sector_count, 268_435_456);
    assert_eq!(geometry.sectors_per_chunk, 64);
    assert_eq!(geometry.bytes_per_sector, 512);
    assert_eq!(geometry.total_bytes().unwrap(), 137_438_953_472);
}

#[test]
fn encase_volume_geometry_uses_chunk_sectors_at_offset_eight() {
    let mut volume = vec![0u8; 32];
    volume[8..12].copy_from_slice(&64u32.to_le_bytes());
    volume[12..16].copy_from_slice(&512u32.to_le_bytes());
    volume[16..24].copy_from_slice(&419_430_400u64.to_le_bytes());

    let geometry = find_geometry(&[("volume".to_string(), volume)]).unwrap();

    assert_eq!(geometry.sectors_per_chunk, 64);
    assert_eq!(geometry.bytes_per_sector, 512);
    assert_eq!(geometry.chunk_bytes().unwrap(), 32 * 1024);
    assert_eq!(geometry.total_bytes().unwrap(), 200 * 1024 * 1024 * 1024);
}

#[test]
fn non_enumerated_sector_size_is_accepted_when_bounded() {
    let mut volume = vec![0u8; 32];
    volume[8..12].copy_from_slice(&64u32.to_le_bytes());
    volume[12..16].copy_from_slice(&520u32.to_le_bytes());
    volume[16..24].copy_from_slice(&1024u64.to_le_bytes());

    let geometry = find_geometry(&[("volume".to_string(), volume)]).unwrap();

    assert_eq!(geometry.bytes_per_sector, 520);
    assert_eq!(geometry.chunk_bytes().unwrap(), 64 * 520);
}

#[test]
fn zero_sector_size_is_rejected_instead_of_guessed() {
    let mut volume = vec![0u8; 32];
    volume[8..12].copy_from_slice(&64u32.to_le_bytes());
    volume[16..24].copy_from_slice(&1024u64.to_le_bytes());

    let error = find_geometry(&[("volume".to_string(), volume)]).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("invalid sector size 0"));
}

#[test]
fn oversized_chunk_geometry_is_rejected() {
    let mut volume = vec![0u8; 32];
    volume[8..12].copy_from_slice(&32_769u32.to_le_bytes());
    volume[12..16].copy_from_slice(&4096u32.to_le_bytes());
    volume[16..24].copy_from_slice(&32_769u64.to_le_bytes());

    let error = find_geometry(&[("volume".to_string(), volume)]).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("above the 134217728 byte limit"));
}

#[test]
fn sectors_sections_are_not_loaded_into_memory_during_section_walk() {
    assert!(!should_read_section_content("sectors"));
    assert!(should_read_section_content("disk"));
    assert!(should_read_section_content("volume"));
    assert!(should_read_section_content("table"));
    assert!(should_read_section_content("table2"));
}

#[test]
fn sequential_reads_populate_bounded_neighbor_cache() {
    let dir = std::env::temp_dir().join("e01_cache_prefetch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cache.E01");
    write_multichunk_e01(&path, 6).unwrap();

    let mut reader = E01Reader::open(&path).unwrap();
    let chunk_bytes = reader.chunk_size_bytes() as usize;
    let mut buf = vec![0u8; chunk_bytes + 1];
    reader.read_exact(&mut buf).unwrap();

    let cached = reader.cached_chunk_indices_for_test();
    assert!(cached.contains(&0));
    assert!(cached.contains(&1));
    assert!(cached.contains(&2));
    assert!(cached.contains(&3));
    assert!(reader.cache_bytes_for_test() <= crate::reader::SEQUENTIAL_CACHE_MAX_BYTES);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn absolute_reads_keep_sequential_prefetch_when_offsets_are_contiguous() {
    let dir = std::env::temp_dir().join("e01_cache_absolute_reads");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cache.E01");
    write_multichunk_e01(&path, 6).unwrap();

    let mut reader = E01Reader::open(&path).unwrap();
    let chunk_bytes = reader.chunk_size_bytes() as usize;
    let mut first = vec![0u8; chunk_bytes];
    let mut second = vec![0u8; chunk_bytes];
    reader.read_exact_at(0, &mut first).unwrap();
    reader
        .read_exact_at(chunk_bytes as u64, &mut second)
        .unwrap();

    let cached = reader.cached_chunk_indices_for_test();
    assert!(cached.contains(&0));
    assert!(cached.contains(&1));
    assert!(cached.contains(&2));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn seek_resets_sequential_prefetch_hint() {
    let dir = std::env::temp_dir().join("e01_cache_seek_reset");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cache.E01");
    write_multichunk_e01(&path, 6).unwrap();

    let mut reader = E01Reader::open(&path).unwrap();
    let chunk_bytes = reader.chunk_size_bytes();
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte).unwrap();
    reader.seek(SeekFrom::Start(chunk_bytes * 4)).unwrap();
    reader.read_exact(&mut byte).unwrap();

    let cached = reader.cached_chunk_indices_for_test();
    assert!(cached.contains(&0));
    assert!(cached.contains(&4));
    assert!(!cached.contains(&5));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cloned_reader_reads_same_data() {
    let dir = std::env::temp_dir().join("e01_clone");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("clone.E01");
    write_multichunk_e01(&path, 6).unwrap();

    let mut reader = E01Reader::open(&path).unwrap();
    let mut clone = reader.try_clone().unwrap();

    // Both should read the same data from the start.
    let chunk_bytes = reader.chunk_size_bytes() as usize;
    let mut buf1 = vec![0u8; chunk_bytes];
    let mut buf2 = vec![0u8; chunk_bytes];
    reader.read_exact(&mut buf1).unwrap();
    clone.read_exact(&mut buf2).unwrap();
    assert_eq!(buf1, buf2);

    // Seek the clone independently — should not affect the original.
    reader.seek(SeekFrom::Start(0)).unwrap();
    clone.seek(SeekFrom::Start(chunk_bytes as u64)).unwrap();
    let mut byte1 = [0u8; 1];
    let mut byte2 = [0u8; 1];
    reader.read_exact(&mut byte1).unwrap();
    clone.read_exact(&mut byte2).unwrap();
    assert_ne!(byte1, byte2); // different chunks

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compressed_chunk_length_error_contains_forensic_context() {
    let path = temporary_chunk_path("short-deflate");
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&[0x41; 32]).unwrap();
    let compressed = encoder.finish().unwrap();
    std::fs::write(&path, &compressed).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let mut reader = E01Reader::from_parts(
        evidence_core::ReaderInfo {
            path: path.clone(),
            size: 512,
            kind: "e01".to_string(),
        },
        512,
        1,
        512,
        vec![(0, 0, true, compressed.len() as u64)],
        vec![file],
    );

    let mut byte = [0u8; 1];
    let error = reader.read_exact(&mut byte).unwrap_err();
    drop(reader);
    std::fs::remove_file(path).unwrap();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    let message = error.to_string();
    assert!(message.contains("E01 chunk 0 codec=deflate"));
    assert!(message.contains("segment=0 offset=0"));
    assert!(message.contains("expected_decompressed_length=512"));
    assert!(message.contains("deflate output length was 32"));
}

#[test]
fn source_chunk_short_read_is_distinct_from_decode_failure() {
    let path = temporary_chunk_path("source-short-read");
    std::fs::write(&path, [0x42; 4]).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let mut reader = E01Reader::from_parts(
        evidence_core::ReaderInfo {
            path: path.clone(),
            size: 512,
            kind: "e01".to_string(),
        },
        512,
        1,
        512,
        vec![(0, 0, false, 512)],
        vec![file],
    );

    let mut byte = [0u8; 1];
    let error = reader.read_exact(&mut byte).unwrap_err();
    drop(reader);
    std::fs::remove_file(path).unwrap();

    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    let message = error.to_string();
    assert!(message.contains("E01 chunk 0 codec=raw"));
    assert!(message.contains("stored_length=512"));
    assert!(message.contains("stored range ends at 512, beyond segment length 4"));
}

#[test]
fn final_compressed_chunk_uses_remaining_logical_length() {
    let path = temporary_chunk_path("partial-final-deflate");
    let expected = vec![0x43; 512];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&expected).unwrap();
    let compressed = encoder.finish().unwrap();
    std::fs::write(&path, &compressed).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let mut reader = E01Reader::from_parts(
        evidence_core::ReaderInfo {
            path: path.clone(),
            size: 512,
            kind: "e01".to_string(),
        },
        512,
        64,
        512,
        vec![(0, 0, true, compressed.len() as u64)],
        vec![file],
    );

    let mut actual = Vec::new();
    reader.read_to_end(&mut actual).unwrap();
    drop(reader);
    std::fs::remove_file(path).unwrap();

    assert_eq!(actual, expected);
}

#[test]
fn invalid_raw_chunk_length_is_reported() {
    let path = temporary_chunk_path("invalid-raw-length");
    std::fs::write(&path, [0x44; 513]).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let mut reader = E01Reader::from_parts(
        evidence_core::ReaderInfo {
            path: path.clone(),
            size: 512,
            kind: "e01".to_string(),
        },
        512,
        1,
        512,
        vec![(0, 0, false, 513)],
        vec![file],
    );

    let mut byte = [0u8; 1];
    let error = reader.read_exact(&mut byte).unwrap_err();
    drop(reader);
    std::fs::remove_file(path).unwrap();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .contains("raw stored length must be 512 bytes"));
}

fn temporary_chunk_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("image-e01-{label}-{}.bin", std::process::id()))
}

fn write_multichunk_e01(path: &Path, chunk_count: u32) -> io::Result<()> {
    let chunk_sectors: u32 = 8;
    let sectors = chunk_count as u64 * chunk_sectors as u64;
    let chunk_bytes = (chunk_sectors * 512) as usize;

    let mut f = std::fs::File::create(path)?;
    f.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")?;

    let mut vol = vec![0u8; 36];
    vol[8..12].copy_from_slice(&chunk_sectors.to_le_bytes());
    vol[12..16].copy_from_slice(&512u32.to_le_bytes());
    vol[16..24].copy_from_slice(&sectors.to_le_bytes());

    let volume_desc_offset = 13u64;
    let table_desc_offset = volume_desc_offset + SECTION_DESCRIPTOR_SIZE + vol.len() as u64;
    let table_len = V1_TABLE_HEADER_SIZE + chunk_count as usize * 4 + 4;
    let done_desc_offset = table_desc_offset + SECTION_DESCRIPTOR_SIZE + table_len as u64;
    let chunk0_offset = done_desc_offset + SECTION_DESCRIPTOR_SIZE;

    f.write_all(&test_section_desc(
        "volume",
        table_desc_offset,
        SECTION_DESCRIPTOR_SIZE + vol.len() as u64,
    ))?;
    f.write_all(&vol)?;

    let mut table = vec![0u8; table_len];
    table[0..4].copy_from_slice(&chunk_count.to_le_bytes());
    table[8..16].copy_from_slice(&chunk0_offset.to_le_bytes());
    for idx in 0..chunk_count as usize {
        let rel = (idx * chunk_bytes) as u32;
        let pos = V1_TABLE_HEADER_SIZE + idx * 4;
        table[pos..pos + 4].copy_from_slice(&rel.to_le_bytes());
    }
    f.write_all(&test_section_desc(
        "table",
        done_desc_offset,
        SECTION_DESCRIPTOR_SIZE + table.len() as u64,
    ))?;
    f.write_all(&table)?;

    f.write_all(&test_section_desc("done", 0, 0))?;

    for idx in 0..chunk_count {
        let mut chunk = vec![idx as u8; chunk_bytes];
        chunk[0..4].copy_from_slice(&idx.to_le_bytes());
        f.write_all(&chunk)?;
    }
    f.flush()
}

fn test_section_desc(stype: &str, next: u64, size: u64) -> [u8; 76] {
    let mut desc = [0u8; 76];
    let bytes = stype.as_bytes();
    desc[0..bytes.len().min(16)].copy_from_slice(&bytes[..bytes.len().min(16)]);
    desc[16..24].copy_from_slice(&next.to_le_bytes());
    desc[24..32].copy_from_slice(&size.to_le_bytes());
    desc
}
