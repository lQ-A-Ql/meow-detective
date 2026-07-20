use std::collections::{BTreeMap, BTreeSet};

use super::{
    cephfs_namespace_projection_sha256, CephFsNamespaceProjection, CephFsNamespaceRepoError,
    CephFsNamespaceRepoResult, CEPHFS_NAMESPACE_DECODER_PROFILE, CEPHFS_NAMESPACE_SCHEMA_VERSION,
};

pub(super) fn validate_projection(
    projection: &CephFsNamespaceProjection,
) -> CephFsNamespaceRepoResult<()> {
    let manifest = &projection.manifest;
    validate_text(&manifest.filesystem_identity, "filesystem identity")?;
    validate_text(&manifest.data_source_id, "data source id")?;
    validate_sha256(&manifest.input_sha256, "input digest")?;
    validate_sha256(&manifest.projection_sha256, "projection digest")?;
    if manifest.filesystem_id < 0
        || manifest.fsmap_epoch == 0
        || manifest.root_inode == 0
        || manifest.schema_version != CEPHFS_NAMESPACE_SCHEMA_VERSION
        || manifest.decoder_profile != CEPHFS_NAMESPACE_DECODER_PROFILE
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "manifest identity or schema is invalid",
        ));
    }
    if !matches!(manifest.completeness.as_str(), "closed" | "incomplete")
        || manifest.published != (manifest.completeness == "closed")
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "publication state is inconsistent with completeness",
        ));
    }
    if manifest.entry_count != projection.dentries.len() as u64
        || manifest.inode_count != projection.inodes.len() as u64
        || manifest.diagnostic_count != projection.diagnostics.len() as u64
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "manifest counts do not match projection rows",
        ));
    }
    validate_inodes(projection)?;
    validate_layouts(projection)?;
    validate_dentries(projection)?;
    validate_diagnostics(projection)?;
    let digest = cephfs_namespace_projection_sha256(
        manifest,
        &projection.inodes,
        &projection.layouts,
        &projection.dentries,
        &projection.diagnostics,
    );
    if digest != manifest.projection_sha256 {
        return Err(CephFsNamespaceRepoError::Invalid(
            "projection digest does not match canonical rows",
        ));
    }
    Ok(())
}

fn validate_inodes(projection: &CephFsNamespaceProjection) -> CephFsNamespaceRepoResult<()> {
    let mut inodes = BTreeSet::new();
    for inode in &projection.inodes {
        if inode.inode == 0
            || inode.encoded_version == 0
            || !matches!(
                inode.inode_kind.as_str(),
                "file" | "directory" | "symlink" | "other"
            )
            || !inodes.insert(inode.inode)
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "inode identity or kind is invalid",
            ));
        }
    }
    if !inodes.contains(&projection.manifest.root_inode) {
        return Err(CephFsNamespaceRepoError::Invalid(
            "root inode is missing from inode projection",
        ));
    }
    Ok(())
}

fn validate_layouts(projection: &CephFsNamespaceProjection) -> CephFsNamespaceRepoResult<()> {
    let inodes = projection
        .inodes
        .iter()
        .map(|inode| (inode.inode, inode))
        .collect::<BTreeMap<_, _>>();
    let mut layouts = BTreeSet::new();
    for layout in &projection.layouts {
        let Some(inode) = inodes.get(&layout.inode) else {
            return Err(CephFsNamespaceRepoError::Invalid(
                "layout references an unknown inode",
            ));
        };
        let empty_layout = layout.stripe_unit == 0
            && layout.stripe_count == 0
            && layout.object_size == 0
            && layout.pool_id == -1
            && layout.pool_namespace.is_empty();
        if !layouts.insert(layout.inode)
            || layout.pool_id < -1
            || layout.pool_namespace.contains('\0')
            || layout
                .inline_data
                .as_ref()
                .is_some_and(|bytes| bytes.len() > 65_536 || bytes.len() as u64 != inode.size)
            || ((layout.stripe_unit == 0 || layout.stripe_count == 0 || layout.object_size == 0)
                && !empty_layout)
            || (layout.stripe_unit > 0
                && (layout.object_size < layout.stripe_unit
                    || !layout.object_size.is_multiple_of(layout.stripe_unit)))
            || (inode.size > 0
                && empty_layout
                && layout.inline_data.is_none()
                && !covers_sparse_range(&layout.sparse_extents, 0, inode.size))
            || (inode.inode_kind == "directory" && layout.inline_data.is_some())
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "file layout is invalid or inconsistent with inode size",
            ));
        }
        validate_sparse_extents(layout, inode.size)?;
    }
    if layouts.len() != inodes.len() {
        return Err(CephFsNamespaceRepoError::Invalid(
            "every resolved inode must have exactly one layout",
        ));
    }
    Ok(())
}

