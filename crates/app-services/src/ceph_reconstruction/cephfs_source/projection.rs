use std::collections::BTreeMap;

use ceph_wire::{
    CephFsNamespaceAssembly, CephFsNamespaceDiagnostic, CephFsNamespaceEntry,
    CephFsNamespaceEntryKind, CephFsNamespaceGraph,
};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::ceph_fs_namespace_repo::{
    cephfs_namespace_projection_digest, CephFsDentryRecord, CephFsFileLayoutRecord,
    CephFsInodeRecord, CephFsNamespaceDiagnosticRecord, CephFsNamespaceManifest,
    CephFsNamespaceProjection, CEPHFS_NAMESPACE_DECODER_PROFILE, CEPHFS_NAMESPACE_SCHEMA_VERSION,
};

use super::{
    projection_validation::{validate_assembly, validate_graph},
    CephFsSourceError, CephFsSourceResult,
};
use crate::ceph_reconstruction::{CephFsDescriptor, CephFsSparseExtentProof};

pub(super) fn build_namespace_projection(
    data_source_id: &DataSourceId,
    descriptor: &CephFsDescriptor,
    assembly: &CephFsNamespaceAssembly,
    input_sha256: &str,
    inline_data_by_inode: &BTreeMap<u64, Vec<u8>>,
    sparse_extents_by_inode: &BTreeMap<u64, Vec<CephFsSparseExtentProof>>,
) -> CephFsSourceResult<CephFsNamespaceProjection> {
    validate_assembly(assembly)?;
    let graph = assembly.graph();
    validate_graph(descriptor, graph)?;
    let entries = std::iter::once(&graph.root)
        .chain(graph.entries.iter())
        .collect::<Vec<_>>();
    let mut inodes = BTreeMap::new();
    let mut layouts = BTreeMap::new();
    for entry in &entries {
        collect_inode(
            entry,
            input_sha256,
            inline_data_by_inode,
            sparse_extents_by_inode,
            &mut inodes,
            &mut layouts,
        )?;
    }
    let dentries = entries.iter().map(|entry| dentry(entry)).collect();
    let diagnostics = graph
        .diagnostics
        .iter()
        .enumerate()
        .map(|(ordinal, diagnostic)| diagnostic_record(ordinal, diagnostic))
        .collect::<CephFsSourceResult<Vec<_>>>()?;
    let mut projection = CephFsNamespaceProjection {
        manifest: CephFsNamespaceManifest {
            filesystem_identity: descriptor.identity.clone(),
            data_source_id: data_source_id.0.clone(),
            filesystem_id: descriptor.filesystem_id,
            fsmap_epoch: descriptor.fsmap_epoch,
            root_inode: graph.filesystem_root_inode,
            input_sha256: input_sha256.to_string(),
            projection_sha256: "0".repeat(64),
            schema_version: CEPHFS_NAMESPACE_SCHEMA_VERSION,
            decoder_profile: CEPHFS_NAMESPACE_DECODER_PROFILE.to_string(),
            completeness: if graph.complete {
                "closed"
            } else {
                "incomplete"
            }
            .to_string(),
            published: graph.complete,
            entry_count: u64::try_from(entries.len())
                .map_err(|_| CephFsSourceError::InvalidInput("entry count overflows"))?,
            inode_count: u64::try_from(inodes.len())
                .map_err(|_| CephFsSourceError::InvalidInput("inode count overflows"))?,
            diagnostic_count: u64::try_from(diagnostics.len())
                .map_err(|_| CephFsSourceError::InvalidInput("diagnostic count overflows"))?,
        },
        inodes: inodes.into_values().collect(),
        layouts: layouts.into_values().collect(),
        dentries,
        diagnostics,
    };
    projection.manifest.projection_sha256 = cephfs_namespace_projection_digest(&projection);
    Ok(projection)
}

pub(super) fn build_file_entries(
    data_source_id: &DataSourceId,
    source_name: &str,
    graph: &CephFsNamespaceGraph,
) -> CephFsSourceResult<Vec<FileEntry>> {
    if !graph.complete {
        return Err(CephFsSourceError::IncompleteNamespace);
    }
    let mut entries = Vec::with_capacity(graph.entries.len().saturating_add(1));
    entries.push(file_entry(data_source_id, source_name, &graph.root, true));
    entries.extend(
        graph
            .entries
            .iter()
            .map(|entry| file_entry(data_source_id, &entry.name, entry, false)),
    );
    Ok(entries)
}

