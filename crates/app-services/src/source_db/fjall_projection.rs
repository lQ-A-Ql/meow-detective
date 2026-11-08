use super::{open_registered_fjall_source_read_only, DataSourceId, FjallStorageRuntimeError};
use std::path::Path;

pub fn ready_source_catalog_count(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<Option<u64>, FjallStorageRuntimeError> {
    let source = open_registered_fjall_source_read_only(case_root, data_source_id)?;
    let snapshot = source.snapshot();
    source.require_projection_complete(&snapshot, &data_source_id.0)?;
    source.catalog_count(&snapshot, &data_source_id.0).map(Some)
}

pub fn ready_source_timeline_count(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<Option<u64>, FjallStorageRuntimeError> {
    let source = open_registered_fjall_source_read_only(case_root, data_source_id)?;
    let snapshot = source.snapshot();
    source.require_projection_complete(&snapshot, &data_source_id.0)?;

    let count = source.timeline_count_by_time(&snapshot, &data_source_id.0, None, None)?;
    Ok(Some(count))
}

pub fn ready_source_artifact_count(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<Option<u64>, FjallStorageRuntimeError> {
    let source = open_registered_fjall_source_read_only(case_root, data_source_id)?;
    let snapshot = source.snapshot();
    source.require_projection_complete(&snapshot, &data_source_id.0)?;
    source
        .artifact_count(&snapshot, &data_source_id.0)
        .map(Some)
}
