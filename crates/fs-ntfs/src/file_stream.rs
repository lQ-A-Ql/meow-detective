//! Bounded streaming access to one NTFS file.

use std::io::{self, Read, Seek, SeekFrom};

use crate::attribute::{data_extents_declared_size, DataAttributeExtent};
use crate::{invalid_fs_data, wof::WofStream, NtfsReader};

/// Seekable file reader backed by bounded NTFS data-run range reads.
pub struct NtfsFileReader {
    filesystem: NtfsReader,
    backing: FileBacking,
    position: u64,
    size: u64,
}

enum FileBacking {
    Plain(Vec<DataAttributeExtent>),
    Wof(WofStream),
}

impl NtfsReader {
    /// Whether an inode can use bounded range reads without whole-file decoding.
    pub fn supports_file_stream_by_inode(&self, inode: u64) -> io::Result<bool> {
        let record = self.read_mft_record(inode)?;
        match self.wof_stream_from_record(inode, &record) {
            Ok(Some(_)) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::Unsupported => return Ok(false),
            Err(error) => return Err(error),
            Ok(None) => {}
        }
        let extents = self.collect_unnamed_data_extents_from_base(inode, &record)?;
        Ok(extents.iter().all(|extent| match extent {
            DataAttributeExtent::Resident { .. } => true,
            DataAttributeExtent::NonResident { attr_flags, .. } => attr_flags & 0x0001 == 0,
        }))
    }

    /// Consume the filesystem reader and open one inode as a bounded stream.
    pub fn into_file_stream_by_inode(self, inode: u64) -> io::Result<NtfsFileReader> {
        let record = self.read_mft_record(inode)?;
        if let Some(wof) = self.wof_stream_from_record(inode, &record)? {
            let size = wof.logical_size;
            return Ok(NtfsFileReader {
                filesystem: self,
                backing: FileBacking::Wof(wof),
                position: 0,
                size,
            });
        }
        let extents = self.collect_unnamed_data_extents_from_base(inode, &record)?;
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
            backing: FileBacking::Plain(extents),
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
        let bytes = match &self.backing {
            FileBacking::Plain(extents) => {
                self.filesystem
                    .read_data_extents_range(extents, self.position, length)?
            }
            FileBacking::Wof(wof) => wof.read_range(&self.filesystem, self.position, length)?,
        };
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
