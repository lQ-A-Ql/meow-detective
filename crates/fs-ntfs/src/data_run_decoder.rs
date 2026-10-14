//! Shared runlist decoding; normal reads borrow encoded bytes without copying them.

use crate::data_runs::{read_sized_le, read_sized_le_signed};
use crate::invalid_fs_data;
use std::io;

const MAX_DATA_RUNS: usize = 100_000;

pub(crate) struct EncodedDataRun<'a> {
    pub header: u8,
    pub cluster_count: u64,
    pub relative_lcn: Option<i64>,
    pub absolute_lcn: Option<i64>,
    pub raw: &'a [u8],
}

pub(crate) fn decode_data_runs(
    mut bytes: &[u8],
    mut visit: impl FnMut(EncodedDataRun<'_>) -> io::Result<()>,
) -> io::Result<()> {
    let mut previous_lcn = 0i64;
    let mut clusters = 0u64;
    for index in 0..=MAX_DATA_RUNS {
        let Some(&header) = bytes.first() else {
            return if index == 0 {
                Ok(())
            } else {
                Err(invalid_fs_data("unterminated NTFS data run list"))
            };
        };
        if header == 0 {
            return Ok(());
        }
        if index == MAX_DATA_RUNS {
            return Err(invalid_fs_data("too many NTFS data runs"));
        }
        let length_size = usize::from(header & 0x0f);
        let offset_size = usize::from(header >> 4);
        if !(1..=8).contains(&length_size) || offset_size > 8 {
            return Err(invalid_fs_data("invalid NTFS data run header"));
        }
        let encoded_size = 1 + length_size + offset_size;
        let raw = bytes
            .get(..encoded_size)
            .ok_or_else(|| invalid_fs_data("truncated NTFS data run"))?;
        let cluster_count = read_sized_le(&raw[1..1 + length_size]);
        if cluster_count == 0 {
            return Err(invalid_fs_data("zero-length NTFS data run"));
        }
        clusters = clusters
            .checked_add(cluster_count)
            .ok_or_else(|| invalid_fs_data("NTFS data run cluster count overflow"))?;
        let relative_lcn =
            (offset_size != 0).then(|| read_sized_le_signed(&raw[1 + length_size..]));
        let absolute_lcn = relative_lcn
            .map(|delta| {
                previous_lcn
                    .checked_add(delta)
                    .filter(|lcn| *lcn >= 0)
                    .ok_or_else(|| invalid_fs_data("invalid NTFS data run LCN"))
            })
            .transpose()?;
        visit(EncodedDataRun {
            header,
            cluster_count,
            relative_lcn,
            absolute_lcn,
            raw,
        })?;
        if let Some(lcn) = absolute_lcn {
            previous_lcn = lcn;
        }
        bytes = &bytes[encoded_size..];
    }
    Err(invalid_fs_data("too many NTFS data runs"))
}
