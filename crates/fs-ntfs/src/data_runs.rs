//! NTFS data run parsing.

use crate::invalid_fs_data;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataRun {
    pub lcn: Option<i64>,
    pub cluster_count: u64,
}

/// Parse allocated runs; metadata streams cannot discard sparse logical ranges.
pub fn parse_data_runs_bytes(data: &[u8]) -> io::Result<Vec<(i64, u64)>> {
    parse_data_runs_ext(data)?
        .into_iter()
        .map(|run| {
            run.lcn
                .map(|lcn| (lcn, run.cluster_count))
                .ok_or_else(|| invalid_fs_data("sparse NTFS metadata stream is unsupported"))
        })
        .collect()
}

/// Parse NTFS data runs into a sequence of lightweight values.
pub fn parse_data_runs_ext(data: &[u8]) -> io::Result<Vec<DataRun>> {
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

#[derive(Debug)]
pub struct MftDataAttribute {
    pub real_size: u64,
    pub runs: Vec<(i64, u64)>,
}

/// Read the unnamed, VCN-zero MFT stream through the common attribute decoder.
pub fn parse_mft_data_attribute(record: &[u8]) -> io::Result<MftDataAttribute> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Err(invalid_fs_data("MFT record 0 is not a valid FILE record"));
    }

    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    if attr_off < 0x18 || attr_off >= record.len() {
        return Err(invalid_fs_data("invalid MFT attribute offset"));
    }
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().unwrap_or([0; 4]));
        if typ == crate::ATTR_TYPE_END {
            break;
        }
        let len =
            u32::from_le_bytes(record[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
        if len < 0x18 || pos + len > record.len() {
            return Err(invalid_fs_data("invalid MFT attribute length"));
        }

        if typ == crate::ATTR_TYPE_DATA && record[pos + 9] == 0 {
            let extent = crate::attribute::parse_data_attribute_extent(record, pos, len)?;
            let Some(crate::DataAttributeExtent::NonResident {
                lowest_vcn: 0,
                real_size,
                attr_flags: 0,
                runs,
                ..
            }) = extent
            else {
                return Err(invalid_fs_data(
                    "MFT requires an allocated, nonresident VCN-zero stream",
                ));
            };
            let runs = runs
                .into_iter()
                .map(|run| {
                    run.lcn
                        .map(|lcn| (lcn, run.cluster_count))
                        .ok_or_else(|| invalid_fs_data("sparse MFT stream is unsupported"))
                })
                .collect::<io::Result<Vec<_>>>()?;
            if runs.is_empty() {
                return Err(invalid_fs_data("MFT stream has no data runs"));
            }
            return Ok(MftDataAttribute { real_size, runs });
        }
        pos += len;
    }
    Err(invalid_fs_data("MFT record has no unnamed data stream"))
}

pub fn parse_mft_data_runs_from_record(record: &[u8]) -> io::Result<Vec<(i64, u64)>> {
    Ok(parse_mft_data_attribute(record)?.runs)
}