fn validate_sparse_extents(
    layout: &super::CephFsFileLayoutRecord,
    file_size: u64,
) -> CephFsNamespaceRepoResult<()> {
    if layout.inline_data.is_some() && !layout.sparse_extents.is_empty() {
        return Err(CephFsNamespaceRepoError::Invalid(
            "inline data cannot have sparse extents",
        ));
    }
    let mut previous_end = 0u64;
    for (index, extent) in layout.sparse_extents.iter().enumerate() {
        let end =
            extent
                .offset
                .checked_add(extent.length)
                .ok_or(CephFsNamespaceRepoError::Invalid(
                    "sparse extent range overflows",
                ))?;
        if extent.length == 0
            || end > file_size
            || (index > 0 && extent.offset < previous_end)
            || !canonical_sha256(&extent.evidence_sha256)
            || !canonical_sha256(&extent.proof_sha256)
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "sparse extent proof is invalid",
            ));
        }
        previous_end = end;
    }
    Ok(())
}

fn covers_sparse_range(extents: &[super::CephFsSparseExtentRecord], offset: u64, end: u64) -> bool {
    if offset >= end {
        return true;
    }
    let mut cursor = offset;
    for extent in extents {
        if extent.offset > cursor {
            return false;
        }
        cursor = cursor.max(extent.offset.saturating_add(extent.length));
        if cursor >= end {
            return true;
        }
    }
    false
}

fn validate_dentries(projection: &CephFsNamespaceProjection) -> CephFsNamespaceRepoResult<()> {
    let inodes = projection
        .inodes
        .iter()
        .map(|inode| (inode.inode, inode))
        .collect::<BTreeMap<_, _>>();
    let mut by_id = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for entry in &projection.dentries {
        validate_dentry_shape(entry, projection.manifest.root_inode, &inodes)?;
        if by_id.insert(entry.entry_id.as_str(), entry).is_some()
            || !names.insert((entry.parent_inode, entry.name.clone()))
            || !paths.insert(entry.path.as_str())
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "dentry identity, name, or path is duplicated",
            ));
        }
    }
    validate_parent_graph(&by_id)?;
    validate_link_counts(projection, &inodes)?;
    if projection.manifest.published
        && projection
            .dentries
            .iter()
            .any(|entry| matches!(entry.entry_kind.as_str(), "remote" | "other"))
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "published namespace contains unresolved dentries",
        ));
    }
    Ok(())
}

fn validate_dentry_shape(
    entry: &super::CephFsDentryRecord,
    root_inode: u64,
    inodes: &BTreeMap<u64, &super::CephFsInodeRecord>,
) -> CephFsNamespaceRepoResult<()> {
    if entry.entry_id.trim().is_empty()
        || entry.entry_id.contains('\0')
        || entry.name.is_empty()
        || entry.name.contains('\0')
        || entry.path.is_empty()
        || !entry.path.starts_with('/')
        || entry.path.contains('\0')
        || entry.path.contains("//")
        || entry.path.split('/').any(|part| matches!(part, "." | ".."))
        || !matches!(
            entry.entry_kind.as_str(),
            "file" | "directory" | "symlink" | "remote" | "other"
        )
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "dentry identity, name, path, or kind is invalid",
        ));
    }
    if entry.parent_entry_id.is_none() {
        if entry.parent_inode != 0
            || entry.child_inode != root_inode
            || entry.name != "/"
            || entry.path != "/"
            || entry.entry_kind != "directory"
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "namespace root entry is invalid",
            ));
        }
    } else if entry.parent_inode == 0
        || entry.name.contains('/')
        || matches!(entry.name.as_str(), "." | "..")
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "non-root dentry name or parent is invalid",
        ));
    }
    if entry.entry_kind == "remote" {
        if entry.mode.is_some()
            || entry.uid.is_some()
            || entry.gid.is_some()
            || entry.nlink.is_some()
            || entry.size.is_some()
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "remote dentry contains unproven inode metadata",
            ));
        }
        return Ok(());
    }
    let inode = inodes
        .get(&entry.child_inode)
        .ok_or(CephFsNamespaceRepoError::Invalid(
            "dentry references an unknown inode",
        ))?;
    if entry.entry_kind != inode.inode_kind
        || entry.mode != Some(inode.mode)
        || entry.uid != Some(inode.uid)
        || entry.gid != Some(inode.gid)
        || entry.nlink != Some(inode.nlink)
        || entry.size != Some(inode.size)
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "dentry metadata does not match its inode",
        ));
    }
    Ok(())
}

