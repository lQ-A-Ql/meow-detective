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

/// Parse NTFS data runs into a sequence of lightweight values.
pub(crate) fn parse_data_runs_ext(data: &[u8]) -> io::Result<Vec<DataRun>> {
    let mut runs = Vec::new();
    crate::data_run_decoder::decode_data_runs(data, |run| {
        runs.push(DataRun {
            lcn: run.absolute_lcn,
            cluster_count: run.cluster_count,
        });
        Ok(())
    })?;
    Ok(runs)
}

/// Preserve raw fields only when the investigator requests the technical view.
pub(crate) fn parse_data_runs_forensic(
    data: &[u8],
    cluster_size: u64,
    volume_offset: u64,
) -> io::Result<Vec<crate::ForensicDataRun>> {
    if cluster_size == 0 {
        return Err(invalid_fs_data("zero NTFS cluster size"));
    }
    let mut logical_offset = 0u64;
    let mut runs = Vec::new();
    crate::data_run_decoder::decode_data_runs(data, |run| {
        let physical_offset = run
            .absolute_lcn
            .map(|lcn| {
                (lcn as u64)
                    .checked_mul(cluster_size)
                    .and_then(|offset| volume_offset.checked_add(offset))
                    .ok_or_else(|| invalid_fs_data("NTFS data run physical offset overflow"))
            })
            .transpose()?;
        let run_bytes = run
            .cluster_count
            .checked_mul(cluster_size)
            .ok_or_else(|| invalid_fs_data("NTFS data run size overflow"))?;
        runs.push(crate::ForensicDataRun {
            header: run.header,
            length_field_size: run.header & 0x0f,
            offset_field_size: run.header >> 4,
            cluster_count: run.cluster_count,
            relative_lcn: run.relative_lcn,
            absolute_lcn: run.absolute_lcn,
            logical_offset,
            raw: run.raw.to_vec(),
            physical_offset,
        });
        logical_offset = logical_offset
            .checked_add(run_bytes)
            .ok_or_else(|| invalid_fs_data("NTFS data run logical offset overflow"))?;
        Ok(())
    })?;
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
