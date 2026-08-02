use std::io;

use crate::compression::append_compressed_unit;
use crate::{invalid_fs_data, DataRun, MAX_BUFFERED_FILE_BYTES};

impl crate::NtfsReader {
    pub(super) fn read_compressed_data_runs_to_vec(
        &self,
        runs: &[DataRun],
        compression_unit_exp: u16,
        real_size: u64,
    ) -> io::Result<Vec<u8>> {
        let unit_clusters = 1u64
            .checked_shl(compression_unit_exp.min(20) as u32)
            .filter(|value| *value > 0)
            .unwrap_or(16);
        let unit_bytes = unit_clusters
            .checked_mul(self.cluster_size)
            .ok_or_else(|| invalid_fs_data("compressed unit size overflow"))?;
        let mut output = Vec::new();
        let mut unit = Vec::new();
        let mut unit_logical_clusters = 0u64;
        let mut unit_has_sparse = false;

        for run in runs {
            let mut consumed = 0u64;
            while consumed < run.cluster_count && output.len() as u64 <= real_size {
                let unit_remaining = unit_clusters.saturating_sub(unit_logical_clusters);
                let take = (run.cluster_count - consumed).min(unit_remaining);
                if take == 0 {
                    break;
                }

                if let Some(lcn) = run.lcn {
                    let physical_lcn = lcn
                        .checked_add(consumed as i64)
                        .ok_or_else(|| invalid_fs_data("compressed data run LCN overflow"))?;
                    self.read_clusters_into(
                        physical_lcn,
                        take,
                        &mut unit,
                        MAX_BUFFERED_FILE_BYTES,
                    )?;
                } else {
                    unit_has_sparse = true;
                }

                unit_logical_clusters += take;
                consumed += take;
                if unit_logical_clusters == unit_clusters {
                    append_compressed_unit(
                        &mut output,
                        &unit,
                        unit_has_sparse,
                        unit_bytes,
                        MAX_BUFFERED_FILE_BYTES,
                    )?;
                    unit.clear();
                    unit_logical_clusters = 0;
                    unit_has_sparse = false;
                }
            }
        }

        if unit_logical_clusters > 0 && output.len() as u64 <= real_size {
            let logical_bytes = unit_logical_clusters
                .checked_mul(self.cluster_size)
                .ok_or_else(|| invalid_fs_data("compressed partial unit size overflow"))?;
            append_compressed_unit(
                &mut output,
                &unit,
                unit_has_sparse,
                logical_bytes,
                MAX_BUFFERED_FILE_BYTES,
            )?;
        }

        Ok(output)
    }
}
