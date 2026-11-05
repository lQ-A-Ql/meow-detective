use super::{open_registered_fjall_source_read_only, DataSourceId, FjallStorageRuntimeError};
use persistence_fjall::ScanDirection;
use std::path::Path;

pub fn ready_source_catalog_count(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<Option<u64>, FjallStorageRuntimeError> {
    let source_root = crate::storage_runtime::source_storage_root(case_root, &data_source_id.0)?;
    if !source_root.is_dir() {
        return Ok(None);
    }

    let source = open_registered_fjall_source_read_only(case_root, data_source_id)?;
    let snapshot = source.snapshot();
    if !source.source_projection_complete(&snapshot, &data_source_id.0)? {
        return Ok(None);
    }

    let mut cursor = None;
    let mut count = 0u64;
    loop {
        let page = source.catalog_list_all(
            &snapshot,
            &data_source_id.0,
            cursor.as_deref(),
            ScanDirection::Forward,
            512,
        )?;
        count = count.saturating_add(page.records.len() as u64);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(Some(count));
        }
    }
}

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

    let count = source.timeline_count_by_time(&snapshot, &data_source_id.0, None, None)?;
    if count == 0 {
        return Ok(None);
    }
    Ok(Some(count))
}
