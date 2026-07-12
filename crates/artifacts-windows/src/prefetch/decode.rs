use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use std::io::{Cursor, Read};

const SIGNATURE: &[u8; 4] = b"SCCA";
const HEADER_SIZE: usize = 84;
const V17_INFO_SIZE: usize = 68;
const V23_INFO_SIZE: usize = 156;
const V26_INFO_SIZE: usize = 220;
const V30_INFO_SIZE: usize = 220;
const V30_ALTERNATE_INFO_SIZE: usize = 212;
const V31_INFO_SIZE: usize = 212;

pub(super) struct DecodedPrefetch {
    pub(super) format_version: u32,
    pub(super) executable: String,
    pub(super) run_count: u32,
    pub(super) hash: u32,
    pub(super) file_size: u32,
    pub(super) run_times: Vec<DateTime<Utc>>,
}

pub(super) fn decode_prefetch(data: &[u8]) -> Result<DecodedPrefetch, String> {
    if data.len() < HEADER_SIZE {
        return Err("Prefetch payload is truncated".to_string());
    }
    let mut reader = Cursor::new(data);
    let format_version = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let mut signature = [0u8; 4];
    reader
        .read_exact(&mut signature)
        .map_err(|error| error.to_string())?;
    if &signature != SIGNATURE {
        return Err("Not a Prefetch file".to_string());
    }
    let _unknown = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let file_size = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let executable = read_utf16le_string(&mut reader, 60).unwrap_or_else(|| "unknown".to_string());
    let hash = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let _flags = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let info = file_info(format_version, data)?;
    Ok(DecodedPrefetch {
        format_version,
        executable,
        run_count: read_run_count(format_version, info),
        hash,
        file_size,
        run_times: read_run_times(format_version, info),
    })
}

fn file_info(format_version: u32, data: &[u8]) -> Result<&[u8], String> {
    let size = match format_version {
        17 => V17_INFO_SIZE,
        23 => V23_INFO_SIZE,
        26 => V26_INFO_SIZE,
        30 => v30_info_size(data),
        31 => V31_INFO_SIZE,
        other => return Err(format!("Unsupported Prefetch format version: {other}")),
    };
    data.get(HEADER_SIZE..HEADER_SIZE + size)
        .ok_or_else(|| "Prefetch file information section is truncated".to_string())
}

fn v30_info_size(data: &[u8]) -> usize {
    if data.len() >= HEADER_SIZE + V30_ALTERNATE_INFO_SIZE {
        let alternate_count = read_u32(data, HEADER_SIZE + 116).unwrap_or(0);
        let standard_count = read_u32(data, HEADER_SIZE + 124).unwrap_or(0);
        let alternate_hash = read_u32(data, HEADER_SIZE + 128).unwrap_or(0);
        let standard_hash = read_u32(data, HEADER_SIZE + 136).unwrap_or(0);
        let alternate_valid = alternate_hash <= data.len() as u32 && alternate_count > 0;
        let standard_valid = standard_hash <= data.len() as u32 && standard_count > 0;
        if alternate_valid || !standard_valid {
            return V30_ALTERNATE_INFO_SIZE;
        }
    }
    V30_INFO_SIZE
}

fn read_run_count(version: u32, info: &[u8]) -> u32 {
    let offset = match version {
        17 => 60,
        23 => 68,
        26 => 124,
        30 | 31 if info.len() >= V30_INFO_SIZE => 124,
        30 | 31 => 116,
        _ => return 0,
    };
    read_u32(info, offset).unwrap_or(0)
}

fn read_run_times(version: u32, info: &[u8]) -> Vec<DateTime<Utc>> {
    let (offset, slots) = match version {
        17 => (36, 1),
        23 => (44, 1),
        26 | 30 | 31 => (44, 8),
        _ => return Vec::new(),
    };
    (0..slots)
        .filter_map(|index| filetime_to_datetime(read_u64(info, offset + index * 8)?))
        .filter(|timestamp| timestamp.year() > 2000 && timestamp.year() < 2100)
        .collect()
}

fn read_utf16le_string(reader: &mut impl Read, byte_len: usize) -> Option<String> {
    let mut data = vec![0u8; byte_len.min(256)];
    reader.read_exact(&mut data).ok()?;
    let end = data
        .chunks_exact(2)
        .position(|chunk| chunk == [0, 0])
        .map(|index| index * 2)
        .unwrap_or(data.len());
    let characters = data[..end]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&characters).ok()
}

fn filetime_to_datetime(value: u64) -> Option<DateTime<Utc>> {
    if value == 0 || value >= 0x8000_0000_0000_0000 {
        return None;
    }
    let seconds = (value / 10_000_000) as i64 - 11_644_473_600;
    Utc.timestamp_opt(seconds, ((value % 10_000_000) * 100) as u32)
        .single()
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}
