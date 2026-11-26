mod bitlocker;
mod common;
mod exfat;
mod fat;
mod linux;
mod ntfs;

pub(crate) use bitlocker::try_read_bitlocker_ntfs_range_for_descriptor;
pub(crate) use exfat::{
    try_read_exfat_image_range_for_descriptor, try_read_exfat_image_range_for_entry,
};
pub(crate) fn open_filesystem_reader(
    candidate: &super::PreviewPartitionCandidate,
    reader: Box<dyn evidence_core::EvidenceReader>,
    offset: u64,
) -> Result<Box<dyn evidence_core::FileSystemReader + Send>, crate::file_service::FileServiceError>
{
    crate::file_service::open_block_filesystem(&candidate.filesystem_kind, reader, offset)
}
pub(crate) use fat::{try_read_fat_image_range_for_descriptor, try_read_fat_image_range_for_entry};
pub(crate) use linux::{
    try_read_linux_image_range_for_descriptor, try_read_linux_image_range_for_entry,
};
pub(crate) use ntfs::{
    try_read_ntfs_image_range_for_descriptor, try_read_ntfs_image_range_for_entry,
};
