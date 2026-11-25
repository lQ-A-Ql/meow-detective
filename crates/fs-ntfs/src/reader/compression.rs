use std::io;

use crate::attribute::{data_extents_initialized_size, DataAttributeExtent};
use crate::{compression::decode_lznt1, invalid_fs_data, DataRun};

const MAX_COMPRESSION_UNIT_BYTES: u64 = 1024 * 1024;

struct CompressionRuns<'a> {
    runs: Vec<(u64, &'a DataRun)>,
    unit_clusters: u64,
    unit_bytes: u64,
    mapped_clusters: u64,
}

impl crate::NtfsReader {
    pub(super) fn read_compressed_range(
        &self,
        extents: &[DataAttributeExtent],
        offset: u64,
        output: &mut [u8],
    ) -> io::Result<()> {
        let layout = compression_runs(extents, self.cluster_size)?;
        let initialized = data_extents_initialized_size(extents, self.cluster_size)?;
        let mapped_bytes = layout
            .mapped_clusters
            .checked_mul(self.cluster_size)
            .ok_or_else(|| invalid_fs_data("compressed mapped size overflow"))?;
        if initialized > mapped_bytes {
            return Err(invalid_fs_data(
                "compressed initialized data exceeds its mapping",
            ));
        }
        let end = offset
            .checked_add(output.len() as u64)
            .ok_or_else(|| invalid_fs_data("compressed range overflow"))?
            .min(initialized);
        let mut position = offset;
        while position < end {
            let unit_start = position / layout.unit_bytes * layout.unit_bytes;
            let unit = self.read_compression_unit(&layout, unit_start / self.cluster_size)?;
            let required = (initialized - unit_start).min(layout.unit_bytes);
            if (unit.len() as u64) < required {
                return Err(invalid_fs_data(
                    "decoded NTFS compression unit is truncated",
                ));
            }
            let take = (end - position).min(layout.unit_bytes - (position - unit_start)) as usize;
            let start = (position - unit_start) as usize;
            let destination = (position - offset) as usize;
            output[destination..destination + take].copy_from_slice(&unit[start..start + take]);
            position += take as u64;
        }
        Ok(())
    }

    fn read_compression_unit(
        &self,
        layout: &CompressionRuns<'_>,
        first_vcn: u64,
    ) -> io::Result<Vec<u8>> {
        let last_vcn = first_vcn
            .checked_add(layout.unit_clusters)
            .ok_or_else(|| invalid_fs_data("compression unit VCN overflow"))?
            .min(layout.mapped_clusters);
        let mut physical = Vec::new();
        let mut covered = first_vcn;
        let mut sparse = false;
        for (vcn, run) in &layout.runs {
            let end = vcn
                .checked_add(run.cluster_count)
                .ok_or_else(|| invalid_fs_data("compressed run VCN overflow"))?;
            let start = first_vcn.max(*vcn);
            let stop = last_vcn.min(end);
            if start >= stop {
                continue;
            }
            if covered != start {
                return Err(invalid_fs_data(
                    "compression unit contains a missing extent",
                ));
            }
            match run.lcn {
                Some(_) if sparse => {
                    return Err(invalid_fs_data(
                        "allocated clusters follow sparse unit padding",
                    ));
                }
                Some(lcn) => {
                    let relative = i64::try_from(start - vcn)
                        .map_err(|_| invalid_fs_data("compressed run displacement exceeds i64"))?;
                    let lcn = lcn
                        .checked_add(relative)
                        .ok_or_else(|| invalid_fs_data("compressed run LCN overflow"))?;
                    self.read_clusters_into(
                        lcn,
                        stop - start,
                        &mut physical,
                        layout.unit_bytes as usize,
                    )?;
                }
                None => sparse = true,
            }
            covered = stop;
        }
        if covered != last_vcn {
            return Err(invalid_fs_data("compression unit allocation is incomplete"));
        }
        if physical.is_empty() {
            return Ok(vec![0; layout.unit_bytes as usize]);
        }
        if sparse {
            return decode_lznt1(&physical, layout.unit_bytes as usize);
        }
        Ok(physical)
    }
}

fn compression_runs(
    extents: &[DataAttributeExtent],
    cluster_size: u64,
) -> io::Result<CompressionRuns<'_>> {
    let mut exponent = None;
    let mut runs = Vec::new();
    let mut mapped_clusters = 0;
    for extent in extents {
        let DataAttributeExtent::NonResident {
            lowest_vcn,
            attr_flags,
            compression_unit_exp,
            runs: extent_runs,
            ..
        } = extent
        else {
            return Err(invalid_fs_data(
                "compressed stream contains a resident extent",
            ));
        };
        if attr_flags & 1 == 0 || exponent.is_some_and(|value| value != *compression_unit_exp) {
            return Err(invalid_fs_data("inconsistent NTFS compression attributes"));
        }
        exponent = Some(*compression_unit_exp);
        let mut vcn = *lowest_vcn;
        for run in extent_runs {
            runs.push((vcn, run));
            vcn = vcn
                .checked_add(run.cluster_count)
                .ok_or_else(|| invalid_fs_data("compressed extent VCN overflow"))?;
        }
        mapped_clusters = vcn;
    }
    let unit_clusters = 1u64
        .checked_shl(u32::from(exponent.unwrap_or(0)))
        .filter(|value| *value > 1)
        .ok_or_else(|| invalid_fs_data("invalid NTFS compression unit exponent"))?;
    let unit_bytes = unit_clusters
        .checked_mul(cluster_size)
        .filter(|value| *value <= MAX_COMPRESSION_UNIT_BYTES && *value > 0)
        .ok_or_else(|| invalid_fs_data("NTFS compression unit exceeds memory budget"))?;
    Ok(CompressionRuns {
        runs,
        unit_clusters,
        unit_bytes,
        mapped_clusters,
    })
}
