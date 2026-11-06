use super::{
    CephFsFileCatalogSummary, CephFsNamespaceManifest, CephFsNamespaceRepoError,
    CephFsNamespaceRepoResult,
};
use persistence_fjall::{CatalogEntryType, CatalogRecordStore};
use rusqlite::{params, Connection};

pub(super) fn verify_published_catalog(
    conn: &Connection,
    manifest: &CephFsNamespaceManifest,
    expected_root_name: &str,
) -> CephFsNamespaceRepoResult<CephFsFileCatalogSummary> {
    if expected_root_name.trim().is_empty() || expected_root_name.contains('\0') {
        return Err(CephFsNamespaceRepoError::Invalid(
            "CephFS source name is invalid",
        ));
    }
    if crate::source_fjall::source_id(conn)?.as_deref() != Some(&manifest.data_source_id) {
        return Err(CephFsNamespaceRepoError::Invalid(
            "CephFS catalog owner is invalid",
        ));
    }
    let store = crate::source_fjall::open_store(conn)?.ok_or(CephFsNamespaceRepoError::Invalid(
        "CephFS native catalog is missing",
    ))?;
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut statement = conn.prepare(
        "SELECT entry_id, parent_entry_id, path, name, entry_kind, size
         FROM ceph_fs_dentries WHERE filesystem_identity=?1 AND data_source_id=?2 ORDER BY entry_id"
    )?;
    let rows = statement.query_map(
        params![manifest.filesystem_identity, manifest.data_source_id],
        |row| {
            Ok(CatalogRow {
                entry_id: row.get(0)?,
                parent_entry_id: row.get(1)?,
                path: row.get(2)?,
                name: row.get(3)?,
                entry_kind: row.get(4)?,
                size: row.get(5)?,
            })
        },
    )?;
    let mut dentry_count = 0u64;
    let mut directory_count = 0u64;
    let mut total_size = 0u64;
    for row in rows {
        crate::read_operation::ensure_active()?;
        let row = row?;
        let record = catalog
            .get(&snapshot, &manifest.data_source_id, &row.entry_id)
            .map_err(native_error)?
            .ok_or(CephFsNamespaceRepoError::Invalid(
                "CephFS dentry has no file catalog row",
            ))?;
        dentry_count += 1;
        let expected_name = if row.parent_entry_id.is_none() {
            expected_root_name
        } else {
            &row.name
        };
        let (expected_type, expected_size) = if row.entry_kind == "directory" {
            directory_count += 1;
            (CatalogEntryType::Directory, None)
        } else {
            let size = row.size.and_then(|size| u64::try_from(size).ok()).ok_or(
                CephFsNamespaceRepoError::Invalid("CephFS dentry size is invalid"),
            )?;
            total_size = total_size
                .checked_add(size)
                .ok_or(CephFsNamespaceRepoError::Invalid(
                    "CephFS catalog size overflowed",
                ))?;
            (CatalogEntryType::File, Some(size))
        };
        if record.id != row.entry_id
            || record.parent_id != row.parent_entry_id
            || record.data_source_id != manifest.data_source_id
            || record.path != row.path
            || record.name != expected_name
            || record.entry_type != expected_type
            || record.size != expected_size
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "CephFS file catalog does not match namespace projection",
            ));
        }
    }
    let file_count = catalog
        .count(&snapshot, Some(&manifest.data_source_id))
        .map_err(native_error)?;
    let all_count = catalog.count(&snapshot, None).map_err(native_error)?;
    if dentry_count != manifest.entry_count
        || file_count != manifest.entry_count
        || all_count != file_count
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "CephFS file catalog count or source scope is invalid",
        ));
    }
    Ok(CephFsFileCatalogSummary {
        file_count,
        directory_count,
        total_size,
    })
}

fn native_error(error: persistence_fjall::FjallStorageError) -> CephFsNamespaceRepoError {
    crate::DbError::System(error.to_string()).into()
}

struct CatalogRow {
    entry_id: String,
    parent_entry_id: Option<String>,
    path: String,
    name: String,
    entry_kind: String,
    size: Option<i64>,
}
