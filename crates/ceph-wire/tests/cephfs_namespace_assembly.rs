use ceph_wire::{
    assemble_cephfs_namespace, CephFsDentryKey, CephFsDentryKind, CephFsDentryProjection,
    CephFsDirfragBatch, CephFsDirfragIdentity, CephFsDirfragParentProof, CephFsFileLayout,
    CephFsInodeKind, CephFsInodeProjection, CephFsMetadataMutationState,
    CephFsNamespaceAssemblyInput, CephFsNamespaceFreezeReason, CephFsNamespaceRecord, CEPH_NOSNAP,
};

#[test]
fn complete_batches_produce_a_closed_deterministic_namespace() {
    let root = CephFsDirfragIdentity::new(1, 0).unwrap();
    let child = CephFsDirfragIdentity::new(2, 0).unwrap();
    let parent_proof = CephFsDirfragParentProof::new(&child, 1, 0, "etc").unwrap();
    let input = CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone(), child.clone()],
        batches: vec![
            CephFsDirfragBatch {
                identity: root,
                records: vec![record(1, "etc", 2, directory(2))],
                complete: true,
                parent_proof: None,
            },
            CephFsDirfragBatch {
                identity: child,
                records: vec![record(2, "passwd", 3, file(3))],
                complete: true,
                parent_proof: Some(parent_proof),
            },
        ],
        mutation_state: CephFsMetadataMutationState::Complete,
    };

    let first = assemble_cephfs_namespace(input.clone()).unwrap();
    let second = assemble_cephfs_namespace(input).unwrap();

    assert!(first.is_complete());
    assert!(!first.is_frozen());
    assert!(first.freeze_reasons().is_empty());
    assert_eq!(first.graph().entries.len(), 2);
    assert_eq!(first.assembly_sha256(), second.assembly_sha256());
}

#[test]
fn assembly_digest_binds_root_inode_metadata() {
    let root = CephFsDirfragIdentity::new(1, 0).unwrap();
    let input = CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone()],
        batches: vec![CephFsDirfragBatch {
            identity: root,
            records: Vec::new(),
            complete: true,
            parent_proof: None,
        }],
        mutation_state: CephFsMetadataMutationState::Complete,
    };
    let first = assemble_cephfs_namespace(input.clone()).unwrap();
    let mut changed = input;
    changed.root_inode.uid = 2000;
    let second = assemble_cephfs_namespace(changed).unwrap();

    assert_ne!(first.assembly_sha256(), second.assembly_sha256());
}

#[test]
fn assembly_digest_binds_remote_dentry_type() {
    let root = CephFsDirfragIdentity::new(1, 0).unwrap();
    let mut remote = record(1, "remote", 2, file(2));
    remote.dentry.kind = CephFsDentryKind::Remote { d_type: 4 };
    remote.dentry.inode = None;
    let input = CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone()],
        batches: vec![CephFsDirfragBatch {
            identity: root,
            records: vec![remote],
            complete: true,
            parent_proof: None,
        }],
        mutation_state: CephFsMetadataMutationState::Complete,
    };
    let first = assemble_cephfs_namespace(input.clone()).unwrap();
    let mut changed = input;
    changed.batches[0].records[0].dentry.kind = CephFsDentryKind::Remote { d_type: 8 };
    let second = assemble_cephfs_namespace(changed).unwrap();

    assert_ne!(first.assembly_sha256(), second.assembly_sha256());
}

#[test]
fn missing_dirfrag_unknown_mutation_and_incomplete_batch_freeze_without_publishing() {
    let root = CephFsDirfragIdentity::new(1, 0).unwrap();
    let expected_child = CephFsDirfragIdentity::new(2, 0).unwrap();
    let result = assemble_cephfs_namespace(CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone(), expected_child.clone()],
        batches: vec![CephFsDirfragBatch {
            identity: root,
            records: vec![],
            complete: false,
            parent_proof: None,
        }],
        mutation_state: CephFsMetadataMutationState::Unknown {
            digest: "a".repeat(64),
        },
    })
    .unwrap();

    assert!(!result.is_complete());
    assert!(result.is_frozen());
    assert!(result.freeze_reasons().iter().any(|reason| matches!(
        reason,
        CephFsNamespaceFreezeReason::MissingDirfrag(identity) if identity == &expected_child
    )));
    assert!(result.freeze_reasons().iter().any(|reason| matches!(
        reason,
        CephFsNamespaceFreezeReason::UnknownMetadataMutation { .. }
    )));
    assert!(!result.graph().complete);
}

#[test]
fn untracked_child_directory_freezes_namespace_closure() {
    let root = CephFsDirfragIdentity::new(1, 0).unwrap();
    let result = assemble_cephfs_namespace(CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone()],
        batches: vec![CephFsDirfragBatch {
            identity: root,
            records: vec![record(1, "etc", 2, directory(2))],
            complete: true,
            parent_proof: None,
        }],
        mutation_state: CephFsMetadataMutationState::Complete,
    })
    .unwrap();

    assert!(!result.is_complete());
    assert!(result.freeze_reasons().iter().any(|reason| matches!(
        reason,
        CephFsNamespaceFreezeReason::UntrackedDirectory { inode: 2 }
    )));
}

