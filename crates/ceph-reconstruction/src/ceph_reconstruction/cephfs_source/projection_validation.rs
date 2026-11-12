use std::collections::{BTreeMap, BTreeSet};

use ceph_wire::{
    CephFsNamespaceAssembly, CephFsNamespaceDiagnostic, CephFsNamespaceEntry,
    CephFsNamespaceEntryKind, CephFsNamespaceGraph,
};

use super::{CephFsSourceError, CephFsSourceResult};
use crate::ceph_reconstruction::{CephFsDescriptor, CephFsDescriptorState};

pub(super) fn validate_assembly(assembly: &CephFsNamespaceAssembly) -> CephFsSourceResult<()> {
    if assembly.assembly_sha256().len() != 64
        || !assembly
            .assembly_sha256()
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || assembly.is_complete() != assembly.graph().complete
        || assembly.is_frozen() == assembly.is_complete()
        || (assembly.is_complete() && !assembly.freeze_reasons().is_empty())
        || (!assembly.is_complete() && assembly.freeze_reasons().is_empty())
    {
        return Err(CephFsSourceError::InvalidInput(
            "namespace assembly state is inconsistent",
        ));
    }
    Ok(())
}

pub(super) fn validate_graph(
    descriptor: &CephFsDescriptor,
    graph: &CephFsNamespaceGraph,
) -> CephFsSourceResult<()> {
    if descriptor.state != CephFsDescriptorState::Present
        || graph.filesystem_root_inode == 0
        || graph.root.inode != graph.filesystem_root_inode
        || graph.root.kind != CephFsNamespaceEntryKind::Directory
    {
        return Err(CephFsSourceError::InvalidInput(
            "descriptor or namespace root is not publishable",
        ));
    }
    let entries = std::iter::once(&graph.root)
        .chain(graph.entries.iter())
        .collect::<Vec<_>>();
    let mut by_id = BTreeMap::new();
    let mut inode_kinds = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let is_root = index == 0;
        validate_entry_shape(entry, is_root)?;
        if by_id.insert(entry.entry_id.as_str(), *entry).is_some() {
            return Err(CephFsSourceError::InvalidInput(
                "namespace entry IDs are duplicated",
            ));
        }
        if !is_root && !names.insert((entry.parent_inode, entry.name.clone())) {
            return Err(CephFsSourceError::InvalidInput(
                "namespace contains duplicate dentries",
            ));
        }
        if !paths.insert(entry.path.clone()) {
            return Err(CephFsSourceError::InvalidInput(
                "namespace contains duplicate paths",
            ));
        }
        if let Some(previous) = inode_kinds.insert(entry.inode, entry_kind(&entry.kind)) {
            if previous != entry_kind(&entry.kind) {
                return Err(CephFsSourceError::InvalidInput(
                    "inode is referenced with conflicting entry kinds",
                ));
            }
        }
    }
    for entry in entries.iter().skip(1) {
        let parent_id = entry
            .parent_entry_id
            .as_deref()
            .ok_or(CephFsSourceError::InvalidInput(
                "non-root dentry has no parent entry",
            ))?;
        let parent = by_id.get(parent_id).ok_or(CephFsSourceError::InvalidInput(
            "dentry references a missing parent entry",
        ))?;
        if parent.inode != entry.parent_inode || parent.kind != CephFsNamespaceEntryKind::Directory
        {
            return Err(CephFsSourceError::InvalidInput(
                "dentry parent inode or kind is inconsistent",
            ));
        }
        if entry.path != join_path(&parent.path, &entry.name) {
            return Err(CephFsSourceError::InvalidInput(
                "dentry path is not derived from its parent",
            ));
        }
    }
    validate_parent_chains(&entries, &by_id)?;
    validate_link_counts(&entries, graph.complete)?;
    if graph.complete
        && (graph.entries.iter().any(|entry| {
            matches!(
                entry.kind,
                CephFsNamespaceEntryKind::Remote | CephFsNamespaceEntryKind::Other
            )
        }) || graph.diagnostics.iter().any(|diagnostic| {
            !matches!(
                diagnostic,
                CephFsNamespaceDiagnostic::SnapshotDentrySkipped { .. }
            )
        }))
    {
        return Err(CephFsSourceError::InvalidInput(
            "closed namespace contains unresolved dentries",
        ));
    }
    Ok(())
}

