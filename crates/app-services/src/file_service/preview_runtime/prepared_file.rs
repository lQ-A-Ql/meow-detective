use crate::file_service::{
    preview_runtime::{
        prepared_filesystem::PreparedFilesystemFile, prepared_ntfs::PreparedNtfsFile,
    },
    FileServiceError,
};

pub(crate) enum PreparedFile {
    NtfsInode(PreparedNtfsFile),
    FilesystemPath(PreparedFilesystemFile),
}

impl PreparedFile {
    pub(crate) fn open_ntfs(
        reader: Box<dyn evidence_core::EvidenceReader>,
        filesystem_offset: u64,
        inode: u64,
    ) -> Result<Self, FileServiceError> {
        PreparedNtfsFile::open(reader, filesystem_offset, inode).map(Self::NtfsInode)
    }

    pub(crate) fn open_filesystem(
        filesystem: Box<dyn evidence_core::FileSystemReader + Send>,
        descriptor: &crate::file_service::PreviewDescriptor,
    ) -> Result<Self, FileServiceError> {
        PreparedFilesystemFile::open(filesystem, descriptor).map(Self::FilesystemPath)
    }

    pub(crate) fn read_range(
        &mut self,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        match self {
            Self::NtfsInode(file) => file.read_range(offset, length),
            Self::FilesystemPath(file) => file.read_range(offset, length),
        }
    }
}