#[test]
fn unexpected_dirfrag_and_file_backtrace_fail_closed() {
    let root = CephFsDirfragIdentity::new(1, 0).unwrap();
    let child = CephFsDirfragIdentity::new(2, 0).unwrap();
    let child_batch = CephFsDirfragBatch {
        identity: child.clone(),
        records: Vec::new(),
        complete: true,
        parent_proof: Some(CephFsDirfragParentProof::new(&child, 1, 0, "child").unwrap()),
    };
    assert!(assemble_cephfs_namespace(CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone()],
        batches: vec![
            CephFsDirfragBatch {
                identity: root.clone(),
                records: vec![record(1, "child", 2, directory(2))],
                complete: true,
                parent_proof: None,
            },
            child_batch.clone(),
        ],
        mutation_state: CephFsMetadataMutationState::Complete,
    })
    .is_err());

    let result = assemble_cephfs_namespace(CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone(), child],
        batches: vec![
            CephFsDirfragBatch {
                identity: root,
                records: vec![record(1, "child", 2, file(2))],
                complete: true,
                parent_proof: None,
            },
            child_batch,
        ],
        mutation_state: CephFsMetadataMutationState::Complete,
    })
    .unwrap();
    assert!(!result.is_complete());
    assert!(result.freeze_reasons().iter().any(|reason| matches!(
        reason,
        CephFsNamespaceFreezeReason::UnmatchedBacktrace(identity) if identity.inode == 2
    )));
}

#[test]
fn unsafe_or_duplicate_dentries_are_rejected_before_assembly() {
    let root = CephFsDirfragIdentity::new(1, 0).unwrap();
    let mut unsafe_record = record(1, "../passwd", 2, file(2));
    let unsafe_input = CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone()],
        batches: vec![CephFsDirfragBatch {
            identity: root.clone(),
            records: vec![unsafe_record.clone()],
            complete: true,
            parent_proof: None,
        }],
        mutation_state: CephFsMetadataMutationState::Complete,
    };
    assert!(assemble_cephfs_namespace(unsafe_input).is_err());

    unsafe_record.dentry.key.name = "passwd".to_string();
    assert!(assemble_cephfs_namespace(CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone()],
        batches: vec![CephFsDirfragBatch {
            identity: root,
            records: vec![unsafe_record.clone(), unsafe_record],
            complete: true,
            parent_proof: None,
        }],
        mutation_state: CephFsMetadataMutationState::Complete,
    })
    .is_err());

    let mut mismatched_inode = file(3);
    mismatched_inode.mode = 0o040644;
    assert!(assemble_cephfs_namespace(CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![CephFsDirfragIdentity::new(1, 0).unwrap()],
        batches: vec![CephFsDirfragBatch {
            identity: CephFsDirfragIdentity::new(1, 0).unwrap(),
            records: vec![record(1, "bad-mode", 3, mismatched_inode)],
            complete: true,
            parent_proof: None,
        }],
        mutation_state: CephFsMetadataMutationState::Complete,
    })
    .is_err());
}

#[test]
fn duplicate_or_mismatched_batches_and_bad_parent_proofs_fail_closed() {
    let root = CephFsDirfragIdentity::new(1, 0).unwrap();
    let duplicate = CephFsDirfragBatch {
        identity: root.clone(),
        records: vec![],
        complete: true,
        parent_proof: None,
    };
    assert!(assemble_cephfs_namespace(CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root.clone()],
        batches: vec![duplicate.clone(), duplicate],
        mutation_state: CephFsMetadataMutationState::Complete,
    })
    .is_err());

    let child = CephFsDirfragIdentity::new(2, 0).unwrap();
    let mismatched = CephFsDirfragBatch {
        identity: child,
        records: vec![record(1, "wrong", 2, directory(2))],
        complete: true,
        parent_proof: None,
    };
    assert!(assemble_cephfs_namespace(CephFsNamespaceAssemblyInput {
        root_inode: directory(1),
        expected_dirfrags: vec![root, CephFsDirfragIdentity::new(2, 0).unwrap()],
        batches: vec![mismatched],
        mutation_state: CephFsMetadataMutationState::Complete,
    })
    .is_err());
}

fn record(
    parent: u64,
    name: &str,
    child: u64,
    inode: CephFsInodeProjection,
) -> CephFsNamespaceRecord {
    CephFsNamespaceRecord {
        parent: CephFsDirfragIdentity::new(parent, 0).unwrap(),
        dentry: CephFsDentryProjection {
            key: CephFsDentryKey {
                name: name.to_string(),
                snap_id: CEPH_NOSNAP,
            },
            first_snap: CEPH_NOSNAP,
            kind: CephFsDentryKind::Primary,
            child_inode: child,
            alternate_name: String::new(),
            inode: Some(inode),
        },
    }
}

fn directory(inode: u64) -> CephFsInodeProjection {
    projection(inode, CephFsInodeKind::Directory, 0)
}

fn file(inode: u64) -> CephFsInodeProjection {
    projection(inode, CephFsInodeKind::File, 8)
}

fn projection(inode: u64, kind: CephFsInodeKind, size: u64) -> CephFsInodeProjection {
    CephFsInodeProjection {
        ino: inode,
        mode: if kind == CephFsInodeKind::Directory {
            0o040755
        } else {
            0o100644
        },
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
