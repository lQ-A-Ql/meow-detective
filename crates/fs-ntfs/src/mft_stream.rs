use crate::mft::NtfsReader;
use crate::utils::apply_record_fixup;
use crate::{invalid_fs_data as core_invalid_fs_data, unexpected_fs_eof};
use std::io::{self, Read, Seek, SeekFrom};

impl NtfsReader {
    pub(crate) fn mft_offset(&self, record_number: u64) -> io::Result<u64> {
        self.mft_cluster
            .checked_mul(self.cluster_size)
            .and_then(|offset| self.volume_offset.checked_add(offset))
            .and_then(|offset| {
                record_number
                    .checked_mul(u64::from(self.mft_record_size))
                    .and_then(|record| offset.checked_add(record))
            })
            .ok_or_else(|| core_invalid_fs_data("MFT contiguous record offset overflow"))
    }

    pub(crate) fn read_mft_record(&self, record_number: u64) -> io::Result<Vec<u8>> {
        let mut rec = self.read_mft_record_raw(record_number)?;
        apply_record_fixup(&mut rec, self.bytes_per_sector as usize)?;
        Ok(rec)
    }

    pub(crate) fn read_mft_record_raw(&self, record_number: u64) -> io::Result<Vec<u8>> {
        let mut rec = vec![0u8; self.mft_record_size as usize];
        if self.mft_data_runs.is_empty() {
            let off = self.mft_offset(record_number)?;
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(off))?;
            reader.read_exact(&mut rec)?;
        } else {
            let mft_stream_offset = record_number
                .checked_mul(self.mft_record_size as u64)
                .ok_or_else(|| core_invalid_fs_data("MFT record offset overflow"))?;
            self.read_mft_stream_at(mft_stream_offset, &mut rec)?;
        }
        Ok(rec)
    }

    pub(crate) fn mft_record_source_offset(&self, record_number: u64) -> io::Result<u64> {
        self.mft_record_source_ranges(record_number)?
            .first()
            .map(|range| range.source_offset)
            .ok_or_else(|| unexpected_fs_eof("MFT record has no physical source range"))
    }

    pub(crate) fn read_mft_stream_at(
        &self,
        mut stream_offset: u64,
        out: &mut [u8],
    ) -> io::Result<()> {
        if self.mft_data_runs.is_empty() {
            let offset = self
                .mft_offset(stream_offset / u64::from(self.mft_record_size))?
                .checked_add(stream_offset % u64::from(self.mft_record_size))
                .ok_or_else(|| core_invalid_fs_data("MFT contiguous offset overflow"))?;
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(offset))?;
            return reader.read_exact(out);
        }
        let mut written = 0usize;
        let mut run_stream_start = 0u64;
        let mut reader = self.reader.borrow_mut();

        for (lcn, cluster_count) in &self.mft_data_runs {
            if *lcn < 0 {
                return Err(core_invalid_fs_data(format!("negative MFT LCN {}", lcn)));
            }
            let run_bytes = cluster_count
                .checked_mul(self.cluster_size)
                .ok_or_else(|| {
                    core_invalid_fs_data(format!(
                        "MFT run overflow: {} clusters × {} bytes/cluster",
                        cluster_count, self.cluster_size
                    ))
                })?;
            let run_end = run_stream_start
                .checked_add(run_bytes)
                .ok_or_else(|| core_invalid_fs_data("MFT stream range overflow"))?;
            if stream_offset >= run_end {
                run_stream_start = run_end;
                continue;
            }

            let offset_in_run = stream_offset.saturating_sub(run_stream_start);
            let available = run_bytes.saturating_sub(offset_in_run);
            let need = out.len() - written;
            let to_read = available.min(need as u64) as usize;
            let disk_offset = self
                .volume_offset
                .checked_add(
                    (*lcn as u64)
                        .checked_mul(self.cluster_size)
                        .ok_or_else(|| core_invalid_fs_data("MFT physical range overflow"))?,
                )
                .and_then(|base| base.checked_add(offset_in_run))
                .ok_or_else(|| core_invalid_fs_data("MFT run disk offset overflow"))?;

            reader.seek(SeekFrom::Start(disk_offset))?;
            reader.read_exact(&mut out[written..written + to_read])?;
            written += to_read;
            if written == out.len() {
                return Ok(());
            }
            stream_offset = run_end;
            run_stream_start = run_end;
        }

        Err(unexpected_fs_eof(format!(
            "MFT stream ended before record read completed (read {} of {} bytes)",
            written,
            out.len()
        )))
    }
}
