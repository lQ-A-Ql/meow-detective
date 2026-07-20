use super::{
    CephFsDentryRecord, CephFsFileLayoutRecord, CephFsFileLocatorRecord, CephFsInodeRecord,
    CephFsNamespaceDiagnosticRecord, CephFsNamespaceManifest, CephFsNamespaceProjection,
    CephFsNamespaceRepoResult,
};
use rusqlite::{params, Connection, OptionalExtension};

pub(super) fn find(
    conn: &Connection,
    filesystem_identity: &str,
    data_source_id: &str,
) -> CephFsNamespaceRepoResult<Option<CephFsNamespaceProjection>> {
    let Some(manifest) = find_manifest(conn, filesystem_identity, data_source_id)? else {
        return Ok(None);
    };
    let inodes = load_inodes(conn, filesystem_identity, data_source_id)?;
    let layouts = load_layouts(conn, filesystem_identity, data_source_id)?;
    let dentries = load_dentries(conn, filesystem_identity, data_source_id)?;
    let diagnostics = load_diagnostics(conn, filesystem_identity, data_source_id)?;
    Ok(Some(CephFsNamespaceProjection {
        manifest,
        inodes,
        layouts,
        dentries,
        diagnostics,
    }))
}

pub(super) fn find_manifest(
    conn: &Connection,
    filesystem_identity: &str,
    data_source_id: &str,
) -> CephFsNamespaceRepoResult<Option<CephFsNamespaceManifest>> {
    conn.query_row(
        "SELECT filesystem_identity, data_source_id, filesystem_id, fsmap_epoch,
                root_inode, input_sha256, projection_sha256, schema_version,
                decoder_profile, completeness, published, entry_count,
                inode_count, diagnostic_count
         FROM ceph_fs_namespace_manifests
         WHERE filesystem_identity = ?1 AND data_source_id = ?2",
        params![filesystem_identity, data_source_id],
        map_manifest,
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn find_file_locator(
    conn: &Connection,
    data_source_id: &str,
    entry_id: &str,
) -> CephFsNamespaceRepoResult<Option<CephFsFileLocatorRecord>> {
    let locator = conn
        .query_row(
            "SELECT manifest.filesystem_identity, manifest.data_source_id,
                manifest.filesystem_id, manifest.fsmap_epoch,
                manifest.projection_sha256, manifest.schema_version,
                manifest.decoder_profile, dentry.entry_id, dentry.child_inode,
                dentry.entry_kind, inode.size, layout.stripe_unit,
                layout.stripe_count, layout.object_size, layout.pool_id,
                layout.pool_namespace, layout.inline_data
         FROM ceph_fs_namespace_manifests AS manifest
         JOIN ceph_fs_dentries AS dentry
           ON dentry.filesystem_identity = manifest.filesystem_identity
          AND dentry.data_source_id = manifest.data_source_id
         JOIN ceph_fs_inodes AS inode
           ON inode.filesystem_identity = dentry.filesystem_identity
          AND inode.data_source_id = dentry.data_source_id
          AND inode.inode = dentry.child_inode
         JOIN ceph_fs_file_layouts AS layout
           ON layout.filesystem_identity = inode.filesystem_identity
          AND layout.data_source_id = inode.data_source_id
          AND layout.inode = inode.inode
         WHERE manifest.data_source_id = ?1
           AND manifest.completeness = 'closed'
           AND manifest.published = 1
           AND dentry.entry_id = ?2",
            params![data_source_id, entry_id],
            map_file_locator,
        )
        .optional()
        .map_err(super::CephFsNamespaceRepoError::from)?;
    let Some(mut locator) = locator else {
        return Ok(None);
    };
    locator.sparse_extents = load_sparse_extents(
        conn,
        &locator.filesystem_identity,
        data_source_id,
        locator.inode,
    )?;
    Ok(Some(locator))
}

fn load_inodes(
    conn: &Connection,
    filesystem_identity: &str,
    data_source_id: &str,
) -> CephFsNamespaceRepoResult<Vec<CephFsInodeRecord>> {
    let mut statement = conn.prepare(
        "SELECT inode, mode, uid, gid, nlink, size, inode_kind,
                encoded_version, remaining_inode_bytes
         FROM ceph_fs_inodes
         WHERE filesystem_identity = ?1 AND data_source_id = ?2
         ORDER BY inode",
    )?;
    let rows = statement.query_map(params![filesystem_identity, data_source_id], |row| {
        Ok(CephFsInodeRecord {
            inode: row.get::<_, i64>(0)?.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Integer,
                    "invalid inode".into(),
                )
            })?,
            mode: row.get::<_, i64>(1)?.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Integer,
                    "invalid mode".into(),
                )
            })?,
            uid: row.get::<_, i64>(2)?.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Integer,
                    "invalid uid".into(),
                )
            })?,
            gid: row.get::<_, i64>(3)?.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Integer,
                    "invalid gid".into(),
                )
            })?,
            nlink: row.get(4)?,
            size: row.get::<_, i64>(5)?.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Integer,
                    "invalid size".into(),
                )
            })?,
            inode_kind: row.get(6)?,
            encoded_version: row.get::<_, i64>(7)?.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Integer,
                    "invalid version".into(),
                )
            })?,
            remaining_inode_bytes: row.get::<_, i64>(8)?.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Integer,
                    "invalid remaining bytes".into(),
                )
            })?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn load_layouts(
    conn: &Connection,
    filesystem_identity: &str,
    data_source_id: &str,
) -> CephFsNamespaceRepoResult<Vec<CephFsFileLayoutRecord>> {
    let mut statement = conn.prepare(
        "SELECT inode, stripe_unit, stripe_count, object_size, pool_id,
                pool_namespace, inline_data
         FROM ceph_fs_file_layouts
         WHERE filesystem_identity = ?1 AND data_source_id = ?2
         ORDER BY inode",
    )?;
    let rows = statement.query_map(params![filesystem_identity, data_source_id], |row| {
        Ok(CephFsFileLayoutRecord {
            inode: row
                .get::<_, i64>(0)?
                .try_into()
                .map_err(|_| invalid_integer(0, "inode"))?,
            stripe_unit: row
                .get::<_, i64>(1)?
                .try_into()
                .map_err(|_| invalid_integer(1, "stripe unit"))?,
            stripe_count: row
                .get::<_, i64>(2)?
                .try_into()
                .map_err(|_| invalid_integer(2, "stripe count"))?,
            object_size: row
                .get::<_, i64>(3)?
                .try_into()
                .map_err(|_| invalid_integer(3, "object size"))?,
            pool_id: row.get(4)?,
            pool_namespace: row.get(5)?,
            inline_data: row.get(6)?,
            sparse_extents: Vec::new(),
        })
    })?;
    let mut layouts = rows.collect::<Result<Vec<_>, _>>()?;
    for layout in &mut layouts {
        layout.sparse_extents =
            load_sparse_extents(conn, filesystem_identity, data_source_id, layout.inode)?;
    }
    Ok(layouts)
}

