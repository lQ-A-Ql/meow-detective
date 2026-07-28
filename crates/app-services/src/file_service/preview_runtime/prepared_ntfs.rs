use crate::file_service::FileServiceError;

pub(crate) struct PreparedNtfsFile {
    filesystem: fs_ntfs::NtfsReader,
    inode: u64,
}

impl PreparedNtfsFile {
    pub(crate) fn open(
        reader: Box<dyn evidence_core::EvidenceReader>,
        filesystem_offset: u64,
        inode: u64,
    ) -> Result<Self, FileServiceError> {
        Ok(Self {
            filesystem: fs_ntfs::NtfsReader::open(reader, filesystem_offset)?,
            inode,
        })
    }

    pub(crate) fn read_range(
        &self,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        self.filesystem
            .read_file_range_by_inode(self.inode, offset, length)
            .map_err(Into::into)
    }
}