fn collect_inode(
    entry: &CephFsNamespaceEntry,
    namespace_input_sha256: &str,
    inline_data_by_inode: &BTreeMap<u64, Vec<u8>>,
    sparse_extents_by_inode: &BTreeMap<u64, Vec<CephFsSparseExtentProof>>,
    inodes: &mut BTreeMap<u64, CephFsInodeRecord>,
    layouts: &mut BTreeMap<u64, CephFsFileLayoutRecord>,
) -> CephFsSourceResult<()> {
    let Some(inode) = inode_record(entry)? else {
        return Ok(());
    };
    if let Some(existing) = inodes.insert(entry.inode, inode.clone()) {
        if existing != inode {
            return Err(CephFsSourceError::InvalidInput(
                "hard-linked inode metadata conflicts",
            ));
        }
    }
    let layout = entry
        .layout
        .as_ref()
        .ok_or(CephFsSourceError::InvalidInput("missing file layout"))?;
    ceph_wire::CephFsFileLayout::new(
        layout.stripe_unit,
        layout.stripe_count,
        layout.object_size,
        layout.pool_id,
        layout.pool_namespace.clone(),
    )
    .map_err(|_| CephFsSourceError::InvalidInput("invalid file layout"))?;
    let inline_data = inline_data_by_inode.get(&entry.inode).cloned();
    let sparse_extents = sparse_extents_by_inode
        .get(&entry.inode)
        .cloned()
        .unwrap_or_default();
    if inline_data.is_some() && !sparse_extents.is_empty() {
        return Err(CephFsSourceError::InvalidInput(
            "inline data and sparse extent proofs are mutually exclusive",
        ));
    }
    validate_sparse_extents(
        entry.inode,
        inode.size,
        namespace_input_sha256,
        &sparse_extents,
    )?;
    if inline_data
        .as_ref()
        .is_some_and(|bytes| bytes.len() > 65_536 || bytes.len() as u64 != inode.size)
    {
        return Err(CephFsSourceError::InvalidInput(
            "inline data is inconsistent with inode size",
        ));
    }
    if entry.kind != CephFsNamespaceEntryKind::Directory
        && inode.size > 0
        && layout.is_empty()
        && inline_data.is_none()
        && !covers_sparse_range(&sparse_extents, 0, inode.size)
    {
        return Err(CephFsSourceError::InvalidInput(
            "non-empty inode has neither object layout nor inline bytes",
        ));
    }
    let record = CephFsFileLayoutRecord {
        inode: entry.inode,
        stripe_unit: layout.stripe_unit,
        stripe_count: layout.stripe_count,
        object_size: layout.object_size,
        pool_id: layout.pool_id,
        pool_namespace: layout.pool_namespace.clone(),
        inline_data,
        sparse_extents: sparse_extents
            .into_iter()
            .map(|extent| {
                persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsSparseExtentRecord {
                    offset: extent.offset,
                    length: extent.length,
                    evidence_sha256: extent.evidence_sha256,
                    proof_sha256: extent.proof_sha256,
                }
            })
            .collect(),
    };
    if let Some(existing) = layouts.insert(entry.inode, record.clone()) {
        if existing != record {
            return Err(CephFsSourceError::InvalidInput(
                "hard-linked inode layouts conflict",
            ));
        }
    }
    Ok(())
}

fn inode_record(entry: &CephFsNamespaceEntry) -> CephFsSourceResult<Option<CephFsInodeRecord>> {
    let Some(mode) = entry.mode else {
        if entry.kind == CephFsNamespaceEntryKind::Remote {
            return Ok(None);
        }
        return Err(CephFsSourceError::InvalidInput(
            "resolved dentry has no inode metadata",
        ));
    };
    Ok(Some(CephFsInodeRecord {
        inode: entry.inode,
        mode,
        uid: entry
            .uid
            .ok_or(CephFsSourceError::InvalidInput("missing uid"))?,
        gid: entry
            .gid
            .ok_or(CephFsSourceError::InvalidInput("missing gid"))?,
        nlink: entry
            .nlink
            .ok_or(CephFsSourceError::InvalidInput("missing link count"))?,
        size: entry
            .size
            .ok_or(CephFsSourceError::InvalidInput("missing inode size"))?,
        inode_kind: entry_kind(&entry.kind).to_string(),
        encoded_version: entry
            .encoded_version
            .ok_or(CephFsSourceError::InvalidInput("missing inode version"))?,
        remaining_inode_bytes: u64::try_from(entry.remaining_inode_bytes.ok_or(
            CephFsSourceError::InvalidInput("missing remaining inode byte count"),
        )?)
        .map_err(|_| CephFsSourceError::InvalidInput("inode byte count overflows"))?,
    }))
}

