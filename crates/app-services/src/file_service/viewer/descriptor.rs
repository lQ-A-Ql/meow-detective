//! Preview descriptor construction and caching.

use crate::file_service::viewer::{PreviewCephFsDescriptor, PreviewDescriptor, PreviewReadContext};
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

    let partition_candidates = match source_kind.as_str() {
        "logical_directory" | "ceph_fs" => Vec::new(),
        "e01" | "ceph_rbd" => {
            let expected_partition_index =
                crate::file_service::viewer::resolve_partition_index_for_entry(repo, entry)?;
            crate::file_service::viewer::e01_partition_candidates(
                conn,
                entry,
                expected_partition_index,
            )?
        }
        "raw" => {
            let expected_partition_index =
                crate::file_service::viewer::resolve_partition_index_for_entry(repo, entry)?;
            crate::file_service::viewer::raw_partition_candidates(
                &source_path,
                expected_partition_index,
            )?
        }
        other => {
            return Err(FileServiceError::other(format!(
                "Range reading is not yet wired for data source kind '{}'",
                other
            )))
        }
    };
    let selected = partition_candidates.first();
    let ceph_fs = if source_kind == "ceph_fs" {
        Some(cephfs_descriptor(conn, entry)?)
    } else {
        None
    };

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
        ceph_fs,
    })
}

fn cephfs_descriptor(
    conn: &Connection,
    entry: &FileEntry,
) -> Result<PreviewCephFsDescriptor, FileServiceError> {
    let locator =
        persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepo::new(conn)
            .find_file_locator(&entry.data_source_id.0, &entry.id.0)?
            .ok_or_else(|| {
                FileServiceError::not_found("Published CephFS file locator not found")
            })?;
    if !matches!(locator.entry_kind.as_str(), "file" | "symlink")
        || locator.size != entry.size.unwrap_or(0)
    {
        return Err(FileServiceError::other(
            "CephFS file locator does not match the file catalog",
        ));
    }
    Ok(PreviewCephFsDescriptor {
        filesystem_identity: locator.filesystem_identity,
        filesystem_id: locator.filesystem_id,
        fsmap_epoch: locator.fsmap_epoch,
        inode: locator.inode,
        stripe_unit: locator.stripe_unit,
        stripe_count: locator.stripe_count,
        object_size: locator.object_size,
        pool_id: locator.pool_id,
        pool_namespace: locator.pool_namespace,
        inline_data: locator.inline_data,
        projection_sha256: locator.projection_sha256,
        schema_version: locator.schema_version,
        decoder_profile: locator.decoder_profile,
        sparse_extents: locator
            .sparse_extents
            .into_iter()
            .map(
                |extent| crate::ceph_reconstruction::CephFsSparseExtentProof {
                    offset: extent.offset,
                    length: extent.length,
                    evidence_sha256: extent.evidence_sha256,
                    proof_sha256: extent.proof_sha256,
                },
            )
            .collect(),
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
            Ok(_) => {
                tracing::debug!(
                    cache_key = %key,
                    "Discarding stale preview descriptor cache entry"
                );
            }
            Err(error) => {
                tracing::warn!(
                    cache_key = %key,
                    %error,
                    "Discarding malformed preview descriptor cache entry"
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
    let file_repo = FileRepo::new(conn);
    let entry = match file_repo.find_by_id(file_id) {
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

    let current_partition_index = match file_repo.find_partition_index_by_id(file_id) {
        Ok(index) => index,
        Err(error) => {
            tracing::warn!(
                %error,
                file_id = %file_id.0,
                "Failed to validate descriptor partition routing"
            );
            return false;
        }
    };
    let current_size = entry.size.unwrap_or(0);
    let current_modified = entry.modified_at.as_ref().map(|dt| dt.to_rfc3339());

    if descriptor.entry_size != current_size
        || descriptor.entry_modified_at != current_modified
        || descriptor.partition_index != current_partition_index
    {
        tracing::debug!(
            file_id = %file_id.0,
            cached_size = descriptor.entry_size,
            current_size = current_size,
            cached_modified = ?descriptor.entry_modified_at,
            current_modified = ?current_modified,
            cached_partition_index = ?descriptor.partition_index,
            current_partition_index = ?current_partition_index,
            "Preview descriptor metadata changed; rebuilding"
        );
        return false;
    }

    if descriptor.source_kind == "ceph_fs"
        && !cephfs_descriptor_is_fresh(conn, &entry, descriptor.ceph_fs.as_ref())
    {
        return false;
    }

    true
}

fn cephfs_descriptor_is_fresh(
    conn: &Connection,
    entry: &FileEntry,
    cached: Option<&PreviewCephFsDescriptor>,
) -> bool {
    let Some(cached) = cached else {
        return false;
    };
    persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepo::new(conn)
        .find_file_locator(&entry.data_source_id.0, &entry.id.0)
        .ok()
        .flatten()
        .is_some_and(|current| {
            current.filesystem_identity == cached.filesystem_identity
                && current.fsmap_epoch == cached.fsmap_epoch
                && current.inode == cached.inode
                && current.projection_sha256 == cached.projection_sha256
        })
}

pub(crate) fn descriptor_cache_key(case_id: &str, file_id: &FileEntryId) -> String {
    format!("preview-descriptor:v5:{case_id}:{}", file_id.0)
}
