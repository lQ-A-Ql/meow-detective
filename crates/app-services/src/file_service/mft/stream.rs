use std::{io::SeekFrom, path::Path};

use evidence_core::EvidenceReader;
use fs_ntfs::ValidatedRecord;
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
    let record = ValidatedRecord::new(&mut record, bytes_per_sector as usize)?;
    Ok(record.mft_data()?.runs)
}

pub fn parse_ntfs_data_runs(data: &[u8]) -> std::io::Result<Vec<(i64, u64)>> {
    fs_ntfs::parse_data_runs_bytes(data)
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
        let run_end = run_stream_start
            .checked_add(run_bytes)
            .ok_or_else(|| invalid_data("MFT stream range overflow"))?;
        if stream_offset >= run_end {
            run_stream_start = run_end;
            continue;
        }
        let offset_in_run = stream_offset.saturating_sub(run_stream_start);
        let available = run_bytes.saturating_sub(offset_in_run);
        let to_read = available.min((out.len() - written) as u64) as usize;
        let disk_offset = (*lcn as u64)
            .checked_mul(cluster_size)
            .and_then(|offset| volume_offset.checked_add(offset))
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
