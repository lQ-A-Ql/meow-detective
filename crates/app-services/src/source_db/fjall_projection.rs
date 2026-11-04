use super::{open_registered_fjall_source_read_only, DataSourceId, FjallStorageRuntimeError};
use std::path::Path;

pub fn ready_source_timeline_count(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<Option<u64>, FjallStorageRuntimeError> {
    let source_root = crate::storage_runtime::source_storage_root(case_root, &data_source_id.0)?;
    if !source_root.is_dir() {
        return Ok(None);
    }

    let source = open_registered_fjall_source_read_only(case_root, data_source_id)?;
    let snapshot = source.snapshot();
    if !source.source_projection_complete(&snapshot, &data_source_id.0)?
        || !source.timeline_counts_ready(&snapshot, &data_source_id.0)?
    {
        return Ok(None);
    }

    source
        .timeline_count_by_time(&snapshot, &data_source_id.0, None, None)
        .map(Some)
}
