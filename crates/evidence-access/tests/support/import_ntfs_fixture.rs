//! Deterministic synthetic NTFS data volume with resident files and an empty root index.
//! It exercises MFT ingestion, not the cost of large real-world INDX backfill.
use std::io::{self, Write};
use std::path::Path;

pub const RECORD_BYTES: usize = 1024;
pub const MFT_OFFSET: usize = 4096;
pub const FILE_CONTENT: &[u8] = b"hello world\n";

pub fn image(record_count: usize) -> Vec<u8> {
    assert!((32..=1_100_000).contains(&record_count));
    let length = (MFT_OFFSET + record_count * RECORD_BYTES).div_ceil(4096) * 4096;
    let mut bytes = vec![0u8; length];
    bytes[..3].copy_from_slice(&[0xEB, 0x52, 0x90]);
    bytes[3..11].copy_from_slice(b"NTFS    ");
    bytes[11..13].copy_from_slice(&512u16.to_le_bytes());
    bytes[13] = 2;
    bytes[40..48].copy_from_slice(&((length / 512) as u64).to_le_bytes());
    bytes[48..56].copy_from_slice(&4u64.to_le_bytes());
    bytes[64] = (-10i8) as u8;
    bytes[68] = (-10i8) as u8;
    bytes[72..80].copy_from_slice(&0x1234u64.to_le_bytes());
    bytes[510..512].copy_from_slice(&[0x55, 0xAA]);
    for index in std::iter::once(0)
        .chain(std::iter::once(5))
        .chain(16..record_count)
    {
        let name = match index {
            0 => "$MFT".into(),
            5 => ".".into(),
            _ => format!("file-{index}.txt"),
        };
        let record = record(index, &name, record_count);
        let start = MFT_OFFSET + index * RECORD_BYTES;
        bytes[start..start + RECORD_BYTES].copy_from_slice(&record);
    }
    bytes
}

