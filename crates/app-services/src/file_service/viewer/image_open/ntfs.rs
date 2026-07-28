use crate::file_service::{
    viewer::{
        mft_file_locator_from_entry_id, open_first_image_path_seekable, PreviewDescriptor,
        PreviewPartitionCandidate, RangeContentReader,
    },
    FileServiceError,
};

pub(super) fn open_ntfs_descriptor_file(
    filesystem: fs_ntfs::NtfsReader,
    descriptor: &PreviewDescriptor,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
) -> Result<RangeContentReader, FileServiceError> {
    let Some(inode) = descriptor_inode(descriptor, candidate)? else {
        return open_first_image_path_seekable(&filesystem, paths).map_err(FileServiceError::Io);
    };
    if !filesystem.supports_file_stream_by_inode(inode)? {
        return open_first_image_path_seekable(&filesystem, paths).map_err(FileServiceError::Io);
    }
    let reader = filesystem.into_file_stream_by_inode(inode)?;
    Ok(RangeContentReader::Seekable(Box::new(reader)))
}

pub(crate) fn open_ntfs_descriptor_stream(
    filesystem: fs_ntfs::NtfsReader,
    descriptor: &PreviewDescriptor,
    candidate: &PreviewPartitionCandidate,
) -> Result<Option<fs_ntfs::NtfsFileReader>, FileServiceError> {
    let Some(inode) = descriptor_inode(descriptor, candidate)? else {
        return Ok(None);
    };
    if !filesystem.supports_file_stream_by_inode(inode)? {
        return Ok(None);
    }
    let reader = filesystem.into_file_stream_by_inode(inode)?;
    Ok(Some(reader))
}

fn descriptor_inode(
    descriptor: &PreviewDescriptor,
    candidate: &PreviewPartitionCandidate,
) -> Result<Option<u64>, FileServiceError> {
    let Some((partition_index, inode)) = mft_file_locator_from_entry_id(&descriptor.file_id) else {
        return Ok(None);
    };
    if partition_index != candidate.partition_index {
        return Err(FileServiceError::security(
            "MFT file identifier does not match the routed partition",
        ));
    }
    Ok(Some(inode))
}
