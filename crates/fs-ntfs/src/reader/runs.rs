use std::io::{self, Read, Seek, SeekFrom};

use crate::{invalid_fs_data, DataRun, MAX_BUFFERED_FILE_BYTES};

impl crate::NtfsReader {
    pub(super) fn read_data_runs_to_vec(
        &self,
        runs: &[DataRun],
        include_sparse: bool,
        max_bytes: u64,
    ) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        let mut reader = self.reader.borrow_mut();

        for run in runs {
            let chunk = run
                .cluster_count
                .checked_mul(self.cluster_size)
                .ok_or_else(|| {
                    invalid_fs_data(format!(
                        "data run overflow: {} clusters × {} bytes/cluster",
                        run.cluster_count, self.cluster_size
                    ))
                })?;
            if max_bytes > 0 && data.len() as u64 >= max_bytes {
                break;
            }
            let to_append = if max_bytes > 0 {
                chunk.min(max_bytes.saturating_sub(data.len() as u64))
            } else {
                chunk
            } as usize;
            if to_append == 0 {
                continue;
            }
            let new_size = data
                .len()
                .checked_add(to_append)
                .ok_or_else(|| invalid_fs_data("data run buffer size overflow"))?;
            if new_size > MAX_BUFFERED_FILE_BYTES {
                return Err(invalid_fs_data(format!(
                    "data run buffer exceeds {} byte limit (would be {} bytes)",
                    MAX_BUFFERED_FILE_BYTES, new_size
                )));
            }

            match run.lcn {
                Some(lcn) => {
                    let offset = self.cluster_to_offset(lcn)?;
                    let start = data.len();
                    data.resize(new_size, 0);
                    reader.seek(SeekFrom::Start(offset))?;
                    reader.read_exact(&mut data[start..])?;
                }
                None if include_sparse => data.resize(new_size, 0),
                None => {}
            }
        }

        Ok(data)
    }

    pub(super) fn read_data_runs_range(
        &self,
        runs: &[DataRun],
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        let mut output = vec![0u8; length];
        if length == 0 {
            return Ok(output);
        }

        let request_end = offset
            .checked_add(length as u64)
            .ok_or_else(|| invalid_fs_data("requested range offset overflow"))?;
        let mut logical_start = 0u64;
        let mut reader = self.reader.borrow_mut();

        for run in runs {
            let run_bytes = run
                .cluster_count
                .checked_mul(self.cluster_size)
                .ok_or_else(|| {
                    invalid_fs_data(format!(
                        "data run overflow: {} clusters × {} bytes/cluster",
                        run.cluster_count, self.cluster_size
                    ))
                })?;
            let run_end = logical_start
                .checked_add(run_bytes)
                .ok_or_else(|| invalid_fs_data("data run logical offset overflow"))?;
            if run_end <= offset {
                logical_start = run_end;
                continue;
            }
            if logical_start >= request_end {
                break;
            }

            let overlap_start = offset.max(logical_start);
            let overlap_end = request_end.min(run_end);
            if overlap_start < overlap_end {
                let output_start = usize::try_from(overlap_start - offset)
                    .map_err(|_| invalid_fs_data("range output offset overflow"))?;
                let output_length = usize::try_from(overlap_end - overlap_start)
                    .map_err(|_| invalid_fs_data("range output length overflow"))?;
                if let Some(lcn) = run.lcn {
                    let run_relative = overlap_start - logical_start;
                    let disk_offset = self
                        .cluster_to_offset(lcn)?
                        .checked_add(run_relative)
                        .ok_or_else(|| invalid_fs_data("data run disk offset overflow"))?;
                    reader.seek(SeekFrom::Start(disk_offset))?;
                    reader.read_exact(&mut output[output_start..output_start + output_length])?;
                }
            }
            logical_start = run_end;
        }

        Ok(output)
    }

    pub(super) fn read_clusters_into(
        &self,
        lcn: i64,
        cluster_count: u64,
        output: &mut Vec<u8>,
        max_bytes: usize,
    ) -> io::Result<()> {
        let bytes = cluster_count
            .checked_mul(self.cluster_size)
            .ok_or_else(|| {
                invalid_fs_data(format!(
                    "data run overflow: {} clusters × {} bytes/cluster",
                    cluster_count, self.cluster_size
                ))
            })? as usize;
        let new_size = output
            .len()
            .checked_add(bytes)
            .ok_or_else(|| invalid_fs_data("data run buffer size overflow"))?;
        if new_size > max_bytes {
            return Err(invalid_fs_data(format!(
                "data run buffer exceeds {} byte limit (would be {} bytes)",
                max_bytes, new_size
            )));
        }

        let offset = self.cluster_to_offset(lcn)?;
        let start = output.len();
        output.resize(new_size, 0);
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut output[start..])?;
        Ok(())
    }
}
