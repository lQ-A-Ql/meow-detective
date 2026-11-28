use std::path::Path;

use evidence_core::FileSystemReader;

use crate::datasource_service::{self, ImageFilesystemCandidate, ImageFilesystemProbe};
use crate::file_service::{
    filesystem_locators::preview_candidate_for_locator, open_block_filesystem,
    open_candidate_block_reader_with_lvm_cache, FileServiceError, LvmPoolRequestCache,
};

pub fn probe_mdraid_filesystems(
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<ImageFilesystemProbe, super::MdRaidPlanError> {
    let native_plan = super::read_mdraid_descriptor(path)?;
    let access = crate::structure_access::plan_discovery_access(
        crate::structure_access::AccessTarget::MdRaid(&native_plan),
        cancel,
    )?;
    let mut reader = crate::structure_access::open_discovery_view(access, cancel)?;
    let mut probe = datasource_service::detect_image_filesystem(&mut reader)?;
    datasource_service::expand_lvm_pool_candidates(
        &mut probe,
        path,
        &domain::DataSourceKind::MdRaid,
    );
    let indices = datasource_service::assign_effective_partition_indices(&probe.candidates);
    for (position, candidate) in probe.candidates.iter_mut().enumerate() {
        candidate.partition_index = Some(datasource_service::effective_partition_index(
            candidate, position, &indices,
        ));
    }
    Ok(probe)
}

pub fn open_mdraid_filesystem(
    path: &Path,
    candidate: &ImageFilesystemCandidate,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    let preview = preview_candidate_for_locator(candidate)?;
    let mut cache = LvmPoolRequestCache::new();
    let (reader, offset) = open_candidate_block_reader_with_lvm_cache(
        path,
        &preview,
        &mut |path| datasource_service::open_evidence_reader(path, &domain::DataSourceKind::MdRaid),
        &mut cache,
    )?;
    open_block_filesystem(&preview.filesystem_kind, reader, offset)
}

pub(crate) fn open_mdraid_partition(
    source_id: &str,
    path: &Path,
    partition_index: Option<usize>,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    super::persisted_inputs::validate(&domain::DataSourceId(source_id.to_owned()), path)
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    crate::file_service::registered_partition::open_registered_partition(
        source_id,
        path,
        partition_index,
    )
}
