use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

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
    pub(crate) generation: u64,
    pub(crate) cluster_index: u64,
    pub(crate) record_offset: u64,
    pub(crate) pointer: DataPointer,
    pub(crate) payload_checksum: u32,
}

pub(crate) enum ParsedRecord {
    Data(PendingData),
    Commit {
        generation: u64,
        count: u64,
        digest: [u8; 32],
    },
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
        generation,
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

pub(crate) fn read_record(
    file: &mut File,
    offset: u64,
    file_length: u64,
    cluster_size: u32,
) -> Result<Option<(ParsedRecord, u64)>, EmulationError> {
    if offset == file_length || file_length.saturating_sub(offset) < RECORD_HEADER_SIZE as u64 {
        return Ok(None);
    }
    let mut header = [0u8; RECORD_HEADER_SIZE];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut header)?;
    validate_header(&header)?;
    let kind = u16_at(&header, 8);
    let generation = u64_at(&header, 16);
    let key = u64_at(&header, 24);
    let payload_length = u32_at(&header, 32) as usize;
    let total_length = u64_at(&header, 36);
    validate_record_lengths(kind, payload_length, total_length, cluster_size)?;
    let record_end = offset
        .checked_add(total_length)
        .ok_or(EmulationError::ArithmeticOverflow)?;
    if record_end > file_length {
        return Ok(None);
    }
    let mut payload = vec![0u8; payload_length];
    file.read_exact(&mut payload)?;
    let expected = u32_at(&header, 44);
    if record_checksum(&header, &payload) != expected {
        return Err(corrupt("record checksum mismatch"));
    }
    let next = align_up(record_end)?;
    let parsed = match kind {
        KIND_DATA => ParsedRecord::Data(PendingData {
            generation,
            cluster_index: key,
            record_offset: offset,
            pointer: DataPointer {
                data_offset: offset + RECORD_HEADER_SIZE as u64,
            },
            payload_checksum: crc32c::checksum(&payload),
        }),
        KIND_COMMIT => ParsedRecord::Commit {
            generation,
            count: key,
            digest: payload
                .try_into()
                .map_err(|_| corrupt("invalid commit digest length"))?,
        },
        _ => return Err(corrupt("unknown record type")),
    };
    Ok(Some((parsed, next)))
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

fn validate_header(header: &[u8; RECORD_HEADER_SIZE]) -> Result<(), EmulationError> {
    if &header[..8] != MAGIC
        || u16_at(header, 10) != VERSION
        || u32_at(header, 12) != RECORD_HEADER_SIZE as u32
    {
        return Err(corrupt("invalid record header"));
    }
    Ok(())
}

fn validate_record_lengths(
    kind: u16,
    payload_length: usize,
    total_length: u64,
    cluster_size: u32,
) -> Result<(), EmulationError> {
    let expected = match kind {
        KIND_DATA => cluster_size as usize,
        KIND_COMMIT => 32,
        _ => return Err(corrupt("unknown record type")),
    };
    if payload_length != expected || total_length != (RECORD_HEADER_SIZE + expected) as u64 {
        return Err(corrupt("record length does not match its type"));
    }
    Ok(())
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

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap_or([0; 2]))
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap_or([0; 4]))
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap_or([0; 8]))
}

fn corrupt(message: &str) -> EmulationError {
    EmulationError::CorruptOverlay(message.to_string())
}