fn load_sparse_extents(
    conn: &Connection,
    filesystem_identity: &str,
    data_source_id: &str,
    inode: u64,
) -> CephFsNamespaceRepoResult<Vec<super::CephFsSparseExtentRecord>> {
    let mut statement = conn.prepare(
        "SELECT offset, length, evidence_sha256, proof_sha256
         FROM ceph_fs_sparse_extents
         WHERE filesystem_identity = ?1 AND data_source_id = ?2 AND inode = ?3
         ORDER BY offset",
    )?;
    let rows = statement.query_map(
        params![filesystem_identity, data_source_id, inode as i64],
        |row| {
            Ok(super::CephFsSparseExtentRecord {
                offset: row
                    .get::<_, i64>(0)?
                    .try_into()
                    .map_err(|_| invalid_integer(0, "sparse offset"))?,
                length: row
                    .get::<_, i64>(1)?
                    .try_into()
                    .map_err(|_| invalid_integer(1, "sparse length"))?,
                evidence_sha256: row.get(2)?,
                proof_sha256: row.get(3)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn load_dentries(
    conn: &Connection,
    filesystem_identity: &str,
    data_source_id: &str,
) -> CephFsNamespaceRepoResult<Vec<CephFsDentryRecord>> {
    let mut statement = conn.prepare(
        "SELECT entry_id, parent_entry_id, parent_inode, child_inode, fragment,
                name, path, entry_kind, mode, uid, gid, nlink, size,
                alternate_name
         FROM ceph_fs_dentries
         WHERE filesystem_identity = ?1 AND data_source_id = ?2
         ORDER BY path, entry_id",
    )?;
    let rows = statement.query_map(params![filesystem_identity, data_source_id], |row| {
        Ok(CephFsDentryRecord {
            entry_id: row.get(0)?,
            parent_entry_id: row.get(1)?,
            parent_inode: row
                .get::<_, i64>(2)?
                .try_into()
                .map_err(|_| invalid_integer(2, "parent inode"))?,
            child_inode: row
                .get::<_, i64>(3)?
                .try_into()
                .map_err(|_| invalid_integer(3, "child inode"))?,
            fragment: row
                .get::<_, i64>(4)?
                .try_into()
                .map_err(|_| invalid_integer(4, "fragment"))?,
            name: row.get(5)?,
            path: row.get(6)?,
            entry_kind: row.get(7)?,
            mode: row
                .get::<_, Option<i64>>(8)?
                .map(|value| value.try_into().map_err(|_| invalid_integer(8, "mode")))
                .transpose()?,
            uid: row
                .get::<_, Option<i64>>(9)?
                .map(|value| value.try_into().map_err(|_| invalid_integer(9, "uid")))
                .transpose()?,
            gid: row
                .get::<_, Option<i64>>(10)?
                .map(|value| value.try_into().map_err(|_| invalid_integer(10, "gid")))
                .transpose()?,
            nlink: row.get(11)?,
            size: row
                .get::<_, Option<i64>>(12)?
                .map(|value| value.try_into().map_err(|_| invalid_integer(12, "size")))
                .transpose()?,
            alternate_name: row.get(13)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn load_diagnostics(
    conn: &Connection,
    filesystem_identity: &str,
    data_source_id: &str,
) -> CephFsNamespaceRepoResult<Vec<CephFsNamespaceDiagnosticRecord>> {
    let mut statement = conn.prepare(
        "SELECT diagnostic_ordinal, diagnostic_kind, parent_inode, child_inode,
                name, snap_id
         FROM ceph_fs_namespace_diagnostics
         WHERE filesystem_identity = ?1 AND data_source_id = ?2
         ORDER BY diagnostic_ordinal",
    )?;
    let rows = statement.query_map(params![filesystem_identity, data_source_id], |row| {
        Ok(CephFsNamespaceDiagnosticRecord {
            diagnostic_ordinal: row
                .get::<_, i64>(0)?
                .try_into()
                .map_err(|_| invalid_integer(0, "ordinal"))?,
            diagnostic_kind: row.get(1)?,
            parent_inode: row
                .get::<_, i64>(2)?
                .try_into()
                .map_err(|_| invalid_integer(2, "parent inode"))?,
            child_inode: row
                .get::<_, i64>(3)?
                .try_into()
                .map_err(|_| invalid_integer(3, "child inode"))?,
            name: row.get(4)?,
            snap_id: row
                .get::<_, Option<i64>>(5)?
                .map(|value| value.try_into().map_err(|_| invalid_integer(5, "snapshot")))
                .transpose()?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn map_manifest(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephFsNamespaceManifest> {
    Ok(CephFsNamespaceManifest {
        filesystem_identity: row.get(0)?,
        data_source_id: row.get(1)?,
        filesystem_id: row.get(2)?,
        fsmap_epoch: row
            .get::<_, i64>(3)?
            .try_into()
            .map_err(|_| invalid_integer(3, "epoch"))?,
        root_inode: row
            .get::<_, i64>(4)?
            .try_into()
            .map_err(|_| invalid_integer(4, "root inode"))?,
        input_sha256: row.get(5)?,
        projection_sha256: row.get(6)?,
        schema_version: row
            .get::<_, i64>(7)?
            .try_into()
            .map_err(|_| invalid_integer(7, "schema"))?,
        decoder_profile: row.get(8)?,
        completeness: row.get(9)?,
        published: row.get::<_, i64>(10)? != 0,
        entry_count: row
            .get::<_, i64>(11)?
            .try_into()
            .map_err(|_| invalid_integer(11, "entry count"))?,
        inode_count: row
            .get::<_, i64>(12)?
            .try_into()
            .map_err(|_| invalid_integer(12, "inode count"))?,
        diagnostic_count: row
            .get::<_, i64>(13)?
            .try_into()
            .map_err(|_| invalid_integer(13, "diagnostic count"))?,
    })
}

fn map_file_locator(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephFsFileLocatorRecord> {
    Ok(CephFsFileLocatorRecord {
        filesystem_identity: row.get(0)?,
        data_source_id: row.get(1)?,
        filesystem_id: row.get(2)?,
        fsmap_epoch: row
            .get::<_, i64>(3)?
            .try_into()
            .map_err(|_| invalid_integer(3, "epoch"))?,
        projection_sha256: row.get(4)?,
        schema_version: row
            .get::<_, i64>(5)?
            .try_into()
            .map_err(|_| invalid_integer(5, "schema"))?,
        decoder_profile: row.get(6)?,
        entry_id: row.get(7)?,
        inode: row
            .get::<_, i64>(8)?
            .try_into()
            .map_err(|_| invalid_integer(8, "inode"))?,
        entry_kind: row.get(9)?,
        size: row
            .get::<_, i64>(10)?
            .try_into()
            .map_err(|_| invalid_integer(10, "size"))?,
        stripe_unit: row
            .get::<_, i64>(11)?
            .try_into()
            .map_err(|_| invalid_integer(11, "stripe unit"))?,
        stripe_count: row
            .get::<_, i64>(12)?
            .try_into()
            .map_err(|_| invalid_integer(12, "stripe count"))?,
        object_size: row
            .get::<_, i64>(13)?
            .try_into()
            .map_err(|_| invalid_integer(13, "object size"))?,
        pool_id: row.get(14)?,
        pool_namespace: row.get(15)?,
        inline_data: row.get(16)?,
        sparse_extents: Vec::new(),
    })
}

fn invalid_integer(index: usize, label: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Integer,
        format!("invalid {label}").into(),
    )
}
