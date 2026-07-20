use std::collections::{BTreeMap, BTreeSet};

use super::{
    dirfrag::{CephFsDentryKind, CephFsDentryProjection, CephFsDirfragIdentity},
    inode::{CephFsInodeKind, CephFsInodeProjection},
    layout::CephFsFileLayout,
};
use crate::{CephWireError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceRecord {
    pub parent: CephFsDirfragIdentity,
    pub dentry: CephFsDentryProjection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CephFsNamespaceEntryKind {
    File,
    Directory,
    Symlink,
    Remote,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceEntry {
    pub entry_id: String,
    pub parent_entry_id: Option<String>,
    pub parent_inode: u64,
    pub inode: u64,
    pub fragment: u32,
    pub name: String,
    pub path: String,
    pub kind: CephFsNamespaceEntryKind,
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub nlink: Option<i32>,
    pub size: Option<u64>,
    pub layout: Option<CephFsFileLayout>,
    pub encoded_version: Option<u8>,
    pub remaining_inode_bytes: Option<usize>,
    pub alternate_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CephFsNamespaceDiagnostic {
    SnapshotDentrySkipped {
        parent_inode: u64,
        name: String,
        snap_id: u64,
    },
    DuplicateDentry {
        parent_inode: u64,
        name: String,
    },
    OrphanDentry {
        parent_inode: u64,
        child_inode: u64,
        name: String,
    },
    CycleDentry {
        parent_inode: u64,
        child_inode: u64,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceGraph {
    pub filesystem_root_inode: u64,
    pub root: CephFsNamespaceEntry,
    pub entries: Vec<CephFsNamespaceEntry>,
    pub diagnostics: Vec<CephFsNamespaceDiagnostic>,
    pub complete: bool,
}

pub fn build_cephfs_namespace(
    root_inode: CephFsInodeProjection,
    records: &[CephFsNamespaceRecord],
) -> Result<CephFsNamespaceGraph> {
    if root_inode.ino == 0 || !root_inode.is_directory() {
        return Err(CephWireError::InvalidCephFsInode {
            field: "root",
            reason: "CephFS namespace root must be a non-zero directory inode",
        });
    }
    let root = root_entry(&root_inode);
    let PreparedRecords {
        pending,
        mut diagnostics,
        head_count,
    } = prepare_records(records);
    let mut entries = materialize_records(&root_inode, &root, pending, &mut diagnostics);
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let complete = diagnostics.iter().all(|diagnostic| {
        matches!(
            diagnostic,
            CephFsNamespaceDiagnostic::SnapshotDentrySkipped { .. }
        )
    }) && entries.len() == head_count
        && entries.iter().all(|entry| {
            !matches!(
                entry.kind,
                CephFsNamespaceEntryKind::Remote | CephFsNamespaceEntryKind::Other
            )
        });
    Ok(CephFsNamespaceGraph {
        filesystem_root_inode: root_inode.ino,
        root,
        entries,
        diagnostics,
        complete,
    })
}

struct PreparedRecords {
    pending: Vec<CephFsNamespaceRecord>,
    diagnostics: Vec<CephFsNamespaceDiagnostic>,
    head_count: usize,
}

fn prepare_records(records: &[CephFsNamespaceRecord]) -> PreparedRecords {
    let mut pending = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen_names = BTreeSet::new();
    for record in records {
        if !record.dentry.key.is_head() {
            diagnostics.push(CephFsNamespaceDiagnostic::SnapshotDentrySkipped {
                parent_inode: record.parent.inode,
                name: record.dentry.key.name.clone(),
                snap_id: record.dentry.key.snap_id,
            });
            continue;
        }
        let key = (record.parent.inode, record.dentry.key.name.clone());
        if !seen_names.insert(key) {
            diagnostics.push(CephFsNamespaceDiagnostic::DuplicateDentry {
                parent_inode: record.parent.inode,
                name: record.dentry.key.name.clone(),
            });
            continue;
        }
        pending.push(record.clone());
    }
    pending.sort_by(|left, right| {
        (
            left.parent.inode,
            left.dentry.key.name.as_str(),
            left.dentry.child_inode,
        )
            .cmp(&(
                right.parent.inode,
                right.dentry.key.name.as_str(),
                right.dentry.child_inode,
            ))
    });
    PreparedRecords {
        pending,
        diagnostics,
        head_count: seen_names.len(),
    }
}

fn materialize_records(
    root_inode: &CephFsInodeProjection,
    root: &CephFsNamespaceEntry,
    pending: Vec<CephFsNamespaceRecord>,
    diagnostics: &mut Vec<CephFsNamespaceDiagnostic>,
) -> Vec<CephFsNamespaceEntry> {
    let mut entries = Vec::new();
    let mut paths = BTreeMap::from([(
        root_inode.ino,
        (root.path.clone(), Vec::from([root_inode.ino])),
    )]);
    let mut parent_entries = BTreeMap::from([(root_inode.ino, root.entry_id.clone())]);
    let mut remaining = pending;
    loop {
        let mut next = Vec::new();
        let mut progress = false;
        for record in remaining {
            let Some((parent_path, ancestors)) = paths.get(&record.parent.inode).cloned() else {
                next.push(record);
                continue;
            };
            if ancestors.contains(&record.dentry.child_inode) {
                diagnostics.push(CephFsNamespaceDiagnostic::CycleDentry {
                    parent_inode: record.parent.inode,
                    child_inode: record.dentry.child_inode,
                    name: record.dentry.key.name.clone(),
                });
                progress = true;
                continue;
            }
            let path = join_path(&parent_path, &record.dentry.key.name);
            let entry = entry_from_record(&record, &path, parent_entries.get(&record.parent.inode));
            let entry_id = entry.entry_id.clone();
            if entry.kind == CephFsNamespaceEntryKind::Directory {
                paths.entry(entry.inode).or_insert_with(|| {
                    (entry.path.clone(), append_ancestor(&ancestors, entry.inode))
                });
                parent_entries
                    .entry(entry.inode)
                    .or_insert(entry_id.clone());
            }
            entries.push(entry);
            progress = true;
        }
        if !progress {
            for record in next {
                diagnostics.push(CephFsNamespaceDiagnostic::OrphanDentry {
                    parent_inode: record.parent.inode,
                    child_inode: record.dentry.child_inode,
                    name: record.dentry.key.name,
                });
            }
            break;
        }
        if next.is_empty() {
            break;
        }
        remaining = next;
    }
    entries
}

fn root_entry(root: &CephFsInodeProjection) -> CephFsNamespaceEntry {
    CephFsNamespaceEntry {
        entry_id: format!("cephfs:root:{:016x}", root.ino),
        parent_entry_id: None,
        parent_inode: 0,
        inode: root.ino,
        fragment: 0,
        name: "/".to_string(),
        path: "/".to_string(),
        kind: CephFsNamespaceEntryKind::Directory,
        mode: Some(root.mode),
        uid: Some(root.uid),
        gid: Some(root.gid),
        nlink: Some(root.nlink),
        size: Some(root.size),
        layout: Some(root.layout.clone()),
        encoded_version: Some(root.encoded_version),
        remaining_inode_bytes: Some(root.remaining_inode_bytes),
        alternate_name: String::new(),
    }
}

fn entry_from_record(
    record: &CephFsNamespaceRecord,
    path: &str,
    parent_entry_id: Option<&String>,
) -> CephFsNamespaceEntry {
    let inode = record.dentry.inode.as_ref();
    let kind = match (&record.dentry.kind, inode.map(|value| value.kind)) {
        (CephFsDentryKind::Remote { .. }, _) => CephFsNamespaceEntryKind::Remote,
        (_, Some(CephFsInodeKind::File)) => CephFsNamespaceEntryKind::File,
        (_, Some(CephFsInodeKind::Directory)) => CephFsNamespaceEntryKind::Directory,
        (_, Some(CephFsInodeKind::Symlink)) => CephFsNamespaceEntryKind::Symlink,
        _ => CephFsNamespaceEntryKind::Other,
    };
    CephFsNamespaceEntry {
        entry_id: format!(
            "cephfs:{:016x}:{:08x}:{:016x}:{}",
            record.parent.inode,
            record.parent.fragment,
            record.dentry.child_inode,
            record.dentry.key.name
        ),
        parent_entry_id: parent_entry_id.cloned(),
        parent_inode: record.parent.inode,
        inode: record.dentry.child_inode,
        fragment: record.parent.fragment,
        name: record.dentry.key.name.clone(),
        path: path.to_string(),
        kind,
        mode: inode.map(|value| value.mode),
        uid: inode.map(|value| value.uid),
        gid: inode.map(|value| value.gid),
        nlink: inode.map(|value| value.nlink),
        size: inode.map(|value| value.size),
        layout: inode.map(|value| value.layout.clone()),
        encoded_version: inode.map(|value| value.encoded_version),
        remaining_inode_bytes: inode.map(|value| value.remaining_inode_bytes),
        alternate_name: record.dentry.alternate_name.clone(),
    }
}

fn append_ancestor(ancestors: &[u64], inode: u64) -> Vec<u64> {
    let mut result = ancestors.to_vec();
    result.push(inode);
    result
}

fn join_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}
