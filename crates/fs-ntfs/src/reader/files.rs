use std::io;

use crate::attribute::{
    data_extent_logical_len, data_extent_logical_start, data_extents_declared_size,
    data_extents_logical_size, DataAttributeExtent,
};
use crate::data_runs::data_runs_logical_size;
use crate::utils::validate_file_record;
use crate::{
    fs_out_of_memory, invalid_fs_data, truncate_data_to_declared_size, MAX_BUFFERED_FILE_BYTES,
};

impl crate::NtfsReader {
    /// Read the unnamed `$DATA` attribute of a file by MFT inode.
    pub(crate) fn read_file_data(&self, inode: u64) -> io::Result<Vec<u8>> {
        let extents = self.collect_unnamed_data_extents(inode)?;
        if extents.is_empty() {
            return Ok(Vec::new());
        }
        self.read_data_extents_to_vec(&extents)
    }

    pub(crate) fn read_file_data_range(
        &self,
        inode: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        if length == 0 {
            return Ok(Vec::new());
        }
        let record = self.read_mft_record(inode)?;
        validate_file_record(&record, inode)?;
        let extents = self.collect_unnamed_data_extents_from_base(inode, record)?;
        if extents.is_empty() {
            return Ok(Vec::new());
        }
        self.read_data_extents_range(&extents, offset, length)
    }

    fn read_data_extents_to_vec(&self, extents: &[DataAttributeExtent]) -> io::Result<Vec<u8>> {
        let data_length = data_extents_logical_size(extents, self.cluster_size)?;
        if data_length as usize > MAX_BUFFERED_FILE_BYTES {
            return Err(invalid_fs_data(format!(
                "data run buffer exceeds {} byte limit (would be {} bytes)",
                MAX_BUFFERED_FILE_BYTES, data_length
            )));
        }

        let mut output = vec![0u8; data_length as usize];
        for extent in extents {
            let extent_start = data_extent_logical_start(extent, self.cluster_size)?;
            let extent_bytes = self.read_data_extent_to_vec(extent)?;
            let start = usize::try_from(extent_start)
                .map_err(|_| invalid_fs_data("data extent offset too large"))?;
            if start >= output.len() {
                continue;
            }
            let end = start.saturating_add(extent_bytes.len()).min(output.len());
            output[start..end].copy_from_slice(&extent_bytes[..end - start]);
        }

        Ok(truncate_data_to_declared_size(
            output,
            data_extents_declared_size(extents, self.cluster_size)?,
        ))
    }

    fn read_data_extent_to_vec(&self, extent: &DataAttributeExtent) -> io::Result<Vec<u8>> {
        match extent {
            DataAttributeExtent::Resident { data } => Ok(data.clone()),
            DataAttributeExtent::NonResident {
                allocated_size,
                real_size,
                attr_flags,
                compression_unit_exp,
                runs,
                ..
            } => {
                if *attr_flags & 0x0001 != 0 {
                    let decoded = self.read_compressed_data_runs_to_vec(
                        runs,
                        *compression_unit_exp,
                        *real_size,
                    )?;
                    return Ok(truncate_data_to_declared_size(decoded, *real_size));
                }

                let allocated = data_runs_logical_size(runs, self.cluster_size)?;
                let allocated = if *allocated_size > 0 {
                    (*allocated_size).min(allocated)
                } else {
                    allocated
                };
                self.read_data_runs_to_vec(runs, true, allocated)
            }
        }
    }

    pub(crate) fn read_data_extents_range(
        &self,
        extents: &[DataAttributeExtent],
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        let logical_size = data_extents_declared_size(extents, self.cluster_size)?;
        if offset >= logical_size {
            return Ok(Vec::new());
        }

        let length_u64 = u64::try_from(length)
            .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
        let bounded_length = length_u64.min(logical_size.saturating_sub(offset));
        let bounded_length = usize::try_from(bounded_length)
            .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
        let mut output = vec![0u8; bounded_length];
        let request_end = offset
            .checked_add(bounded_length as u64)
            .ok_or_else(|| invalid_fs_data("requested range offset overflow"))?;

        for extent in extents {
            let extent_start = data_extent_logical_start(extent, self.cluster_size)?;
            let extent_length = data_extent_logical_len(extent, self.cluster_size)?;
            let extent_end = extent_start
                .checked_add(extent_length)
                .ok_or_else(|| invalid_fs_data("data extent logical offset overflow"))?;
            if extent_end <= offset || extent_start >= request_end {
                continue;
            }

            let overlap_start = offset.max(extent_start);
            let overlap_end = request_end.min(extent_end);
            let output_start = usize::try_from(overlap_start - offset)
                .map_err(|_| invalid_fs_data("range output offset overflow"))?;
            let output_length = usize::try_from(overlap_end - overlap_start)
                .map_err(|_| invalid_fs_data("range output length overflow"))?;
            let bytes = self.read_data_extent_range(
                extent,
                overlap_start.saturating_sub(extent_start),
                output_length,
            )?;
            let copy_length = bytes.len().min(output_length);
            output[output_start..output_start + copy_length].copy_from_slice(&bytes[..copy_length]);
        }

        Ok(output)
    }

    fn read_data_extent_range(
        &self,
        extent: &DataAttributeExtent,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        match extent {
            DataAttributeExtent::Resident { data } => {
                let Ok(start) = usize::try_from(offset) else {
                    return Ok(Vec::new());
                };
                if start >= data.len() {
                    return Ok(Vec::new());
                }
                let end = start.saturating_add(length).min(data.len());
                Ok(data[start..end].to_vec())
            }
            DataAttributeExtent::NonResident {
                attr_flags, runs, ..
            } => {
                let extent_length = data_extent_logical_len(extent, self.cluster_size)?;
                if offset >= extent_length {
                    return Ok(Vec::new());
                }
                let length_u64 = u64::try_from(length)
                    .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
                let bounded_length = length_u64.min(extent_length.saturating_sub(offset));
                let bounded_length = usize::try_from(bounded_length)
                    .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
                if *attr_flags & 0x0001 != 0 {
                    return Err(invalid_fs_data(
                        "range reads for compressed NTFS data are not supported",
                    ));
                }
                self.read_data_runs_range(runs, offset, bounded_length)
            }
        }
    }
}
