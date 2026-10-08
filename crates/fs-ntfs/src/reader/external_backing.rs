//! Resolve logical WOF file content through its owned compressed data stream.
use std::io;

use crate::{
    attribute::data_extents_declared_size,
    wof::{WofAlgorithm, WofStream},
};

const ATTR_TYPE_REPARSE_POINT: u32 = 0xc0;

impl crate::NtfsReader {
    pub(crate) fn wof_stream_from_record(
        &self,
        inode: u64,
        record: &[u8],
    ) -> io::Result<Option<WofStream>> {
        let reparse =
            self.collect_attribute_extents_from_base(inode, record, ATTR_TYPE_REPARSE_POINT, None)?;
        if reparse.is_empty() {
            return Ok(None);
        }
        let length = data_extents_declared_size(&reparse, self.cluster_size)?;
        let tag = self.read_data_extents_range(&reparse, 0, 4)?;
        if tag.len() != 4 {
            return Err(crate::invalid_fs_data("truncated NTFS reparse tag"));
        }
        if tag.as_slice() != 0x8000_0017u32.to_le_bytes() {
            return Ok(None);
        }
        if !(4..=64).contains(&length) {
            return Err(crate::invalid_fs_data("invalid WOF reparse payload size"));
        }
        let data = self.read_data_extents_range(&reparse, 0, length as usize)?;
        let Some(algorithm) = WofAlgorithm::from_reparse(&data)? else {
            return Ok(None);
        };
        let unnamed = self.collect_unnamed_data_extents_from_base(inode, record)?;
        if unnamed.is_empty() {
            return Err(crate::invalid_fs_data("WOF file has no logical size"));
        }
        let logical_size = data_extents_declared_size(&unnamed, self.cluster_size)?;
        let compressed = self.collect_attribute_extents_from_base(
            inode,
            record,
            crate::ATTR_TYPE_DATA,
            Some("WofCompressedData"),
        )?;
        WofStream::new(algorithm, logical_size, compressed, self.cluster_size).map(Some)
    }
}