fn validate_parent_graph(
    by_id: &BTreeMap<&str, &super::CephFsDentryRecord>,
) -> CephFsNamespaceRepoResult<()> {
    if by_id
        .values()
        .filter(|entry| entry.parent_entry_id.is_none())
        .count()
        != 1
    {
        return Err(CephFsNamespaceRepoError::Invalid(
            "namespace must contain exactly one root entry",
        ));
    }
    for entry in by_id
        .values()
        .filter(|entry| entry.parent_entry_id.is_some())
    {
        let parent_id = entry.parent_entry_id.as_deref().unwrap_or_default();
        let parent = by_id
            .get(parent_id)
            .ok_or(CephFsNamespaceRepoError::Invalid(
                "dentry parent reference is invalid",
            ))?;
        if parent.child_inode != entry.parent_inode
            || parent.entry_kind != "directory"
            || entry.path != join_path(&parent.path, &entry.name)
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "dentry parent inode, kind, or derived path is inconsistent",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut current = *entry;
        while let Some(parent_id) = current.parent_entry_id.as_deref() {
            if !seen.insert(current.entry_id.as_str()) {
                return Err(CephFsNamespaceRepoError::Invalid(
                    "namespace contains a parent cycle",
                ));
            }
            current = by_id
                .get(parent_id)
                .copied()
                .ok_or(CephFsNamespaceRepoError::Invalid(
                    "namespace parent chain is not closed",
                ))?;
        }
    }
    Ok(())
}

fn validate_link_counts(
    projection: &CephFsNamespaceProjection,
    inodes: &BTreeMap<u64, &super::CephFsInodeRecord>,
) -> CephFsNamespaceRepoResult<()> {
    let mut observed = BTreeMap::<u64, u64>::new();
    for entry in &projection.dentries {
        if entry.entry_kind != "remote" {
            *observed.entry(entry.child_inode).or_default() += 1;
        }
    }
    for (inode_id, inode) in inodes {
        let count = observed.get(inode_id).copied().unwrap_or(0);
        let nlink = u64::try_from(inode.nlink)
            .map_err(|_| CephFsNamespaceRepoError::Invalid("inode link count must be positive"))?;
        if nlink == 0
            || count == 0
            || count > nlink
            || (projection.manifest.published && inode.inode_kind != "directory" && count != nlink)
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "inode link count does not match namespace references",
            ));
        }
    }
    Ok(())
}

fn join_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn validate_diagnostics(projection: &CephFsNamespaceProjection) -> CephFsNamespaceRepoResult<()> {
    for (ordinal, diagnostic) in projection.diagnostics.iter().enumerate() {
        if diagnostic.diagnostic_ordinal != ordinal as u64
            || !matches!(
                diagnostic.diagnostic_kind.as_str(),
                "snapshot_skipped" | "duplicate" | "orphan" | "cycle"
            )
            || diagnostic.name.contains('\0')
            || (projection.manifest.published && diagnostic.diagnostic_kind != "snapshot_skipped")
        {
            return Err(CephFsNamespaceRepoError::Invalid(
                "namespace diagnostic is invalid or out of order",
            ));
        }
    }
    Ok(())
}

fn validate_text(value: &str, field: &'static str) -> CephFsNamespaceRepoResult<()> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(CephFsNamespaceRepoError::Invalid(field));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &'static str) -> CephFsNamespaceRepoResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CephFsNamespaceRepoError::Invalid(field));
    }
    Ok(())
}

fn canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
