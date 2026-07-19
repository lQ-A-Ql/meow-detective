use ceph_wire::{
    build_cephfs_namespace, CephFsDentryKey, CephFsDentryKind, CephFsDentryProjection,
    CephFsDirfragIdentity, CephFsFileLayout, CephFsInodeKind, CephFsInodeProjection,
    CephFsNamespaceDiagnostic, CephFsNamespaceRecord, CEPH_NOSNAP,
};

#[test]
fn builds_nested_paths_and_keeps_snapshot_records_out_of_the_tree() {
    let records = vec![
        record(1, "etc", 2, directory(2)),
        record(2, "passwd", 3, file(3)),
        record_with_snap(1, "etc", 9, file(9), 7),
    ];
    let graph = build_cephfs_namespace(directory(1), &records).unwrap();
    assert_eq!(graph.root.path, "/");
    assert_eq!(graph.entries.len(), 2);
    assert_eq!(graph.entries[0].path, "/etc");
    assert_eq!(graph.entries[1].path, "/etc/passwd");
    assert!(graph.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        CephFsNamespaceDiagnostic::SnapshotDentrySkipped { snap_id: 7, .. }
    )));
    assert!(graph.complete);
}

#[test]
fn fails_closed_for_duplicate_orphan_and_cycle_edges() {
    let records = vec![
        record(1, "same", 2, file(2)),
        record(1, "same", 3, file(3)),
        record(99, "orphan", 4, file(4)),
        record(1, "loop", 1, directory(1)),
    ];
    let graph = build_cephfs_namespace(directory(1), &records).unwrap();
    assert!(graph.entries.iter().all(|entry| entry.path != "/orphan"));
    assert!(graph.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        CephFsNamespaceDiagnostic::DuplicateDentry { name, .. } if name == "same"
    )));
    assert!(graph.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        CephFsNamespaceDiagnostic::OrphanDentry { name, .. } if name == "orphan"
    )));
    assert!(graph.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        CephFsNamespaceDiagnostic::CycleDentry { name, .. } if name == "loop"
    )));
    assert!(!graph.complete);
}

fn record(
    parent: u64,
    name: &str,
    child: u64,
    inode: CephFsInodeProjection,
) -> CephFsNamespaceRecord {
    record_with_snap(parent, name, child, inode, CEPH_NOSNAP)
}

fn record_with_snap(
    parent: u64,
    name: &str,
    child: u64,
    inode: CephFsInodeProjection,
    snap_id: u64,
) -> CephFsNamespaceRecord {
    CephFsNamespaceRecord {
        parent: CephFsDirfragIdentity::new(parent, 0).unwrap(),
        dentry: CephFsDentryProjection {
            key: CephFsDentryKey {
                name: name.to_string(),
                snap_id,
            },
            first_snap: CEPH_NOSNAP,
            kind: CephFsDentryKind::Primary,
            child_inode: child,
            alternate_name: String::new(),
            inode: Some(inode),
        },
    }
}

fn directory(ino: u64) -> CephFsInodeProjection {
    projection(ino, CephFsInodeKind::Directory, 0o040755, 0)
}

fn file(ino: u64) -> CephFsInodeProjection {
    projection(ino, CephFsInodeKind::File, 0o100644, 12)
}

fn projection(ino: u64, kind: CephFsInodeKind, mode: u32, size: u64) -> CephFsInodeProjection {
    CephFsInodeProjection {
        ino,
        mode,
        uid: 1000,
        gid: 1000,
        nlink: 1,
        size,
        kind,
        layout: CephFsFileLayout::new(0, 0, 0, -1, "").unwrap(),
        encoded_version: 20,
        remaining_inode_bytes: 0,
    }
}

#[test]
fn rejects_a_non_directory_root() {
    assert!(build_cephfs_namespace(file(1), &[]).is_err());
}
