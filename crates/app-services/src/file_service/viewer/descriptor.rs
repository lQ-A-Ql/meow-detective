//! Preview descriptor construction and caching.

use crate::file_service::viewer::{PreviewDescriptor, PreviewReadContext};
use crate::file_service::{metadata::lookup::mime_for_entry, FileServiceError};
use domain::{EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;

pub fn preview_descriptor_for_case(
    conn: &Connection,
    case_id: &str,
    file_id: &FileEntryId,
) -> Result<PreviewDescriptor, FileServiceError> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(file_id)?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;

    preview_descriptor_for_entry(conn, &repo, case_id, &entry)
}

fn preview_descriptor_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    case_id: &str,
    entry: &FileEntry,
) -> Result<PreviewDescriptor, FileServiceError> {
    if entry.entry_type != EntryType::File {
        return Err(FileServiceError::invalid_input(
            "Cannot read a directory as a file",
        ));
    }

    let (source_kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index =
        crate::file_service::viewer::root_partition_index_for_entry(repo, entry);

    let partition_candidates = match source_kind.as_str() {
        "logical_directory" => Vec::new(),
        "e01" => crate::file_service::viewer::e01_partition_candidates(
            conn,
            entry,
            expected_partition_index,
        )?,
        "raw" => crate::file_service::viewer::raw_partition_candidates(
            &source_path,
            expected_partition_index,
        )?,
        other => {
            return Err(FileServiceError::other(format!(
                "Range reading is not yet wired for data source kind '{}'",
                other
            )))
        }
    };
    let selected = partition_candidates.first();

    Ok(PreviewDescriptor {
        case_id: case_id.to_string(),
        file_id: entry.id.0.clone(),
        source_kind,
        source_path,
        partition_index: selected.map(|candidate| candidate.partition_index),
        filesystem_kind: selected.map(|candidate| candidate.filesystem_kind.clone()),
        path: entry.path.clone(),
        mime: mime_for_entry(entry),
        size: entry.size.unwrap_or(0),
        data_source_id: entry.data_source_id.0.clone(),
        partition_candidates,
        entry_size: entry.size.unwrap_or(0),
        entry_modified_at: entry.modified_at.as_ref().map(|dt| dt.to_rfc3339()),
    })
}

pub(crate) fn descriptor_for_file_with_cache<C>(
    context: &mut C,
    file_id: &FileEntryId,
) -> Result<PreviewDescriptor, FileServiceError>
where
    C: PreviewReadContext,
{
    let case_id = context.case_id().to_string();
    let key = descriptor_cache_key(&case_id, file_id);
    if let Some(value) = context.get_cached_preview_descriptor(&key) {
        match serde_json::from_value::<PreviewDescriptor>(value) {
            Ok(descriptor)
                if descriptor.case_id == case_id
                    && descriptor.file_id == file_id.0
                    && descriptor_is_fresh(context.conn(), file_id, &descriptor) =>
            {
                return Ok(descriptor);
            }
            Ok(_) | Err(_) => {
                tracing::warn!(
                    cache_key = %key,
                    "Ignoring stale or invalid preview descriptor cache entry"
                );
            }
        }
    }

    let descriptor = preview_descriptor_for_case(context.conn(), &case_id, file_id)?;
    if let Ok(value) = serde_json::to_value(&descriptor) {
        context.set_cached_preview_descriptor(&key, &value);
    }
    Ok(descriptor)
}

/// Check whether a cached preview descriptor still matches the current file
/// entry metadata. A mismatch indicates the entry was updated in place (for
/// example by a re-import or staging merge) and the descriptor must be rebuilt.
pub(crate) fn descriptor_is_fresh(
    conn: &Connection,
    file_id: &FileEntryId,
    descriptor: &PreviewDescriptor,
) -> bool {
    let repo = match FileRepo::new(conn).find_by_id(file_id) {
        Ok(Some(entry)) => entry,
        Ok(None) => {
            tracing::debug!(file_id = %file_id.0, "File entry not found; treating descriptor as stale");
            return false;
        }
        Err(error) => {
            tracing::warn!(%error, file_id = %file_id.0, "Failed to validate descriptor freshness");
            return false;
        }
    };

    let current_size = repo.size.unwrap_or(0);
    let current_modified = repo.modified_at.as_ref().map(|dt| dt.to_rfc3339());

    if descriptor.entry_size != current_size || descriptor.entry_modified_at != current_modified {
        tracing::debug!(
            file_id = %file_id.0,
            cached_size = descriptor.entry_size,
            current_size = current_size,
            cached_modified = ?descriptor.entry_modified_at,
            current_modified = ?current_modified,
            "Preview descriptor metadata changed; rebuilding"
        );
        return false;
    }

    true
}

pub(crate) fn descriptor_cache_key(case_id: &str, file_id: &FileEntryId) -> String {
    format!("preview-descriptor:v2:{case_id}:{}", file_id.0)
}
