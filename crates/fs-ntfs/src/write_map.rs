//! Extent-to-volume-offset mapping for bounded, same-size content writes.
//!
//! The emulation bypass writes modified system hives through a copy-on-write
//! overlay, which requires translating file-relative offsets into absolute
//! volume offsets. Only plainly allocated, non-resident, uncompressed,
//! unencrypted streams are mappable — anything else fails closed.

use std::io;

use crate::attribute::{data_extent_logical_start, DataAttributeExtent};
use crate::invalid_fs_data;

/// One contiguous mapping between a file's `$DATA` stream and byte offsets in
/// the reader's coordinate space (i.e. including any volume base the reader
/// was opened with).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NtfsFileExtent {
    /// Byte offset within the file's data stream.
    pub logical_offset: u64,
    /// Byte offset in the reader's coordinate space.
    pub volume_offset: u64,
    /// Mapped length in bytes.
    pub length: u64,
}

impl crate::NtfsReader {
    /// Map the unnamed `$DATA` stream of a file to absolute volume offsets.
    pub fn file_extent_map(&self, path: &str) -> io::Result<Vec<NtfsFileExtent>> {
        let preview = self.preview_file(path)?;
        self.file_extent_map_by_inode(preview.inode())
    }

    /// Inode-based variant of [`Self::file_extent_map`].
    pub fn file_extent_map_by_inode(&self, inode: u64) -> io::Result<Vec<NtfsFileExtent>> {
        let extents = self.collect_unnamed_data_extents(inode)?;
        let mut mapped = Vec::new();
        for extent in &extents {
            let DataAttributeExtent::NonResident {
                attr_flags, runs, ..
            } = extent
            else {
                return Err(invalid_fs_data(
                    "resident stream cannot be raw-written through an overlay",
                ));
            };
            if *attr_flags != 0 {
                return Err(invalid_fs_data(
                    "compressed, encrypted, or sparse stream cannot be raw-written",
                ));
            }
            let mut logical = data_extent_logical_start(extent, self.cluster_size())?;
            for run in runs {
                let length = run
                    .cluster_count
                    .checked_mul(self.cluster_size())
                    .ok_or_else(|| invalid_fs_data("data run length overflows"))?;
                if run.lcn.is_none() {
                    return Err(invalid_fs_data(
                        "sparse stream cannot be raw-written through an overlay",
                    ));
                }
                mapped.push(NtfsFileExtent {
                    logical_offset: logical,
                    volume_offset: self.data_run_source_offset(run)?,
                    length,
                });
                logical = logical
                    .checked_add(length)
                    .ok_or_else(|| invalid_fs_data("extent logical offset overflows"))?;
            }
        }
        Ok(mapped)
    }
}
