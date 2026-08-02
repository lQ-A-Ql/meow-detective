use domain::DataSourceId;
use evidence_core::{FileSystemDirectoryLocator, FileSystemFileLocator, FileSystemReader};
use persistence_sqlite::repositories::filesystem_locator_repo::{
    FilesystemDirectoryLocatorRecord, FilesystemFileLocatorRecord, FilesystemLocatorRepo,
};
use sha2::{Digest, Sha256};

use crate::datasource_service::{ImageFilesystemCandidate, ImageFilesystemKind};

use super::{viewer::PreviewPartitionCandidate, FileServiceError};

const DERIVED_LOCATOR_SCOPE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RestoredFilesystemLocatorCounts {
    pub(crate) directories: usize,
    pub(crate) files: usize,
}

pub(crate) fn persist_filesystem_locators(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    candidate: &PreviewPartitionCandidate,
    scope_identity: &str,
    filesystem: &dyn FileSystemReader,
) -> Result<(), FileServiceError> {
    let repo = FilesystemLocatorRepo::new(conn);
    let directory_locators = directory_locator_records(filesystem);
    if !directory_locators.is_empty() {
        repo.replace_directory_locators(
            &data_source_id.0,
            candidate.partition_index,
            &candidate.filesystem_kind,
            scope_identity,
            &directory_locators,
        )?;
    }
    let file_locators = file_locator_records(filesystem);
    if !file_locators.is_empty() {
        repo.replace_file_locators(
            &data_source_id.0,
            candidate.partition_index,
            &candidate.filesystem_kind,
            scope_identity,
            &file_locators,
        )?;
    }
    Ok(())
}

pub(crate) fn load_directory_locators(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    candidate: &PreviewPartitionCandidate,
    scope_identity: &str,
) -> Result<Vec<FileSystemDirectoryLocator>, FileServiceError> {
    FilesystemLocatorRepo::new(conn)
        .list_directory_locators(
            &data_source_id.0,
            candidate.partition_index,
            &candidate.filesystem_kind,
            scope_identity,
        )
        .map(|records| records.into_iter().map(locator_from_record).collect())
        .map_err(FileServiceError::Db)
}

pub(crate) fn load_file_locators(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    candidate: &PreviewPartitionCandidate,
    scope_identity: &str,
) -> Result<Vec<FileSystemFileLocator>, FileServiceError> {
    FilesystemLocatorRepo::new(conn)
        .list_file_locators(
            &data_source_id.0,
            candidate.partition_index,
            &candidate.filesystem_kind,
            scope_identity,
        )
        .map(|records| records.into_iter().map(file_locator_from_record).collect())
        .map_err(FileServiceError::Db)
}

pub(crate) fn restore_filesystem_locators(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    candidate: &PreviewPartitionCandidate,
    scope_identity: &str,
    filesystem: &dyn FileSystemReader,
) -> RestoredFilesystemLocatorCounts {
    RestoredFilesystemLocatorCounts {
        directories: restore_directory_locators(
            conn,
            data_source_id,
            candidate,
            scope_identity,
            filesystem,
        ),
        files: restore_file_locators(conn, data_source_id, candidate, scope_identity, filesystem),
    }
}

pub(crate) fn restore_directory_locators(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    candidate: &PreviewPartitionCandidate,
    scope_identity: &str,
    filesystem: &dyn FileSystemReader,
) -> usize {
    let persisted = match load_directory_locators(conn, data_source_id, candidate, scope_identity) {
        Ok(persisted) => persisted,
        Err(error) => {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                partition_index = candidate.partition_index,
                filesystem = %candidate.filesystem_kind,
                error = %error,
                "Ignoring unreadable persisted filesystem directory locators"
            );
            return 0;
        }
    };
    match filesystem.seed_directory_locators(&persisted) {
        Ok(()) => persisted.len(),
        Err(error) => {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                partition_index = candidate.partition_index,
                filesystem = %candidate.filesystem_kind,
                error = %error,
                "Ignoring invalid persisted filesystem directory locators"
            );
            0
        }
    }
}

