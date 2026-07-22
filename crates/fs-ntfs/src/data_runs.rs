//! NTFS data run parsing.

use crate::invalid_fs_data;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataRun {
    pub lcn: Option<i64>,
    pub cluster_count: u64,
}

/// Parse data runs and return only non-sparse runs as `(lcn, cluster_count)` pairs.
pub(crate) fn parse_data_runs_bytes(data: &[u8]) -> io::Result<Vec<(i64, u64)>> {
    Ok(parse_data_runs_ext(data)?
        .into_iter()
        .filter_map(|run| run.lcn.map(|lcn| (lcn, run.cluster_count)))
        .collect())
}

/// Parse NTFS data runs into a sequence of [`DataRun`] values.
pub(crate) fn parse_data_runs_ext(mut data: &[u8]) -> io::Result<Vec<DataRun>> {
    const MAX_DATA_RUNS: usize = 100_000;

    let mut runs = Vec::new();
    let mut prev_lcn: i64 = 0;
    while !data.is_empty() && data[0] != 0 {
        if runs.len() >= MAX_DATA_RUNS {
            return Err(invalid_fs_data(format!(
                "too many data runs (limit: {})",
                MAX_DATA_RUNS
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
        let lcn = if offset_bytes == 0 {
            None
        } else if runs.is_empty() {
            Some(lcn_offset)
        } else {
            Some(prev_lcn + lcn_offset)
        };
        if let Some(lcn) = lcn {
            prev_lcn = lcn;
        }
        if cluster_count == 0 {
            continue;
        }
        runs.push(DataRun { lcn, cluster_count });
    }
    Ok(runs)
}

/// Total logical size covered by a list of data runs.
pub(crate) fn data_runs_logical_size(runs: &[DataRun], cluster_size: u64) -> io::Result<u64> {
    let mut size = 0u64;
    for run in runs {
        let run_bytes = run
            .cluster_count
            .checked_mul(cluster_size)
            .ok_or_else(|| invalid_fs_data("data run logical size overflow"))?;
        size = size
            .checked_add(run_bytes)
            .ok_or_else(|| invalid_fs_data("data run logical size overflow"))?;
    }
    Ok(size)
}

/// Read a variable-width little-endian unsigned integer (1-8 bytes).
pub(crate) fn read_sized_le(bytes: &[u8]) -> u64 {
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate().take(8) {
        val |= (b as u64) << (i * 8);
    }
    val
}

/// Read a variable-width little-endian signed integer (1-8 bytes).
pub(crate) fn read_sized_le_signed(bytes: &[u8]) -> i64 {
    let n = bytes.len().min(8);
    if n == 0 {
        return 0;
    }
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate().take(n) {
        val |= (b as u64) << (i * 8);
    }
    // Sign-extend: if the highest bit of the last byte is set,
    // fill upper bytes with 0xFF.
    let last = bytes[n - 1];
    if last & 0x80 != 0 {
        for i in n..8 {
            val |= 0xFFu64 << (i * 8);
        }
    }
    val as i64
}

/// Parse the $DATA data runs from MFT record 0.
pub(crate) fn parse_mft_data_runs_from_record(record: &[u8]) -> io::Result<Vec<(i64, u64)>> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Err(invalid_fs_data("MFT record 0 is not a valid FILE record"));
    }

    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().unwrap_or([0; 4]));
        if typ == crate::ATTR_TYPE_END {
            break;
        }
        let len =
            u32::from_le_bytes(record[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
        if len == 0 || pos + len > record.len() {
            break;
        }

        if typ == 0x80 && pos + 0x40 <= record.len() && (record[pos + 8] & 1) != 0 {
            let run_off = u16::from_le_bytes([record[pos + 0x20], record[pos + 0x21]]) as usize;
            if run_off == 0 || run_off >= len {
                return Ok(Vec::new());
            }
            return parse_data_runs_bytes(&record[pos + run_off..pos + len]);
        }
        pos += len;
    }
    Ok(Vec::new())
}