fn validate_sparse_extents(
    inode: u64,
    file_size: u64,
    namespace_input_sha256: &str,
    extents: &[CephFsSparseExtentProof],
) -> CephFsSourceResult<()> {
    let mut previous_end = 0;
    for (index, extent) in extents.iter().enumerate() {
        extent
            .validate_for_inode(inode, file_size)
            .map_err(|_| CephFsSourceError::InvalidInput("invalid sparse extent proof"))?;
        if extent.evidence_sha256 != namespace_input_sha256 {
            return Err(CephFsSourceError::InvalidInput(
                "sparse extent proof is not bound to namespace evidence",
            ));
        }
        if index > 0 && extent.offset < previous_end {
            return Err(CephFsSourceError::InvalidInput(
                "sparse extents overlap or are not ordered",
            ));
        }
        previous_end = extent.end();
    }
    Ok(())
}

fn covers_sparse_range(extents: &[CephFsSparseExtentProof], offset: u64, end: u64) -> bool {
    if offset >= end {
        return true;
    }
    let mut cursor = offset;
    for extent in extents {
        if extent.offset > cursor {
            return false;
        }
        cursor = cursor.max(extent.end());
        if cursor >= end {
            return true;
        }
    }
    false
}

fn dentry(entry: &CephFsNamespaceEntry) -> CephFsDentryRecord {
    CephFsDentryRecord {
        entry_id: entry.entry_id.clone(),
        parent_entry_id: entry.parent_entry_id.clone(),
        parent_inode: entry.parent_inode,
        child_inode: entry.inode,
        fragment: entry.fragment,
        name: entry.name.clone(),
        path: entry.path.clone(),
        entry_kind: entry_kind(&entry.kind).to_string(),
        mode: entry.mode,
        uid: entry.uid,
        gid: entry.gid,
        nlink: entry.nlink,
        size: entry.size,
        alternate_name: entry.alternate_name.clone(),
    }
}

fn diagnostic_record(
    ordinal: usize,
    diagnostic: &CephFsNamespaceDiagnostic,
) -> CephFsSourceResult<CephFsNamespaceDiagnosticRecord> {
    let ordinal = u64::try_from(ordinal)
        .map_err(|_| CephFsSourceError::InvalidInput("diagnostic ordinal overflows"))?;
    let (kind, parent_inode, child_inode, name, snap_id) = match diagnostic {
        CephFsNamespaceDiagnostic::SnapshotDentrySkipped {
            parent_inode,
            name,
            snap_id,
        } => (
            "snapshot_skipped",
            *parent_inode,
            0,
            name.clone(),
            Some(*snap_id),
        ),
        CephFsNamespaceDiagnostic::DuplicateDentry { parent_inode, name } => {
            ("duplicate", *parent_inode, 0, name.clone(), None)
        }
        CephFsNamespaceDiagnostic::OrphanDentry {
            parent_inode,
            child_inode,
            name,
        } => ("orphan", *parent_inode, *child_inode, name.clone(), None),
        CephFsNamespaceDiagnostic::CycleDentry {
            parent_inode,
            child_inode,
            name,
        } => ("cycle", *parent_inode, *child_inode, name.clone(), None),
    };
    Ok(CephFsNamespaceDiagnosticRecord {
        diagnostic_ordinal: ordinal,
        diagnostic_kind: kind.to_string(),
        parent_inode,
        child_inode,
        name,
        snap_id,
    })
}

fn file_entry(
    data_source_id: &DataSourceId,
    display_name: &str,
    entry: &CephFsNamespaceEntry,
    root: bool,
) -> FileEntry {
    let directory = entry.kind == CephFsNamespaceEntryKind::Directory;
    FileEntry {
        id: FileEntryId(entry.entry_id.clone()),
        parent_id: entry.parent_entry_id.clone().map(FileEntryId),
        data_source_id: data_source_id.clone(),
        path: entry.path.clone(),
        name: if root {
            display_name.to_string()
        } else {
            entry.name.clone()
        },
        entry_type: if directory {
            EntryType::Directory
        } else {
            EntryType::File
        },
        size: (!directory).then_some(entry.size.unwrap_or(0)),
        ext: (!directory).then(|| extension(&entry.name)).flatten(),
        deleted: false,
        hidden: !root && entry.name.starts_with('.'),
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn extension(name: &str) -> Option<String> {
    let (_, extension) = name.rsplit_once('.')?;
    (!extension.is_empty()).then(|| extension.to_ascii_lowercase())
}

fn entry_kind(kind: &CephFsNamespaceEntryKind) -> &'static str {
    match kind {
        CephFsNamespaceEntryKind::File => "file",
        CephFsNamespaceEntryKind::Directory => "directory",
        CephFsNamespaceEntryKind::Symlink => "symlink",
        CephFsNamespaceEntryKind::Remote => "remote",
        CephFsNamespaceEntryKind::Other => "other",
    }
}