pub(super) fn record(index: usize, name: &str, record_count: usize) -> [u8; RECORD_BYTES] {
    let mut record = [0u8; RECORD_BYTES];
    record[..4].copy_from_slice(b"FILE");
    record[4..6].copy_from_slice(&48u16.to_le_bytes());
    record[6..8].copy_from_slice(&3u16.to_le_bytes());
    record[16..18].copy_from_slice(&7u16.to_le_bytes());
    record[20..22].copy_from_slice(&56u16.to_le_bytes());
    record[22..24].copy_from_slice(&(if index == 5 { 3u16 } else { 1u16 }).to_le_bytes());
    record[28..32].copy_from_slice(&(RECORD_BYTES as u32).to_le_bytes());
    record[44..48].copy_from_slice(&(index as u32).to_le_bytes());
    let mut si = [0u8; 72];
    for offset in [0, 8, 16, 24] {
        si[offset..offset + 8].copy_from_slice(&125_911_584_000_000_000u64.to_le_bytes());
    }
    let mut position = resident(&mut record, 56, 0x10, &si);
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let mut filename = vec![0u8; 66 + utf16.len() * 2];
    filename[..8].copy_from_slice(&(5u64 | (7u64 << 48)).to_le_bytes());
    for offset in [8, 16, 24, 32] {
        filename[offset..offset + 8].copy_from_slice(&125_911_584_000_000_000u64.to_le_bytes());
    }
    filename[64] = utf16.len() as u8;
    filename[65] = 1;
    for (i, character) in utf16.iter().enumerate() {
        filename[66 + i * 2..68 + i * 2].copy_from_slice(&character.to_le_bytes());
    }
    position = resident(&mut record, position, 0x30, &filename);
    if index == 0 {
        position = mft_data(&mut record, position, record_count);
    } else if index == 5 {
        let mut root = [0u8; 48];
        root[..4].copy_from_slice(&0x30u32.to_le_bytes());
        root[8..12].copy_from_slice(&1024u32.to_le_bytes());
        root[16..20].copy_from_slice(&16u32.to_le_bytes());
        root[20..24].copy_from_slice(&32u32.to_le_bytes());
        root[24..28].copy_from_slice(&32u32.to_le_bytes());
        root[40..42].copy_from_slice(&16u16.to_le_bytes());
        root[44..46].copy_from_slice(&2u16.to_le_bytes());
        position = resident(&mut record, position, 0x90, &root);
    } else {
        position = resident(&mut record, position, 0x80, FILE_CONTENT);
    }
    record[position..position + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    record[24..28].copy_from_slice(&((position + 4) as u32).to_le_bytes());
    record[48..50].copy_from_slice(&[0x55, 0xAA]);
    for (tail, replacement) in [(510, 50), (1022, 52)] {
        let original = [record[tail], record[tail + 1]];
        record[replacement..replacement + 2].copy_from_slice(&original);
        record[tail..tail + 2].copy_from_slice(&[0x55, 0xAA]);
    }
    record
}

pub(super) fn resident(record: &mut [u8], start: usize, kind: u32, data: &[u8]) -> usize {
    let length = (24 + data.len()).div_ceil(8) * 8;
    record[start..start + 4].copy_from_slice(&kind.to_le_bytes());
    record[start + 4..start + 8].copy_from_slice(&(length as u32).to_le_bytes());
    record[start + 16..start + 20].copy_from_slice(&(data.len() as u32).to_le_bytes());
    record[start + 20..start + 22].copy_from_slice(&24u16.to_le_bytes());
    record[start + 24..start + 24 + data.len()].copy_from_slice(data);
    start + length
}

fn mft_data(record: &mut [u8], start: usize, count: usize) -> usize {
    record[start..start + 4].copy_from_slice(&0x80u32.to_le_bytes());
    record[start + 4..start + 8].copy_from_slice(&72u32.to_le_bytes());
    record[start + 8] = 1;
    record[start + 24..start + 32].copy_from_slice(&((count - 1) as u64).to_le_bytes());
    record[start + 32..start + 34].copy_from_slice(&64u16.to_le_bytes());
    for offset in [40, 48, 56] {
        record[start + offset..start + offset + 8]
            .copy_from_slice(&((count * RECORD_BYTES) as u64).to_le_bytes());
    }
    record[start + 64] = 0x13;
    record[start + 65..start + 68].copy_from_slice(&(count as u32).to_le_bytes()[..3]);
    record[start + 68] = 4;
    start + 72
}

pub fn write_e01(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let chunks: Vec<Vec<u8>> = bytes
        .chunks(4096)
        .map(|data| {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(data)?;
            encoder.finish()
        })
        .collect::<io::Result<_>>()?;
    let mut file = std::fs::File::create(path)?;
    file.write_all(b"EVF\t\r\n\x01\x00\x00\x01\x00\x01\x00")?;
    let mut volume = [0u8; 36];
    volume[8..12].copy_from_slice(&8u32.to_le_bytes());
    volume[12..16].copy_from_slice(&512u32.to_le_bytes());
    volume[16..24].copy_from_slice(&((bytes.len() / 512) as u64).to_le_bytes());
    let table_offset = 13 + 76 + 36;
    let table_length = 24 + chunks.len() * 4 + 4;
    let done_offset = table_offset + 76 + table_length;
    let data_offset = done_offset + 76;
    file.write_all(&section("volume", table_offset as u64, 112))?;
    file.write_all(&volume)?;
    let mut table = vec![0u8; table_length];
    table[..4].copy_from_slice(&(chunks.len() as u32).to_le_bytes());
    table[8..16].copy_from_slice(&(data_offset as u64).to_le_bytes());
    let mut relative = 0u32;
    for (index, chunk) in chunks.iter().enumerate() {
        table[24 + index * 4..28 + index * 4]
            .copy_from_slice(&(relative | 0x8000_0000).to_le_bytes());
        relative += chunk.len() as u32;
    }
    file.write_all(&section(
        "table",
        done_offset as u64,
        (76 + table_length) as u64,
    ))?;
    file.write_all(&table)?;
    file.write_all(&section("done", 0, 0))?;
    for chunk in chunks {
        file.write_all(&chunk)?;
    }
    file.flush()?;
    drop(file);
    testing::ewf::finalize_synthetic_e01(path)
}

fn section(kind: &str, next: u64, length: u64) -> [u8; 76] {
    let mut descriptor = [0u8; 76];
    descriptor[..kind.len()].copy_from_slice(kind.as_bytes());
    descriptor[16..24].copy_from_slice(&next.to_le_bytes());
    descriptor[24..32].copy_from_slice(&length.to_le_bytes());
    descriptor
}