fn validate_link_counts(
    entries: &[&CephFsNamespaceEntry],
    complete: bool,
) -> CephFsSourceResult<()> {
    let mut links = BTreeMap::new();
    for entry in entries {
        if entry.kind == CephFsNamespaceEntryKind::Remote {
            continue;
        }
        let nlink = entry
            .nlink
            .ok_or(CephFsSourceError::InvalidInput("missing link count"))?;
        if nlink <= 0 {
            return Err(CephFsSourceError::InvalidInput(
                "inode link count must be positive",
            ));
        }
        let value = links
            .entry(entry.inode)
            .or_insert((entry.kind.clone(), nlink, 0u64));
        if value.0 != entry.kind || value.1 != nlink {
            return Err(CephFsSourceError::InvalidInput(
                "hard-linked inode kind or link count conflicts",
            ));
        }
        value.2 = value.2.saturating_add(1);
    }
    if links.values().any(|(kind, nlink, observed)| {
        let expected = u64::try_from(*nlink).unwrap_or(0);
        *observed > expected
            || (complete && *kind != CephFsNamespaceEntryKind::Directory && *observed != expected)
    }) {
        return Err(CephFsSourceError::InvalidInput(
            "inode link count does not match namespace references",
        ));
    }
    Ok(())
}

fn validate_entry_shape(entry: &CephFsNamespaceEntry, root: bool) -> CephFsSourceResult<()> {
    if entry.entry_id.trim().is_empty()
        || entry.entry_id.contains('\0')
        || entry.inode == 0
        || entry.alternate_name.contains('\0')
        || entry.path.is_empty()
        || !entry.path.starts_with('/')
        || entry.path.contains('\0')
        || entry.path.contains("//")
        || entry.path.split('/').any(|part| matches!(part, "." | ".."))
        || entry.nlink.is_some_and(|nlink| nlink < 0)
    {
        return Err(CephFsSourceError::InvalidInput(
            "namespace entry identity or path is invalid",
        ));
    }
    if root {
        if entry.parent_entry_id.is_some()
            || entry.parent_inode != 0
            || entry.name != "/"
            || entry.path != "/"
            || entry.fragment != 0
            || entry.kind != CephFsNamespaceEntryKind::Directory
        {
            return Err(CephFsSourceError::InvalidInput(
                "namespace root entry is invalid",
            ));
        }
    } else if entry.parent_inode == 0
        || entry.name.is_empty()
        || entry.name.contains('/')
        || matches!(entry.name.as_str(), "." | "..")
    {
        return Err(CephFsSourceError::InvalidInput(
            "non-root namespace entry has an invalid name or parent",
        ));
    }
    Ok(())
}

fn validate_parent_chains(
    entries: &[&CephFsNamespaceEntry],
    by_id: &BTreeMap<&str, &CephFsNamespaceEntry>,
) -> CephFsSourceResult<()> {
    for entry in entries.iter().skip(1) {
        let mut seen = BTreeSet::new();
        let mut current = *entry;
        loop {
            if !seen.insert(current.entry_id.as_str()) {
                return Err(CephFsSourceError::InvalidInput(
                    "namespace contains a parent cycle",
                ));
            }
            let Some(parent_id) = current.parent_entry_id.as_deref() else {
                break;
            };
            current = *by_id.get(parent_id).ok_or(CephFsSourceError::InvalidInput(
                "namespace parent chain is not closed",
            ))?;
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

fn entry_kind(kind: &CephFsNamespaceEntryKind) -> &'static str {
    match kind {
        CephFsNamespaceEntryKind::File => "file",
        CephFsNamespaceEntryKind::Directory => "directory",
        CephFsNamespaceEntryKind::Symlink => "symlink",
        CephFsNamespaceEntryKind::Remote => "remote",
        CephFsNamespaceEntryKind::Other => "other",
    }
}
