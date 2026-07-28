//! Bounded streaming access to one NTFS file.

use std::io::{self, Read, Seek, SeekFrom};

use crate::attribute::{data_extents_declared_size, DataAttributeExtent};
use crate::{invalid_fs_data, NtfsReader};

/// Seekable file reader backed by bounded NTFS data-run range reads.
pub struct NtfsFileReader {
    filesystem: NtfsReader,
    extents: Vec<DataAttributeExtent>,
    position: u64,
    size: u64,
}

impl NtfsReader {
    /// Whether an inode can use bounded range reads without whole-file decoding.
    pub fn supports_file_stream_by_inode(&self, inode: u64) -> io::Result<bool> {
        Ok(self
            .collect_unnamed_data_extents(inode)?
            .iter()
            .all(|extent| match extent {
                DataAttributeExtent::Resident { .. } => true,
                DataAttributeExtent::NonResident { attr_flags, .. } => attr_flags & 0x0001 == 0,
            }))
    }

    /// Consume the filesystem reader and open one inode as a bounded stream.
    pub fn into_file_stream_by_inode(self, inode: u64) -> io::Result<NtfsFileReader> {
        let extents = self.collect_unnamed_data_extents(inode)?;
        if extents.iter().any(|extent| {
            matches!(
                extent,
                DataAttributeExtent::NonResident { attr_flags, .. } if attr_flags & 0x0001 != 0
            )
        }) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "streaming compressed NTFS data is not supported",
            ));
        }
        let size = data_extents_declared_size(&extents, self.cluster_size)?;
        Ok(NtfsFileReader {
            filesystem: self,
            extents,
            position: 0,
            size,
        })
    }
}

impl Read for NtfsFileReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.position >= self.size {
            return Ok(0);
        }
        let length = output
            .len()
            .min(usize::try_from(self.size - self.position).unwrap_or(usize::MAX));
        let bytes =
            self.filesystem
                .read_data_extents_range(&self.extents, self.position, length)?;
        if bytes.len() > length {
            return Err(invalid_fs_data(
                "NTFS range reader returned more bytes than requested",
            ));
        }
        output[..bytes.len()].copy_from_slice(&bytes);
        self.position = self
            .position
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid_fs_data("NTFS file stream position overflow"))?;
        Ok(bytes.len())
    }
}

impl Seek for NtfsFileReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let target = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(delta) => i128::from(self.size) + i128::from(delta),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
        };
        self.position = u64::try_from(target)
            .map_err(|_| invalid_fs_data("invalid negative NTFS file stream seek"))?;
        Ok(self.position)
    }
}
