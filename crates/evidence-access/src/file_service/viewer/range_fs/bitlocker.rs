use crate::file_service::{
    viewer::{PreviewDescriptor, PreviewPartitionCandidate, PreviewReadContext},
    FileServiceError,
};

pub(crate) fn try_read_bitlocker_ntfs_range_for_descriptor<C>(
    context: &mut C,
    descriptor: &PreviewDescriptor,
    candidate: &PreviewPartitionCandidate,
    offset: u64,
    length: usize,
) -> Result<Option<Vec<u8>>, FileServiceError>
where
    C: PreviewReadContext,
{
    let Some((partition_index, inode)) =
        crate::file_service::viewer::filesystem::mft_file_locator_from_entry_id(
            &descriptor.file_id,
        )
    else {
        return Ok(None);
    };
    if partition_index != candidate.partition_index {
        return Err(FileServiceError::security(
            "MFT file identifier does not match the routed partition",
        ));
    }

    let (reader, filesystem_offset, filesystem_kind) =
        context.open_candidate_block_reader(descriptor, candidate)?;
    if !filesystem_kind.eq_ignore_ascii_case("NTFS") {
        return Ok(None);
    }

    let filesystem = fs_ntfs::NtfsReader::open(reader, filesystem_offset)?;
    filesystem
        .read_file_range_by_inode(inode, offset, length)
        .map(Some)
        .map_err(Into::into)
}
