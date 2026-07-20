use rusqlite::{params, Connection};

use super::{
    CephFsNamespaceProjection, CephFsNamespaceRepoError, CephFsNamespaceRepoResult,
    CephFsNamespaceWriteOutcome,
};

pub(super) fn replace(
    conn: &Connection,
    projection: &CephFsNamespaceProjection,
) -> CephFsNamespaceRepoResult<CephFsNamespaceWriteOutcome> {
    let existing = super::query::find(
        conn,
        &projection.manifest.filesystem_identity,
        &projection.manifest.data_source_id,
    )?;
    if let Some(existing) = existing {
        if existing.manifest.input_sha256 == projection.manifest.input_sha256 {
            if existing == *projection {
                return Ok(CephFsNamespaceWriteOutcome::Unchanged);
            }
            return Err(CephFsNamespaceRepoError::DeterminismConflict);
        }
    }
    let transaction = conn.unchecked_transaction()?;
    delete_existing(&transaction, projection)?;
    insert_projection(&transaction, projection)?;
    transaction.commit()?;
    let stored = super::query::find(
        conn,
        &projection.manifest.filesystem_identity,
        &projection.manifest.data_source_id,
    )?
    .ok_or(CephFsNamespaceRepoError::Invalid(
        "projection disappeared after write",
    ))?;
    if stored != *projection {
        return Err(CephFsNamespaceRepoError::Invalid(
            "stored projection differs from written projection",
        ));
    }
    Ok(CephFsNamespaceWriteOutcome::Replaced)
}

fn delete_existing(
    conn: &Connection,
    projection: &CephFsNamespaceProjection,
) -> CephFsNamespaceRepoResult<()> {
    let identity = &projection.manifest.filesystem_identity;
    let source = &projection.manifest.data_source_id;
    for table in [
        "ceph_fs_namespace_diagnostics",
        "ceph_fs_dentries",
        "ceph_fs_sparse_extents",
        "ceph_fs_file_layouts",
        "ceph_fs_inodes",
        "ceph_fs_namespace_manifests",
    ] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE filesystem_identity = ?1 AND data_source_id = ?2"),
            params![identity, source],
        )?;
    }
    Ok(())
}

fn insert_projection(
    conn: &Connection,
    projection: &CephFsNamespaceProjection,
) -> CephFsNamespaceRepoResult<()> {
    let manifest = &projection.manifest;
    conn.execute(
        "INSERT INTO ceph_fs_namespace_manifests (
             filesystem_identity, data_source_id, filesystem_id, fsmap_epoch,
             root_inode, input_sha256, projection_sha256, schema_version,
             decoder_profile, completeness, published, entry_count, inode_count,
             diagnostic_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            manifest.filesystem_identity,
            manifest.data_source_id,
            manifest.filesystem_id,
            manifest.fsmap_epoch,
            manifest.root_inode as i64,
            manifest.input_sha256,
            manifest.projection_sha256,
            manifest.schema_version,
            manifest.decoder_profile,
            manifest.completeness,
            manifest.published as i32,
            manifest.entry_count as i64,
            manifest.inode_count as i64,
            manifest.diagnostic_count as i64,
        ],
    )?;
    insert_inodes(conn, projection)?;
    insert_layouts(conn, projection)?;
    insert_dentries(conn, projection)?;
    insert_diagnostics(conn, projection)?;
    Ok(())
}

fn insert_inodes(
    conn: &Connection,
    projection: &CephFsNamespaceProjection,
) -> CephFsNamespaceRepoResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_fs_inodes (
             filesystem_identity, data_source_id, inode, mode, uid, gid, nlink,
             size, inode_kind, encoded_version, remaining_inode_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for inode in &projection.inodes {
        statement.execute(params![
            projection.manifest.filesystem_identity,
            projection.manifest.data_source_id,
            inode.inode as i64,
            inode.mode as i64,
            inode.uid as i64,
            inode.gid as i64,
            inode.nlink,
            inode.size as i64,
            inode.inode_kind,
            inode.encoded_version,
            inode.remaining_inode_bytes as i64,
        ])?;
    }
    Ok(())
}

fn insert_layouts(
    conn: &Connection,
    projection: &CephFsNamespaceProjection,
) -> CephFsNamespaceRepoResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_fs_file_layouts (
             filesystem_identity, data_source_id, inode, stripe_unit,
             stripe_count, object_size, pool_id, pool_namespace, inline_data
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for layout in &projection.layouts {
        statement.execute(params![
            projection.manifest.filesystem_identity,
            projection.manifest.data_source_id,
            layout.inode as i64,
            layout.stripe_unit as i64,
            layout.stripe_count as i64,
            layout.object_size as i64,
            layout.pool_id,
            layout.pool_namespace,
            layout.inline_data,
        ])?;
    }
    drop(statement);
    for layout in &projection.layouts {
        insert_sparse_extents(conn, projection, layout)?;
    }
    Ok(())
}

fn insert_sparse_extents(
    conn: &Connection,
    projection: &CephFsNamespaceProjection,
    layout: &super::CephFsFileLayoutRecord,
) -> CephFsNamespaceRepoResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_fs_sparse_extents (
             filesystem_identity, data_source_id, inode, offset, length,
             evidence_sha256, proof_sha256
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for extent in &layout.sparse_extents {
        statement.execute(params![
            projection.manifest.filesystem_identity,
            projection.manifest.data_source_id,
            layout.inode as i64,
            extent.offset as i64,
            extent.length as i64,
            extent.evidence_sha256,
            extent.proof_sha256,
        ])?;
    }
    Ok(())
}

fn insert_dentries(
    conn: &Connection,
    projection: &CephFsNamespaceProjection,
) -> CephFsNamespaceRepoResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_fs_dentries (
             filesystem_identity, data_source_id, entry_id, parent_entry_id,
             parent_inode, child_inode, fragment, name, path, entry_kind, mode,
             uid, gid, nlink, size, alternate_name
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )?;
    for dentry in &projection.dentries {
        statement.execute(params![
            projection.manifest.filesystem_identity,
            projection.manifest.data_source_id,
            dentry.entry_id,
            dentry.parent_entry_id,
            dentry.parent_inode as i64,
            dentry.child_inode as i64,
            dentry.fragment as i64,
            dentry.name,
            dentry.path,
            dentry.entry_kind,
            dentry.mode.map(|value| value as i64),
            dentry.uid.map(|value| value as i64),
            dentry.gid.map(|value| value as i64),
            dentry.nlink,
            dentry.size.map(|value| value as i64),
            dentry.alternate_name,
        ])?;
    }
    Ok(())
}

fn insert_diagnostics(
    conn: &Connection,
    projection: &CephFsNamespaceProjection,
) -> CephFsNamespaceRepoResult<()> {
    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_fs_namespace_diagnostics (
             filesystem_identity, data_source_id, diagnostic_ordinal,
             diagnostic_kind, parent_inode, child_inode, name, snap_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for diagnostic in &projection.diagnostics {
        statement.execute(params![
            projection.manifest.filesystem_identity,
            projection.manifest.data_source_id,
            diagnostic.diagnostic_ordinal as i64,
            diagnostic.diagnostic_kind,
            diagnostic.parent_inode as i64,
            diagnostic.child_inode as i64,
            diagnostic.name,
            diagnostic.snap_id.map(|value| value as i64),
        ])?;
    }
    Ok(())
}
