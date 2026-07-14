use std::io::SeekFrom;

use ceph_wire::{BluefsExtent, BluefsFnode};
use transport::CommandError;

pub(super) const BLUEFS_MAX_CONTROL_FILE_BYTES: u64 = 64 * 1024 * 1024;

pub(super) struct PreparedBluefsFile {
    size: u64,
    extents: Vec<PreparedExtent>,
}

#[derive(Debug, Clone, Copy)]
struct PreparedExtent {
    logical_start: u64,
    logical_end: u64,
    physical_start: u64,
}

pub(super) struct BluefsExtentReader<'a> {
    reader: &'a mut dyn evidence_core::EvidenceReader,
    shared_device_id: u8,
    device_size: u64,
    reserved_bytes: u64,
}

impl<'a> BluefsExtentReader<'a> {
    pub(super) fn new(
        reader: &'a mut dyn evidence_core::EvidenceReader,
        shared_device_id: u8,
        device_size: u64,
        reserved_bytes: u64,
    ) -> Self {
        Self {
            reader,
            shared_device_id,
            device_size,
            reserved_bytes,
        }
    }

    pub(super) fn read_plain_file(&mut self, fnode: &BluefsFnode) -> Result<Vec<u8>, CommandError> {
        if fnode.encoding != 0 {
            return Err(CommandError::unsupported(format!(
                "BlueFS content encoding {} is not supported",
                fnode.encoding
            )));
        }
        if fnode.size > BLUEFS_MAX_CONTROL_FILE_BYTES {
            return Err(file_error(format!(
                "BlueFS control file size {} exceeds the {} byte limit",
                fnode.size, BLUEFS_MAX_CONTROL_FILE_BYTES
            )));
        }
        let allocated = allocated_bytes(&fnode.extents)?;
        if fnode.size > allocated {
            return Err(file_error(format!(
                "BlueFS file size {} exceeds allocated extent bytes {}",
                fnode.size, allocated
            )));
        }
        self.read_range_with_limit(fnode, 0, fnode.size, fnode.size)?
            .ok_or_else(|| file_error("BlueFS file has no readable content"))
    }

    pub(super) fn read_allocated_range(
        &mut self,
        fnode: &BluefsFnode,
        logical_offset: u64,
        length: u64,
    ) -> Result<Option<Vec<u8>>, CommandError> {
        let allocated = allocated_bytes(&fnode.extents)?;
        self.read_range_with_limit(fnode, logical_offset, length, allocated)
    }

    pub(super) fn prepare_file(
        &self,
        fnode: &BluefsFnode,
    ) -> Result<PreparedBluefsFile, CommandError> {
        if fnode.encoding != 0 {
            return Err(CommandError::unsupported(format!(
                "BlueFS content encoding {} is not supported",
                fnode.encoding
            )));
        }
        self.validate_extents(&fnode.extents)?;
        let allocated = allocated_bytes(&fnode.extents)?;
        if fnode.size > allocated {
            return Err(file_error(format!(
                "BlueFS file size {} exceeds allocated extent bytes {}",
                fnode.size, allocated
            )));
        }
        validate_no_physical_overlap(&fnode.extents)?;
        let mut logical_start = 0u64;
        let mut extents = Vec::with_capacity(fnode.extents.len());
        for extent in &fnode.extents {
            let logical_end = logical_start
                .checked_add(u64::from(extent.length))
                .ok_or_else(|| file_error("BlueFS extent logical end overflow"))?;
            extents.push(PreparedExtent {
                logical_start,
                logical_end,
                physical_start: extent.offset,
            });
            logical_start = logical_end;
        }
        Ok(PreparedBluefsFile {
            size: fnode.size,
            extents,
        })
    }