pub(crate) fn restore_file_locators(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    candidate: &PreviewPartitionCandidate,
    scope_identity: &str,
    filesystem: &dyn FileSystemReader,
) -> usize {
    let persisted = match load_file_locators(conn, data_source_id, candidate, scope_identity) {
        Ok(persisted) => persisted,
        Err(error) => {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                partition_index = candidate.partition_index,
                filesystem = %candidate.filesystem_kind,
                error = %error,
                "Ignoring unreadable persisted filesystem file locators"
            );
            return 0;
        }
    };
    match filesystem.seed_file_locators(&persisted) {
        Ok(()) => persisted.len(),
        Err(error) => {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                partition_index = candidate.partition_index,
                filesystem = %candidate.filesystem_kind,
                error = %error,
                "Ignoring invalid persisted filesystem file locators"
            );
            0
        }
    }
}

fn directory_locator_records(
    filesystem: &dyn FileSystemReader,
) -> Vec<FilesystemDirectoryLocatorRecord> {
    let mut records = filesystem
        .directory_locators()
        .into_iter()
        .map(|locator| FilesystemDirectoryLocatorRecord {
            path: locator.path,
            locator: locator.locator,
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.path.cmp(&right.path));
    records
}

fn file_locator_records(filesystem: &dyn FileSystemReader) -> Vec<FilesystemFileLocatorRecord> {
    let mut records = filesystem
        .file_locators()
        .into_iter()
        .map(|locator| FilesystemFileLocatorRecord {
            path: locator.path,
            locator: locator.locator,
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.path.cmp(&right.path));
    records
}

fn locator_from_record(record: FilesystemDirectoryLocatorRecord) -> FileSystemDirectoryLocator {
    FileSystemDirectoryLocator {
        path: record.path,
        locator: record.locator,
    }
}

fn file_locator_from_record(record: FilesystemFileLocatorRecord) -> FileSystemFileLocator {
    FileSystemFileLocator {
        path: record.path,
        locator: record.locator,
    }
}

pub(crate) fn preview_candidate_for_locator(
    candidate: &ImageFilesystemCandidate,
) -> Result<PreviewPartitionCandidate, FileServiceError> {
    let partition_index = candidate.partition_index.ok_or_else(|| {
        FileServiceError::other("Filesystem locator candidate has no partition index")
    })?;
    let filesystem_kind = match candidate.kind {
        ImageFilesystemKind::Ntfs => "NTFS",
        ImageFilesystemKind::Fat => "FAT",
        ImageFilesystemKind::Ext4 => "Ext4",
        ImageFilesystemKind::Xfs => "XFS",
        ImageFilesystemKind::Btrfs => "Btrfs",
        ImageFilesystemKind::BitLocker | ImageFilesystemKind::LvmPool => {
            return Err(FileServiceError::Unsupported(
                "Filesystem locator candidate is not directly readable".to_string(),
            ))
        }
    };
    Ok(PreviewPartitionCandidate {
        partition_index,
        filesystem_kind: filesystem_kind.to_string(),
        offset: candidate.offset,
        lvm_identity: candidate
            .lvm_identity
            .as_ref()
            .map(crate::file_service::viewer::preview_lvm_identity_from_datasource),
    })
}

pub(crate) fn derived_filesystem_locator_scope(
    catalog_fingerprint: &str,
    candidate: &PreviewPartitionCandidate,
) -> Result<String, FileServiceError> {
    if catalog_fingerprint.len() != 64
        || !catalog_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(FileServiceError::other(
            "Derived filesystem Catalog fingerprint is invalid",
        ));
    }
    let candidate_identity = serde_json::to_vec(candidate).map_err(|error| {
        FileServiceError::other(format!(
            "Derived filesystem locator candidate could not be serialized: {error}"
        ))
    })?;
    let mut hasher = Sha256::new();
    update_scope_field(&mut hasher, b"derived-rbd-filesystem-locator");
    update_scope_field(&mut hasher, &DERIVED_LOCATOR_SCOPE_VERSION.to_le_bytes());
    update_scope_field(
        &mut hasher,
        &crate::derived_source_catalog::CATALOG_MATERIALIZER_VERSION.to_le_bytes(),
    );
    update_scope_field(&mut hasher, catalog_fingerprint.as_bytes());
    update_scope_field(&mut hasher, &candidate_identity);
    Ok(hex::encode(hasher.finalize()))
}

fn update_scope_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[cfg(test)]
#[path = "../../tests/unit/file_service/filesystem_locators.rs"]
mod tests;
