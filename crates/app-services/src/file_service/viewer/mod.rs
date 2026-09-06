//! Viewer subsystem for source-scoped handles and bounded evidence reads.

mod descriptor;
mod document;
mod filesystem;
mod handle;
mod image;
mod image_open;
mod io;
mod media;
mod model;
mod office_preview;
pub(crate) mod partition;
mod path;
mod preview_bytes;
mod range;
mod range_fs;
mod text;
mod validation;

pub use crate::e01_reader_cache::{clear_e01_reader_cache, clear_e01_reader_cache_for_case};
pub use document::document_preview_for_file;
pub use handle::{get_file_path_for_entry, open_file_handle_real};
pub use image::image_preview_for_file;
pub use io::skip_reader_bytes;
pub use media::{media_preview_plan_for_file, media_range_for_file, MediaPreviewPlan};
pub use path::safe_relative_path;
pub use preview_bytes::read_preview_bytes_for_file;
pub(crate) use range::open_range_content_for_descriptor_with_context;
pub(crate) use range::read_file_bytes_for_descriptor_with_context;
pub(crate) use range::read_file_header_with_context;
pub use range::{
    open_file_content_by_id, read_file_bytes_for_case, read_file_header_by_id,
    read_file_range_for_case, FileHeaderReadCache,
};
pub use text::text_preview_for_file;

pub(crate) use crate::e01_reader_cache::open_e01_reader_cached;
pub(crate) use descriptor::{descriptor_for_file_with_cache, preview_descriptor_for_case};
pub(crate) use filesystem::{
    format_image_range_error, is_exfat_filesystem_kind, is_fat_filesystem_kind,
    is_linux_filesystem_kind, is_preview_image_filesystem_kind, looks_like_exfat_boot_sector,
    mft_file_locator_from_entry_id, resolve_partition_index_for_entry,
};
pub(crate) use image_open::{
    open_candidate_block_reader_with_lvm_cache, open_descriptor_image_file,
    open_descriptor_image_file_with_context, open_e01_file, open_local_disk_file,
    open_ntfs_descriptor_stream, open_raw_file, LvmPoolRequestCache,
};
pub(crate) use io::{
    open_first_image_path, open_first_image_path_seekable, read_bounded, read_seekable_range,
};
pub(crate) use model::open_host_evidence_reader;
pub(crate) use model::{
    PreviewCephFsDescriptor, PreviewDescriptor, PreviewLvmIdentity, PreviewLvmPhysicalVolumeSource,
    PreviewPartitionCandidate, PreviewReadContext, RangeContentReader, FILE_HANDLE_PREFIX,
};
pub(crate) use partition::{
    block_partition_candidates, e01_partition_candidates, exact_partition_candidate,
    local_disk_partition_candidates, preview_lvm_identity_from_datasource,
    preview_partition_candidate_from_record, raw_partition_candidates,
};
pub(crate) use path::{
    descriptor_file_entry, descriptor_image_path_candidates, entry_image_path_candidates,
};
pub(crate) use range::file_id_from_handle;
pub(crate) use range_fs::{
    open_filesystem_reader, try_read_bitlocker_ntfs_range_for_descriptor,
    try_read_exfat_image_range_for_descriptor, try_read_exfat_image_range_for_entry,
    try_read_fat_image_range_for_descriptor, try_read_fat_image_range_for_entry,
    try_read_linux_image_range_for_descriptor, try_read_linux_image_range_for_entry,
    try_read_ntfs_image_range_for_descriptor, try_read_ntfs_image_range_for_entry,
};
pub(crate) use validation::{validate_file_encryption_status, validate_readable_file_entry};

#[cfg(test)]
#[path = "../../../tests/unit/file_service/viewer.rs"]
mod tests;
