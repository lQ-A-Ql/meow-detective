use crate::file_service::FileServiceError;
use std::io::{Read, Seek, SeekFrom};

pub(crate) struct PreparedNtfsFile {
    stream: fs_ntfs::NtfsFileReader,
}

impl PreparedNtfsFile {
    pub(crate) fn open(
        reader: Box<dyn evidence_core::EvidenceReader>,
        filesystem_offset: u64,
        inode: u64,
    ) -> Result<Self, FileServiceError> {
        Ok(Self {
            stream: fs_ntfs::NtfsReader::open(reader, filesystem_offset)?
                .into_file_stream_by_inode(inode)?,
        })
    }

    pub(crate) fn read_range(
        &mut self,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        self.stream.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        self.stream
            .by_ref()
            .take(length as u64)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}
