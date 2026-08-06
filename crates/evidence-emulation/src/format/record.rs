use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

use sha2::{Digest, Sha256};

use crate::crc32c;
use crate::EmulationError;

const MAGIC: &[u8; 8] = b"MDCOWREC";
const VERSION: u16 = 1;
const KIND_DATA: u16 = 1;
const KIND_COMMIT: u16 = 2;
pub(crate) const RECORD_HEADER_SIZE: usize = 48;
pub(crate) const RECORD_ALIGNMENT: u64 = 4096;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DataPointer {
    pub(crate) data_offset: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingData {
    pub(crate) cluster_index: u64,
    pub(crate) record_offset: u64,
    pub(crate) pointer: DataPointer,
    pub(crate) payload_checksum: u32,
}

pub(crate) fn write_data_record(
    file: &mut File,
    generation: u64,
    cluster_index: u64,
    payload: &[u8],
) -> Result<PendingData, EmulationError> {
    let record_offset = aligned_end(file)?;
    let payload_checksum = crc32c::checksum(payload);
    write_record(
        file,
        record_offset,
        KIND_DATA,
        generation,
        cluster_index,
        payload,
    )?;
    Ok(PendingData {
        cluster_index,
        record_offset,
        pointer: DataPointer {
            data_offset: record_offset + RECORD_HEADER_SIZE as u64,
        },
        payload_checksum,
    })
}

pub(crate) fn write_commit_record(
    file: &mut File,
    generation: u64,
    pending: &[PendingData],
) -> Result<(), EmulationError> {
    let offset = aligned_end(file)?;
    let digest = commit_digest(generation, pending);
    write_record(
        file,
        offset,
        KIND_COMMIT,
        generation,
        pending.len() as u64,
        &digest,
    )
}

pub(crate) fn commit_digest(generation: u64, pending: &[PendingData]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(generation.to_le_bytes());
    digest.update((pending.len() as u64).to_le_bytes());
    for item in pending {
        digest.update(item.cluster_index.to_le_bytes());
        digest.update(item.record_offset.to_le_bytes());
        digest.update(item.payload_checksum.to_le_bytes());
    }
    digest.finalize().into()
}

fn write_record(
    file: &mut File,
    offset: u64,
    kind: u16,
    generation: u64,
    key: u64,
    payload: &[u8],
) -> Result<(), EmulationError> {
    let total_length = (RECORD_HEADER_SIZE as u64)
        .checked_add(payload.len() as u64)
        .ok_or(EmulationError::ArithmeticOverflow)?;
    let mut header = [0u8; RECORD_HEADER_SIZE];
    header[..8].copy_from_slice(MAGIC);
    header[8..10].copy_from_slice(&kind.to_le_bytes());
    header[10..12].copy_from_slice(&VERSION.to_le_bytes());
    header[12..16].copy_from_slice(&(RECORD_HEADER_SIZE as u32).to_le_bytes());
    header[16..24].copy_from_slice(&generation.to_le_bytes());
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

fn record_checksum(header: &[u8; RECORD_HEADER_SIZE], payload: &[u8]) -> u32 {
    crc32c::checksum_parts(&[&header[..44], payload])
}

fn align_up(value: u64) -> Result<u64, EmulationError> {
    value
        .checked_add(RECORD_ALIGNMENT - 1)
        .map(|sum| sum & !(RECORD_ALIGNMENT - 1))
        .ok_or(EmulationError::ArithmeticOverflow)
}
