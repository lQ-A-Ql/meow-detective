use rusqlite::{params, Connection};

use super::{
    CephFsFileCatalogSummary, CephFsNamespaceManifest, CephFsNamespaceRepoError,
    CephFsNamespaceRepoResult,
};

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

    let mut statement = conn.prepare(
        "SELECT d.entry_id, d.parent_entry_id, d.path, d.name, d.entry_kind,
                d.size, f.id, f.parent_id, f.data_source_id, f.path,
                f.name, f.entry_type, f.size
         FROM ceph_fs_dentries AS d
         LEFT JOIN file_entries AS f
           ON f.id = d.entry_id
          AND f.data_source_id = d.data_source_id
         WHERE d.filesystem_identity = ?1
           AND d.data_source_id = ?2
         ORDER BY d.entry_id",
    )?;
    let rows = statement.query_map(
        params![manifest.filesystem_identity, manifest.data_source_id],
        map_catalog_row,
    )?;

    let mut dentry_count = 0u64;
    let mut directory_count = 0u64;
    let mut total_size = 0u64;
    for row in rows {
        let row = row?;
        dentry_count = dentry_count.saturating_add(1);
        let Some(file_id) = row.file_id.as_deref() else {
            return Err(CephFsNamespaceRepoError::Invalid(
                "CephFS dentry has no file catalog row",
            ));
        };
        let expected_name = if row.parent_entry_id.is_none() {
            expected_root_name
        } else {
            row.name.as_str()
        };
        let (expected_type, expected_size) = if row.entry_kind == "directory" {
            directory_count = directory_count.saturating_add(1);
            ("directory", None)
        } else {
            let size = row.size.ok_or(CephFsNamespaceRepoError::Invalid(
                "CephFS non-directory dentry has no size",
            ))?;
            let size = u64::try_from(size)
                .map_err(|_| CephFsNamespaceRepoError::Invalid("CephFS dentry size is negative"))?;
            total_size = total_size.saturating_add(size);
            (
                "file",
                Some(i64::try_from(size).map_err(|_| {
                    CephFsNamespaceRepoError::Invalid("CephFS dentry size overflows SQLite")
                })?),
            )
        };
        if file_id != row.entry_id
            || row.file_parent_id != row.parent_entry_id
            || row.file_data_source_id.as_deref() != Some(manifest.data_source_id.as_str())
            || row.file_path.as_deref() != Some(row.path.as_str())
            || row.file_name.as_deref() != Some(expected_name)
            || row.file_entry_type.as_deref() != Some(expected_type)
            || row.file_size != expected_size
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "CephFS file catalog does not match namespace projection",
            ));
        }
    }

    let file_count = count_file_rows(conn, &manifest.data_source_id)?;
    let all_file_count = count_all_file_rows(conn)?;
    if dentry_count != manifest.entry_count
        || file_count != manifest.entry_count
        || dentry_count != file_count
        || all_file_count != file_count
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

fn count_file_rows(conn: &Connection, data_source_id: &str) -> CephFsNamespaceRepoResult<u64> {
    let count = conn.query_row(
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
        params![data_source_id],
        |row| row.get::<_, i64>(0),
    )?;
    u64::try_from(count)
        .map_err(|_| CephFsNamespaceRepoError::Invalid("CephFS file catalog count is negative"))
}

fn count_all_file_rows(conn: &Connection) -> CephFsNamespaceRepoResult<u64> {
    let count = conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| {
        row.get::<_, i64>(0)
    })?;
    u64::try_from(count)
        .map_err(|_| CephFsNamespaceRepoError::Invalid("CephFS file catalog count is negative"))
}

struct CatalogRow {
    entry_id: String,
    parent_entry_id: Option<String>,
    path: String,
    name: String,
    entry_kind: String,
    size: Option<i64>,
    file_id: Option<String>,
    file_parent_id: Option<String>,
    file_data_source_id: Option<String>,
    file_path: Option<String>,
    file_name: Option<String>,
    file_entry_type: Option<String>,
    file_size: Option<i64>,
}

fn map_catalog_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogRow> {
    Ok(CatalogRow {
        entry_id: row.get(0)?,
        parent_entry_id: row.get(1)?,
        path: row.get(2)?,
        name: row.get(3)?,
        entry_kind: row.get(4)?,
        size: row.get(5)?,
        file_id: row.get(6)?,
        file_parent_id: row.get(7)?,
        file_data_source_id: row.get(8)?,
        file_path: row.get(9)?,
        file_name: row.get(10)?,
        file_entry_type: row.get(11)?,
        file_size: row.get(12)?,
    })
}
