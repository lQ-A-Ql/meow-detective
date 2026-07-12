use std::{io::SeekFrom, path::Path};

use evidence_core::EvidenceReader;
use image_e01::E01Reader;

pub(super) fn read_ntfs_mft_data_runs(
    e01_path: &Path,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
) -> std::io::Result<Vec<(i64, u64)>> {
    let mut reader = E01Reader::open(e01_path)?;
    let mut record = vec![0u8; record_size as usize];
    read_contiguous_ntfs_mft_stream(
        &mut reader,
        volume_offset,
        mft_cluster,
        cluster_size,
        0,
        &mut record,
    )?;
    apply_ntfs_record_fixup(&mut record, bytes_per_sector as usize)?;
    parse_ntfs_mft_data_runs_from_record(&record)
}

fn apply_ntfs_record_fixup(record: &mut [u8], sector_size: usize) -> std::io::Result<()> {
    if record.len() < 8 || sector_size < 2 {
        return Ok(());
    }
    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }
    let usa_bytes = usa_count.checked_mul(2).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid update sequence")
    })?;
    if usa_offset + usa_bytes > record.len() {
        return Err(invalid_data("update sequence array exceeds record length"));
    }

    let expected = [record[usa_offset], record[usa_offset + 1]];
    for index in 1..usa_count {
        let fixup_pos = index
            .checked_mul(sector_size)
            .and_then(|value| value.checked_sub(2))
            .ok_or_else(|| invalid_data("invalid fixup position"))?;
        if fixup_pos + 2 > record.len() {
            return Err(invalid_data("record too short for update sequence fixup"));
        }
        if record[fixup_pos..fixup_pos + 2] != expected {
            return Err(invalid_data("update sequence signature mismatch"));
        }
        let replacement = usa_offset + index * 2;
        record[fixup_pos] = record[replacement];
        record[fixup_pos + 1] = record[replacement + 1];
    }
    Ok(())
}

fn parse_ntfs_mft_data_runs_from_record(record: &[u8]) -> std::io::Result<Vec<(i64, u64)>> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Err(invalid_data("MFT record 0 is not a valid FILE record"));
    }

    let mut position = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while position + 8 < record.len() {
        let attribute_type = read_u32(record, position, "Invalid MFT attribute type")?;
        if attribute_type == 0xFFFF_FFFF {
            break;
        }
        let length = read_u32(record, position + 4, "Invalid MFT attribute length")? as usize;
        if length == 0 || position + length > record.len() {
            break;
        }
        if attribute_type == 0x80
            && position + 0x40 <= record.len()
            && (record[position + 8] & 1) != 0
        {
            let run_offset =
                u16::from_le_bytes([record[position + 0x20], record[position + 0x21]]) as usize;
            if run_offset == 0 || run_offset >= length {
                return Ok(Vec::new());
            }
            return parse_ntfs_data_runs(&record[position + run_offset..position + length]);
        }
        position += length;
    }
    Ok(Vec::new())
}

fn read_u32(record: &[u8], offset: usize, message: &str) -> std::io::Result<u32> {
    let bytes = record
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_data(message))?;
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| invalid_data(message))?,
    ))
}

pub fn parse_ntfs_data_runs(mut data: &[u8]) -> std::io::Result<Vec<(i64, u64)>> {
    const MAX_DATA_RUNS: usize = 100_000;
    let mut runs = Vec::new();
    let mut previous_lcn = 0i64;
    while !data.is_empty() && data[0] != 0 {
        if runs.len() >= MAX_DATA_RUNS {
            return Err(invalid_data(format!(
                "too many data runs (limit: {MAX_DATA_RUNS})"
            )));
        }
        let header = data[0];
        let size_bytes = (header & 0x0F) as usize;
        let offset_bytes = ((header >> 4) & 0x0F) as usize;
        if size_bytes > 8 || offset_bytes > 8 {
            break;
        }
        data = &data[1..];
        if data.len() < size_bytes + offset_bytes {
            break;
        }
        let cluster_count = read_sized_le(&data[..size_bytes]);
        data = &data[size_bytes..];
        let lcn_offset = read_sized_le_signed(&data[..offset_bytes]);
        data = &data[offset_bytes..];
        let lcn = if runs.is_empty() {
            lcn_offset
        } else {
            previous_lcn + lcn_offset
        };
        previous_lcn = lcn;
        if cluster_count != 0 {
            runs.push((lcn, cluster_count));
        }
    }
    Ok(runs)
}

fn read_sized_le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .enumerate()
        .take(8)
        .fold(0u64, |value, (index, byte)| {
            value | (*byte as u64) << (index * 8)
        })
}

fn read_sized_le_signed(bytes: &[u8]) -> i64 {
    let length = bytes.len().min(8);
    if length == 0 {
        return 0;
    }
    let mut value = read_sized_le(&bytes[..length]);
    if bytes[length - 1] & 0x80 != 0 {
        for index in length..8 {
            value |= 0xFFu64 << (index * 8);
        }
    }
    value as i64
}

pub fn read_ntfs_mft_stream(
    reader: &mut dyn EvidenceReader,
    volume_offset: u64,
    cluster_size: u64,
    runs: &[(i64, u64)],
    mut stream_offset: u64,
    out: &mut [u8],
) -> std::io::Result<()> {
    let mut written = 0usize;
    let mut run_stream_start = 0u64;
    for (lcn, cluster_count) in runs {
        if *lcn < 0 {
            return Err(invalid_data(format!("negative MFT LCN {lcn}")));
        }
        let run_bytes = cluster_count.checked_mul(cluster_size).ok_or_else(|| {
            invalid_data(format!(
                "MFT run overflow: {cluster_count} clusters x {cluster_size} bytes"
            ))
        })?;
        let run_end = run_stream_start.saturating_add(run_bytes);
        if stream_offset >= run_end {
            run_stream_start = run_end;
            continue;
        }
        let offset_in_run = stream_offset.saturating_sub(run_stream_start);
        let available = run_bytes.saturating_sub(offset_in_run);
        let to_read = available.min((out.len() - written) as u64) as usize;
        let disk_offset = volume_offset
            .checked_add((*lcn as u64).saturating_mul(cluster_size))
            .and_then(|base| base.checked_add(offset_in_run))
            .ok_or_else(|| invalid_data("MFT disk offset overflow"))?;
        reader.seek(SeekFrom::Start(disk_offset))?;
        reader.read_exact(&mut out[written..written + to_read])?;
        written += to_read;
        if written == out.len() {
            return Ok(());
        }
        stream_offset = run_end;
        run_stream_start = run_end;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        format!(
            "MFT stream ended before read completed (read {} of {} bytes)",
            written,
            out.len()
        ),
    ))
}

pub(super) fn read_contiguous_ntfs_mft_stream(
    reader: &mut dyn EvidenceReader,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    stream_offset: u64,
    out: &mut [u8],
) -> std::io::Result<()> {
    let absolute_offset = volume_offset
        .checked_add(
            mft_cluster
                .checked_mul(cluster_size)
                .ok_or_else(|| invalid_data("MFT absolute offset overflow"))?,
        )
        .and_then(|base| base.checked_add(stream_offset))
        .ok_or_else(|| invalid_data("MFT read offset overflow"))?;
    reader.seek(SeekFrom::Start(absolute_offset))?;
    reader.read_exact(out)
}

fn invalid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}
