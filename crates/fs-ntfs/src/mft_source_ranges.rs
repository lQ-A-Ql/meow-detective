//! Exact physical source spans for a record that may cross fragmented $MFT runs.

use crate::{invalid_fs_data, unexpected_fs_eof, NtfsReader, NtfsRecordSourceRange};
use std::io;

impl NtfsReader {
    pub(crate) fn mft_record_source_ranges(
        &self,
        inode: u64,
    ) -> io::Result<Vec<NtfsRecordSourceRange>> {
        if self.mft_data_runs.is_empty() {
            return Ok(vec![NtfsRecordSourceRange {
                record_offset: 0,
                source_offset: self.mft_offset(inode)?,
                length: self.mft_record_size,
            }]);
        }
        let start = inode
            .checked_mul(u64::from(self.mft_record_size))
            .ok_or_else(|| invalid_fs_data("MFT record range overflow"))?;
        let end = start
            .checked_add(u64::from(self.mft_record_size))
            .ok_or_else(|| invalid_fs_data("MFT record range overflow"))?;
        let mut stream_offset = 0u64;
        let mut output = Vec::new();
        let mut mapped = 0u64;
        for (lcn, clusters) in &self.mft_data_runs {
            let lcn = u64::try_from(*lcn).map_err(|_| invalid_fs_data("negative MFT LCN"))?;
            let run_size = clusters
                .checked_mul(self.cluster_size)
                .ok_or_else(|| invalid_fs_data("MFT run size overflow"))?;
            let run_end = stream_offset
                .checked_add(run_size)
                .ok_or_else(|| invalid_fs_data("MFT stream size overflow"))?;
            let overlap_start = start.max(stream_offset);
            let overlap_end = end.min(run_end);
            if overlap_end > overlap_start {
                let source_offset = lcn
                    .checked_mul(self.cluster_size)
                    .and_then(|offset| self.volume_offset.checked_add(offset))
                    .and_then(|offset| offset.checked_add(overlap_start - stream_offset))
                    .ok_or_else(|| invalid_fs_data("MFT source offset overflow"))?;
                let length = overlap_end - overlap_start;
                mapped += length;
                output.push(NtfsRecordSourceRange {
                    record_offset: (overlap_start - start) as u32,
                    source_offset,
                    length: length as u32,
                });
                if overlap_end == end {
                    break;
                }
            }
            stream_offset = run_end;
        }
        if mapped != u64::from(self.mft_record_size) {
            return Err(unexpected_fs_eof(
                "MFT source spans do not cover the record",
            ));
        }
        Ok(output)
    }
}
