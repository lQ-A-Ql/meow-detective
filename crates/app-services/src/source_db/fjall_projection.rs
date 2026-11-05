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
    if !projection_readable(&source, &snapshot, &data_source_id.0)? {
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
    if !projection_readable(&source, &snapshot, &data_source_id.0)?
        || (!source.timeline_counts_ready(&snapshot, &data_source_id.0)?
            && source.timeline_count_by_time(&snapshot, &data_source_id.0, None, None)? == 0)
    {
        return Ok(None);
    }

    let count = source.timeline_count_by_time(&snapshot, &data_source_id.0, None, None)?;
    Ok(Some(count))
}

pub fn ready_source_artifact_count(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<Option<u64>, FjallStorageRuntimeError> {
    let source_root = crate::storage_runtime::source_storage_root(case_root, &data_source_id.0)?;
    if !source_root.is_dir() {
        return Ok(None);
    }
    let source = open_registered_fjall_source_read_only(case_root, data_source_id)?;
    let snapshot = source.snapshot();
    if !projection_readable(&source, &snapshot, &data_source_id.0)? {
        return Ok(None);
    }
    let mut cursor = None;
    let mut count = 0u64;
    loop {
        let page = source.artifact_list_by_source(
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

fn projection_readable(
    source: &crate::storage_runtime::ReadOnlySourceStorageHandle,
    snapshot: &persistence_fjall::SourceReadSnapshot,
    source_id: &str,
) -> Result<bool, FjallStorageRuntimeError> {
    if source.source_projection_complete(snapshot, source_id)? {
        return Ok(true);
    }
    let key = persistence_fjall::encode_key(
        "projection",
        &[persistence_fjall::KeyPart::Text(source_id.to_owned())],
    )?;
    Ok(!source
        .read_envelope(snapshot, persistence_fjall::KeyspaceName::Meta, &key)?
        .is_some_and(|envelope| envelope.record_kind == "source_projection_building"))
}
