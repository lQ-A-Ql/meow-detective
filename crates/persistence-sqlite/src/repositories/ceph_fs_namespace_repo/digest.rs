use sha2::{Digest, Sha256};

use super::{
    CephFsDentryRecord, CephFsFileLayoutRecord, CephFsInodeRecord, CephFsNamespaceDiagnosticRecord,
    CephFsNamespaceManifest, CephFsNamespaceProjection,
};

pub fn cephfs_namespace_projection_sha256(
    manifest: &CephFsNamespaceManifest,
    inodes: &[CephFsInodeRecord],
    layouts: &[CephFsFileLayoutRecord],
    dentries: &[CephFsDentryRecord],
    diagnostics: &[CephFsNamespaceDiagnosticRecord],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-namespace/v1\0");
    field(&mut digest, &manifest.filesystem_identity);
    field(&mut digest, &manifest.data_source_id);
    digest.update(manifest.filesystem_id.to_le_bytes());
    digest.update(manifest.fsmap_epoch.to_le_bytes());
    digest.update(manifest.root_inode.to_le_bytes());
    digest.update((inodes.len() as u64).to_le_bytes());
    for inode in sorted(inodes, |value| value.inode) {
        digest_inode(&mut digest, inode);
    }
    digest.update((layouts.len() as u64).to_le_bytes());
    for layout in sorted(layouts, |value| value.inode) {
        digest_layout(&mut digest, layout);
    }
    digest.update((dentries.len() as u64).to_le_bytes());
    for dentry in sorted(dentries, |value| value.entry_id.clone()) {
        digest_dentry(&mut digest, dentry);
    }
    digest.update((diagnostics.len() as u64).to_le_bytes());
    for diagnostic in sorted(diagnostics, |value| value.diagnostic_ordinal) {
        digest_diagnostic(&mut digest, diagnostic);
    }
    hex::encode(digest.finalize())
}

pub fn cephfs_namespace_projection_digest(projection: &CephFsNamespaceProjection) -> String {
    cephfs_namespace_projection_sha256(
        &projection.manifest,
        &projection.inodes,
        &projection.layouts,
        &projection.dentries,
        &projection.diagnostics,
    )
}

fn sorted<T, K: Ord>(items: &[T], key: impl Fn(&T) -> K) -> Vec<&T> {
    let mut values = items.iter().collect::<Vec<_>>();
    values.sort_by_key(|item| key(item));
    values
}

fn field(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value.as_bytes());
}

fn optional_field(digest: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            digest.update([1]);
            field(digest, value);
        }
        None => digest.update([0]),
    }
}

fn optional_u64(digest: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            digest.update([1]);
            digest.update(value.to_le_bytes());
        }
        None => digest.update([0]),
    }
}

fn digest_inode(digest: &mut Sha256, inode: &CephFsInodeRecord) {
    digest.update(inode.inode.to_le_bytes());
    digest.update(inode.mode.to_le_bytes());
    digest.update(inode.uid.to_le_bytes());
    digest.update(inode.gid.to_le_bytes());
    digest.update(inode.nlink.to_le_bytes());
    digest.update(inode.size.to_le_bytes());
    field(digest, &inode.inode_kind);
    digest.update([inode.encoded_version]);
    digest.update(inode.remaining_inode_bytes.to_le_bytes());
}

fn digest_layout(digest: &mut Sha256, layout: &CephFsFileLayoutRecord) {
    digest.update(layout.inode.to_le_bytes());
    digest.update(layout.stripe_unit.to_le_bytes());
    digest.update(layout.stripe_count.to_le_bytes());
    digest.update(layout.object_size.to_le_bytes());
    digest.update(layout.pool_id.to_le_bytes());
    field(digest, &layout.pool_namespace);
    match &layout.inline_data {
        Some(bytes) => {
            digest.update([1]);
            digest.update((bytes.len() as u64).to_le_bytes());
            digest.update(bytes);
        }
        None => digest.update([0]),
    }
    digest.update((layout.sparse_extents.len() as u64).to_le_bytes());
    for extent in &layout.sparse_extents {
        digest.update(extent.offset.to_le_bytes());
        digest.update(extent.length.to_le_bytes());
        field(digest, &extent.evidence_sha256);
        field(digest, &extent.proof_sha256);
    }
}

fn digest_dentry(digest: &mut Sha256, dentry: &CephFsDentryRecord) {
    field(digest, &dentry.entry_id);
    optional_field(digest, dentry.parent_entry_id.as_deref());
    digest.update(dentry.parent_inode.to_le_bytes());
    digest.update(dentry.child_inode.to_le_bytes());
    digest.update(dentry.fragment.to_le_bytes());
    field(digest, &dentry.name);
    field(digest, &dentry.path);
    field(digest, &dentry.entry_kind);
    optional_u32(digest, dentry.mode);
    optional_u32(digest, dentry.uid);
    optional_u32(digest, dentry.gid);
    optional_i32(digest, dentry.nlink);
    optional_u64(digest, dentry.size);
    field(digest, &dentry.alternate_name);
}

fn digest_diagnostic(digest: &mut Sha256, diagnostic: &CephFsNamespaceDiagnosticRecord) {
    digest.update(diagnostic.diagnostic_ordinal.to_le_bytes());
    field(digest, &diagnostic.diagnostic_kind);
    digest.update(diagnostic.parent_inode.to_le_bytes());
    digest.update(diagnostic.child_inode.to_le_bytes());
    field(digest, &diagnostic.name);
    optional_u64(digest, diagnostic.snap_id);
}

fn optional_u32(digest: &mut Sha256, value: Option<u32>) {
    match value {
        Some(value) => {
            digest.update([1]);
            digest.update(value.to_le_bytes());
        }
        None => digest.update([0]),
    }
}

fn optional_i32(digest: &mut Sha256, value: Option<i32>) {
    match value {
        Some(value) => {
            digest.update([1]);
            digest.update(value.to_le_bytes());
        }
        None => digest.update([0]),
    }
}
