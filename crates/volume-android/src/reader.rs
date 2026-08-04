use std::io::{self, Read, Seek, SeekFrom};

use evidence_core::{EvidenceReader, ReaderInfo};

use crate::error::{Result, VolumeAndroidError};
use crate::metadata::{LogicalExtentTarget, LogicalPartition};

pub struct LogicalPartitionReader {
    source: Box<dyn EvidenceReader>,
    partition: LogicalPartition,
    cursor: u64,
    info: ReaderInfo,
}

impl LogicalPartitionReader {
    pub fn new(source: Box<dyn EvidenceReader>, partition: LogicalPartition) -> Result<Self> {
        if partition.disabled {
            return Err(VolumeAndroidError::DisabledPartition {
                partition: partition.name,
            });
        }
        if let Some(source_index) =
            partition
                .extents
                .iter()
                .find_map(|extent| match extent.target {
                    LogicalExtentTarget::Linear { source_index, .. } if source_index != 0 => {
                        Some(source_index)
                    }
                    _ => None,
                })
        {
            return Err(VolumeAndroidError::UnsupportedBlockDevice {
                partition: partition.name,
                source_index,
            });
        }
        validate_source_bounds(source.as_ref(), &partition)?;
        let info = ReaderInfo {
            path: source.info().path.clone(),
            size: partition.size,
            kind: format!("android-logical-partition:{}", partition.name),
        };
        Ok(Self {
            source,
            partition,
            cursor: 0,
            info,
        })
    }

    pub fn partition(&self) -> &LogicalPartition {
        &self.partition
    }

    pub fn read_range(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        if offset >= self.partition.size || buffer.is_empty() {
            return Ok(0);
        }
        let remaining = usize::try_from(self.partition.size - offset).unwrap_or(usize::MAX);
        let requested = buffer.len().min(remaining);
        let mut total = 0usize;
        let mut position = offset;
        while total < requested {
            let extent = self.extent_for(position)?;
            let intra = position - extent.logical_offset;
            let available = usize::try_from(extent.length - intra).unwrap_or(usize::MAX);
            let length = available.min(requested - total);
            match extent.target {
                LogicalExtentTarget::Linear { source_offset, .. } => {
                    let physical_offset = source_offset.checked_add(intra).ok_or(
                        VolumeAndroidError::ArithmeticOverflow("logical partition read offset"),
                    )?;
                    self.source.seek(SeekFrom::Start(physical_offset))?;
                    self.source.read_exact(&mut buffer[total..total + length])?;
                }
                LogicalExtentTarget::Zero => buffer[total..total + length].fill(0),
            }
            total += length;
            position += length as u64;
        }
        Ok(total)
    }

    fn extent_for(&self, offset: u64) -> Result<&crate::LogicalExtent> {
        let index = self
            .partition
            .extents
            .partition_point(|extent| extent.logical_offset <= offset);
        let extent = index
            .checked_sub(1)
            .and_then(|index| self.partition.extents.get(index))
            .ok_or(VolumeAndroidError::MissingExtent(offset))?;
        let contains = extent
            .logical_offset
            .checked_add(extent.length)
            .is_some_and(|end| offset < end);
        contains
            .then_some(extent)
            .ok_or(VolumeAndroidError::MissingExtent(offset))
    }
}

fn validate_source_bounds(source: &dyn EvidenceReader, partition: &LogicalPartition) -> Result<()> {
    for extent in &partition.extents {
        if let LogicalExtentTarget::Linear { source_offset, .. } = extent.target {
            let end = source_offset.checked_add(extent.length).ok_or(
                VolumeAndroidError::ArithmeticOverflow("logical reader source bound"),
            )?;
            if end > source.info().size {
                return Err(VolumeAndroidError::InvalidMetadata(format!(
                    "partition `{}` exceeds the supplied source reader",
                    partition.name
                )));
            }
        }
    }
    Ok(())
}

impl Read for LogicalPartitionReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self
            .read_range(self.cursor, buffer)
            .map_err(io::Error::other)?;
        self.cursor = self.cursor.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for LogicalPartitionReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.partition.size) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.cursor) + i128::from(value),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "logical partition seek is outside the addressable range",
            ));
        }
        self.cursor = next as u64;
        Ok(self.cursor)
    }
}

impl EvidenceReader for LogicalPartitionReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
