use std::path::Path;
use std::sync::atomic::AtomicBool;

use evidence_core::FileSystemReader;

use crate::datasource_service::{
    self, ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe,
};
use crate::file_service::{
    filesystem_locators::preview_candidate_for_locator, open_block_filesystem, FileServiceError,
};

use super::plan::LvmPlanError;

pub fn probe_lvm_filesystems(
    descriptor_path: &Path,
    cancel: &AtomicBool,
) -> Result<ImageFilesystemProbe, LvmPlanError> {
    let plan = super::descriptor::read_lvm_descriptor(descriptor_path)?;
    let mut reader = super::reader::open_lvm_volume_reader(&plan, cancel)?;
    let mut probe = datasource_service::detect_image_filesystem(&mut reader)?;
    if probe
        .candidates
        .iter()
        .any(|candidate| candidate.kind == ImageFilesystemKind::LvmPool)
    {
        return Err(LvmPlanError::InvalidContentIdentity);
    }
    let indices = datasource_service::assign_effective_partition_indices(&probe.candidates);
    for (position, candidate) in probe.candidates.iter_mut().enumerate() {
        candidate.partition_index = Some(datasource_service::effective_partition_index(
            candidate, position, &indices,
        ));
    }
    Ok(probe)
}

pub fn open_lvm_filesystem(
    descriptor_path: &Path,
    candidate: &ImageFilesystemCandidate,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    let plan = super::descriptor::read_lvm_descriptor(descriptor_path)
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    let cancel = AtomicBool::new(false);
    let reader = super::reader::open_lvm_volume_reader(&plan, &cancel)
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    let preview = preview_candidate_for_locator(candidate)?;
    open_block_filesystem(&preview.filesystem_kind, reader, preview.offset)
}

pub fn open_lvm_partition(
    source_id: &str,
    descriptor_path: &Path,
    partition_index: Option<usize>,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    super::persisted_inputs::validate(&domain::DataSourceId(source_id.to_owned()), descriptor_path)
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    crate::file_service::registered_partition::open_registered_partition(
        source_id,
        descriptor_path,
        partition_index,
    )
}
