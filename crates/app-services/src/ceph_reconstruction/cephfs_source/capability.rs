use std::collections::BTreeMap;

use persistence_sqlite::repositories::ceph_fs_namespace_repo::{
    CephFsFileLayoutRecord, CephFsNamespaceProjection,
};

use super::CephFsSourceCapability;

pub(super) fn derive_source_capability(
    assembly: &ceph_wire::CephFsNamespaceAssembly,
    projection: &CephFsNamespaceProjection,
) -> CephFsSourceCapability {
    if !assembly.is_complete() {
        return CephFsSourceCapability::MetadataOnly;
    }
    let layouts = projection
        .layouts
        .iter()
        .map(|layout| (layout.inode, layout))
        .collect::<BTreeMap<_, _>>();
    let content_is_bounded = projection.inodes.iter().all(|inode| {
        inode.inode_kind == "directory"
            || inode.size == 0
            || layouts
                .get(&inode.inode)
                .is_some_and(|layout| layout_has_closed_content(layout, inode.size))
    });
    if content_is_bounded {
        CephFsSourceCapability::BoundedPreview
    } else {
        CephFsSourceCapability::MetadataBrowseable
    }
}

fn layout_has_closed_content(layout: &CephFsFileLayoutRecord, file_size: u64) -> bool {
    if layout
        .inline_data
        .as_ref()
        .is_some_and(|bytes| bytes.len() as u64 == file_size)
    {
        return true;
    }
    let mut cursor = 0;
    for extent in &layout.sparse_extents {
        if extent.offset > cursor {
            return false;
        }
        cursor = cursor.max(extent.offset.saturating_add(extent.length));
        if cursor >= file_size {
            return true;
        }
    }
    false
}