    pub(super) fn read_prepared_file_range(
        &mut self,
        file: &PreparedBluefsFile,
        logical_offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, CommandError> {
        let logical_end = logical_offset
            .checked_add(length)
            .ok_or_else(|| file_error("BlueFS file read end overflow"))?;
        if logical_end > file.size {
            return Err(file_error(format!(
                "BlueFS file range {logical_offset}..{logical_end} exceeds logical size {}",
                file.size
            )));
        }
        let output_length =
            usize::try_from(length).map_err(|_| file_error("BlueFS read length exceeds usize"))?;
        let mut output = Vec::with_capacity(output_length);
        let start = file
            .extents
            .partition_point(|extent| extent.logical_end <= logical_offset);
        for extent in &file.extents[start..] {
            if extent.logical_start >= logical_end {
                break;
            }
            self.read_prepared_extent(extent, logical_offset, logical_end, &mut output)?;
        }
        if output.len() != output_length {
            return Err(file_error("BlueFS prepared file range is truncated"));
        }
        Ok(output)
    }

    fn read_range_with_limit(
        &mut self,
        fnode: &BluefsFnode,
        logical_offset: u64,
        length: u64,
        readable_length: u64,
    ) -> Result<Option<Vec<u8>>, CommandError> {
        self.validate_extents(&fnode.extents)?;
        let logical_end = logical_offset
            .checked_add(length)
            .ok_or_else(|| file_error("BlueFS logical read end overflow"))?;
        if logical_offset >= readable_length {
            return Ok(None);
        }
        let requested_end = logical_end.min(readable_length);
        let output_length = usize::try_from(requested_end - logical_offset)
            .map_err(|_| file_error("BlueFS read length exceeds usize"))?;
        let mut output = Vec::with_capacity(output_length);
        let mut logical_base = 0u64;
        for extent in &fnode.extents {
            let extent_end = logical_base
                .checked_add(u64::from(extent.length))
                .ok_or_else(|| file_error("BlueFS extent logical end overflow"))?;
            self.read_extent_overlap(
                extent,
                logical_base,
                logical_offset,
                requested_end,
                &mut output,
            )?;
            logical_base = extent_end;
            if logical_base >= requested_end {
                break;
            }
        }
        Ok((output.len() == output_length).then_some(output))
    }

    fn validate_extents(&self, extents: &[BluefsExtent]) -> Result<(), CommandError> {
        for extent in extents {
            if extent.bdev != self.shared_device_id {
                return Err(file_error(format!(
                    "BlueFS extent references device {} instead of shared device {}",
                    extent.bdev, self.shared_device_id
                )));
            }
            if extent.length == 0 {
                return Err(file_error("BlueFS extent length is zero"));
            }
            let end = extent
                .offset
                .checked_add(u64::from(extent.length))
                .ok_or_else(|| file_error("BlueFS physical extent end overflow"))?;
            if extent.offset < self.reserved_bytes || end > self.device_size {
                return Err(file_error(format!(
                    "BlueFS physical extent {}..{} is outside the readable device range {}..{}",
                    extent.offset, end, self.reserved_bytes, self.device_size
                )));
            }
        }
        Ok(())
    }

    fn read_extent_overlap(
        &mut self,
        extent: &BluefsExtent,
        logical_base: u64,
        logical_offset: u64,
        requested_end: u64,
        output: &mut Vec<u8>,
    ) -> Result<(), CommandError> {
        let extent_end = logical_base
            .checked_add(u64::from(extent.length))
            .ok_or_else(|| file_error("BlueFS extent logical end overflow"))?;
        let overlap_start = logical_offset.max(logical_base);
        let overlap_end = requested_end.min(extent_end);
        if overlap_start >= overlap_end {
            return Ok(());
        }
        let physical_offset = extent
            .offset
            .checked_add(overlap_start - logical_base)
            .ok_or_else(|| file_error("BlueFS physical read offset overflow"))?;
        let read_length = usize::try_from(overlap_end - overlap_start)
            .map_err(|_| file_error("BlueFS extent read length exceeds usize"))?;
        self.reader
            .seek(SeekFrom::Start(physical_offset))
            .map_err(CommandError::from_service_error)?;
        let start = output.len();
        output.resize(start + read_length, 0);
        self.reader
            .read_exact(&mut output[start..])
            .map_err(CommandError::from_service_error)
    }

    fn read_prepared_extent(
        &mut self,
        extent: &PreparedExtent,
        logical_offset: u64,
        logical_end: u64,
        output: &mut Vec<u8>,
    ) -> Result<(), CommandError> {
        let overlap_start = logical_offset.max(extent.logical_start);
        let overlap_end = logical_end.min(extent.logical_end);
        if overlap_start >= overlap_end {
            return Ok(());
        }
        let physical_offset = extent
            .physical_start
            .checked_add(overlap_start - extent.logical_start)
            .ok_or_else(|| file_error("BlueFS physical read offset overflow"))?;
        let read_length = usize::try_from(overlap_end - overlap_start)
            .map_err(|_| file_error("BlueFS extent read length exceeds usize"))?;
        self.reader
            .seek(SeekFrom::Start(physical_offset))
            .map_err(CommandError::from_service_error)?;
        let start = output.len();
        output.resize(start + read_length, 0);
        self.reader
            .read_exact(&mut output[start..])
            .map_err(CommandError::from_service_error)
    }
}

pub(super) fn allocated_bytes(extents: &[BluefsExtent]) -> Result<u64, CommandError> {
    extents.iter().try_fold(0u64, |total, extent| {
        total
            .checked_add(u64::from(extent.length))
            .ok_or_else(|| file_error("BlueFS allocated length overflow"))
    })
}

fn validate_no_physical_overlap(extents: &[BluefsExtent]) -> Result<(), CommandError> {
    let mut ranges = extents
        .iter()
        .map(|extent| {
            let end = extent
                .offset
                .checked_add(u64::from(extent.length))
                .ok_or_else(|| file_error("BlueFS physical extent end overflow"))?;
            Ok((extent.offset, end))
        })
        .collect::<Result<Vec<_>, CommandError>>()?;
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(file_error("BlueFS file extents overlap physically"));
    }
    Ok(())
}

fn file_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("BlueFS file read failed: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_bluefs_file_reader.rs"]
mod tests;
