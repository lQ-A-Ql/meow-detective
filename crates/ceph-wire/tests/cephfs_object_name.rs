use ceph_wire::{
    classify_cephfs_metadata_object_name, CephFsMetadataObjectCandidates,
    CephFsMetadataObjectClass, CephFsRankTableKind,
};

#[test]
fn classifies_directory_and_standalone_inode_objects() {
    let (class, candidates) = classify_cephfs_metadata_object_name(b"1000.00000000");
    assert_eq!(
        class,
        CephFsMetadataObjectClass::DirFragmentCandidate {
            inode: 0x1000,
            fragment: 0,
        }
    );
    assert_eq!(candidates.mask(), 0b11_1111);

    let (class, candidates) = classify_cephfs_metadata_object_name(b"1.00000000.inode");
    assert_eq!(
        class,
        CephFsMetadataObjectClass::StandaloneInodeCandidate { inode: 1 }
    );
    assert_eq!(
        candidates.mask(),
        CephFsMetadataObjectCandidates::INODE
            | CephFsMetadataObjectCandidates::XATTR
            | CephFsMetadataObjectCandidates::SNAPSHOT_REALM
    );
}

#[test]
fn classifies_journal_pointer_queue_and_tables() {
    assert_eq!(
        classify_cephfs_metadata_object_name(b"202.00000003").0,
        CephFsMetadataObjectClass::JournalData {
            rank: 2,
            backup: false,
            object_index: 3,
        }
    );
    assert_eq!(
        classify_cephfs_metadata_object_name(b"404.00000000").0,
        CephFsMetadataObjectClass::JournalPointer { rank: 4 }
    );
    assert_eq!(
        classify_cephfs_metadata_object_name(b"503.00000001").0,
        CephFsMetadataObjectClass::PurgeQueue {
            rank: 3,
            object_index: 1,
        }
    );
    assert_eq!(
        classify_cephfs_metadata_object_name(b"mds7_inotable").0,
        CephFsMetadataObjectClass::RankTable {
            rank: 7,
            kind: CephFsRankTableKind::Inode,
        }
    );
    assert_eq!(
        classify_cephfs_metadata_object_name(b"mds7_openfiles.a").0,
        CephFsMetadataObjectClass::OpenFileTable {
            rank: 7,
            object_index: 10,
        }
    );
    assert_eq!(
        classify_cephfs_metadata_object_name(b"mds_snaptable").0,
        CephFsMetadataObjectClass::SnapTable
    );
}

#[test]
fn unknown_and_non_canonical_names_remain_metadata_only() {
    for name in [
        b"opaque-object".as_slice(),
        b"1000.0000000G",
        b"01000.00000000",
        &[0xff, 0x00],
        b"mds01_inotable",
        b"401.00000001",
    ] {
        let (class, candidates) = classify_cephfs_metadata_object_name(name);
        assert_eq!(class, CephFsMetadataObjectClass::Unknown);
        assert_eq!(candidates.mask(), 0);
    }
}
