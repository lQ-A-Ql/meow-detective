use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

use crate::crc32c;
use crate::EmulationError;

const MAGIC: &[u8; 8] = b"MDCOWREC";
const VERSION: u16 = 2;
const KIND_DATA: u16 = 1;
pub(crate) const RECORD_HEADER_SIZE: usize = 48;
pub(crate) const RECORD_ALIGNMENT: u64 = 4096;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DataPointer {
    pub(crate) data_offset: u64,
}

pub(crate) fn write_data_record(
    file: &mut File,
    cluster_index: u64,
    payload: &[u8],
) -> Result<DataPointer, EmulationError> {
    let record_offset = aligned_end(file)?;
    write_record(file, record_offset, cluster_index, payload)?;
    Ok(DataPointer {
        data_offset: record_offset + RECORD_HEADER_SIZE as u64,
    })
}

fn write_record(
    file: &mut File,
    offset: u64,
    key: u64,
    payload: &[u8],
) -> Result<(), EmulationError> {
    let total_length = (RECORD_HEADER_SIZE as u64)
        .checked_add(payload.len() as u64)
        .ok_or(EmulationError::ArithmeticOverflow)?;
    let mut header = [0u8; RECORD_HEADER_SIZE];
    header[..8].copy_from_slice(MAGIC);
    header[8..10].copy_from_slice(&KIND_DATA.to_le_bytes());
    header[10..12].copy_from_slice(&VERSION.to_le_bytes());
    header[12..16].copy_from_slice(&(RECORD_HEADER_SIZE as u32).to_le_bytes());
    // Bytes 16..24 are reserved (a generation counter lived here before the
    // commit protocol was removed).
    header[24..32].copy_from_slice(&key.to_le_bytes());
    header[32..36].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[36..44].copy_from_slice(&total_length.to_le_bytes());
    let checksum = record_checksum(&header, payload);
    header[44..48].copy_from_slice(&checksum.to_le_bytes());
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&header)?;
    file.write_all(payload)?;
    let aligned = align_up(offset + total_length)?;
    if aligned > offset + total_length {
        file.set_len(aligned)?;
    }
    Ok(())
}

fn aligned_end(file: &mut File) -> Result<u64, EmulationError> {
    let end = file.seek(SeekFrom::End(0))?;
    let aligned = align_up(end)?;
    if aligned != end {
        file.set_len(aligned)?;
    }
    Ok(aligned)
}

/// On-disk footprint of a record carrying `payload_length` bytes: the header,
/// the payload, and the alignment padding that follows them.
pub(crate) fn aligned_record_length(payload_length: usize) -> u64 {
    let value = RECORD_HEADER_SIZE as u64 + payload_length as u64;
    (value + RECORD_ALIGNMENT - 1) & !(RECORD_ALIGNMENT - 1)
}

fn record_checksum(header: &[u8; RECORD_HEADER_SIZE], payload: &[u8]) -> u32 {
    crc32c::checksum_parts(&[&header[..44], payload])
}

fn align_up(value: u64) -> Result<u64, EmulationError> {
    value
        .checked_add(RECORD_ALIGNMENT - 1)
        .map(|sum| sum & !(RECORD_ALIGNMENT - 1))
        .ok_or(EmulationError::ArithmeticOverflow)
}
