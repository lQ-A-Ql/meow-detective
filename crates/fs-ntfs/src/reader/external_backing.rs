//! Prevent sparse WOF placeholder streams from being presented as file content.
use std::io;

use crate::attribute::DataAttributeExtent;

const ATTR_TYPE_REPARSE_POINT: u32 = 0xc0;
const IO_REPARSE_TAG_WOF: u32 = 0x8000_0017;

impl crate::NtfsReader {
    pub(crate) fn collect_readable_data_extents(
        &self,
        inode: u64,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        let record = self.read_mft_record(inode)?;
        self.ensure_supported_file_backing(inode, &record)?;
        self.collect_unnamed_data_extents_from_base(inode, &record)
    }

    pub(super) fn ensure_supported_file_backing(
        &self,
        inode: u64,
        record: &[u8],
    ) -> io::Result<()> {
        let reparse =
            self.collect_attribute_extents_from_base(inode, record, ATTR_TYPE_REPARSE_POINT, None)?;
        if reparse.is_empty() {
            return Ok(());
        }
        let tag = self.read_data_extents_range(&reparse, 0, 4)?;
        let tag: [u8; 4] = tag
            .try_into()
            .map_err(|_| crate::invalid_fs_data("truncated NTFS reparse tag"))?;
        if u32::from_le_bytes(tag) == IO_REPARSE_TAG_WOF {
            // WOF stores the logical bytes in an external provider, often the
            // WofCompressedData ADS. Its unnamed sparse stream contains zeros.
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "WOF external backing requires decoding before file content can be read",
            ));
        }
        Ok(())
    }
}
