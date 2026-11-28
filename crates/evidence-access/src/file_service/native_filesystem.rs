use evidence_core::{EvidenceReader, FileSystemReader};

use super::{viewer::PreviewPartitionCandidate, FileServiceError};

pub fn open_source_filesystem(
    reader: Box<dyn EvidenceReader>,
    candidate: &crate::datasource_service::ImageFilesystemCandidate,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    let preview = super::filesystem_locators::preview_candidate_for_locator(candidate)?;
    open_preview_filesystem(reader, &preview)
}

pub(super) fn open_preview_filesystem(
    reader: Box<dyn EvidenceReader>,
    candidate: &PreviewPartitionCandidate,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    let (reader, offset) = if let Some(identity) = &candidate.lvm_identity {
        let pool = fs_lvm::LvmPool::discover(vec![reader], identity.pv_offsets.clone())
            .map_err(|error| FileServiceError::Io(std::io::Error::other(error)))?;
        (
            pool.open_volume_by_uuid(&identity.vg_uuid, &identity.lv_uuid)
                .map_err(|error| FileServiceError::Io(std::io::Error::other(error)))?,
            0,
        )
    } else {
        (reader, candidate.offset)
    };
    super::open_block_filesystem(&candidate.filesystem_kind, reader, offset)
}
