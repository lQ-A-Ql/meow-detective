use persistence_sqlite::{
    open_in_memory,
    repositories::ceph_fs_namespace_repo::{
        cephfs_namespace_projection_sha256, CephFsDentryRecord, CephFsFileLayoutRecord,
        CephFsInodeRecord, CephFsNamespaceDiagnosticRecord, CephFsNamespaceManifest,
        CephFsNamespaceProjection, CephFsNamespaceRepo, CephFsNamespaceRepoError,
        CephFsNamespaceWriteOutcome, CEPHFS_NAMESPACE_DECODER_PROFILE,
        CEPHFS_NAMESPACE_SCHEMA_VERSION,
    },
    runner,
};
use rusqlite::Connection;

const SOURCE: &str = "cephfs-derived-a";
const FILESYSTEM: &str = "ceph-fs:cluster-a:1:17:7";

fn setup() -> Connection {
    let conn = open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("run source migrations");
    conn.execute(
        "INSERT INTO data_sources (
             id, case_id, name, kind, source_path, imported_at
         ) VALUES (?1, 'case-1', 'CephFS', 'ceph_fs', 'cephfs://cluster-a/1',
                   '2026-07-20T00:00:00Z')",
        [SOURCE],
    )
    .unwrap();
    conn
}

fn projection(input_sha256: &str, complete: bool) -> CephFsNamespaceProjection {
    let inodes = vec![
        CephFsInodeRecord {
            inode: 1,
            mode: 0o040755,
            uid: 0,
            gid: 0,
            nlink: 2,
            size: 0,
            inode_kind: "directory".to_string(),
            encoded_version: 20,
            remaining_inode_bytes: 0,
        },
        CephFsInodeRecord {
            inode: 2,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            nlink: 1,
            size: 4,
            inode_kind: "file".to_string(),
            encoded_version: 20,
            remaining_inode_bytes: 0,
        },
    ];
    let layouts = vec![
        CephFsFileLayoutRecord {
            inode: 1,
            stripe_unit: 0,
            stripe_count: 0,
            object_size: 0,
            pool_id: -1,
            pool_namespace: String::new(),
            inline_data: None,
            sparse_extents: Vec::new(),
        },
        CephFsFileLayoutRecord {
            inode: 2,
            stripe_unit: 0,
            stripe_count: 0,
            object_size: 0,
            pool_id: -1,
            pool_namespace: String::new(),
            inline_data: Some(b"test".to_vec()),
            sparse_extents: Vec::new(),
        },
    ];
    let dentries = vec![
        CephFsDentryRecord {
            entry_id: "cephfs:root:1".to_string(),
            parent_entry_id: None,
            parent_inode: 0,
            child_inode: 1,
            fragment: 0,
            name: "/".to_string(),
            path: "/".to_string(),
            entry_kind: "directory".to_string(),
            mode: Some(0o040755),
            uid: Some(0),
            gid: Some(0),
            nlink: Some(2),
            size: Some(0),
            alternate_name: String::new(),
        },
        CephFsDentryRecord {
            entry_id: "cephfs:1:0:2:file.txt".to_string(),
            parent_entry_id: Some("cephfs:root:1".to_string()),
            parent_inode: 1,
            child_inode: 2,
            fragment: 0,
            name: "file.txt".to_string(),
            path: "/file.txt".to_string(),
            entry_kind: "file".to_string(),
            mode: Some(0o100644),
            uid: Some(1000),
            gid: Some(1000),
            nlink: Some(1),
            size: Some(4),
            alternate_name: String::new(),
        },
    ];
    let diagnostics = if complete {
        Vec::new()
    } else {
        vec![CephFsNamespaceDiagnosticRecord {
            diagnostic_ordinal: 0,
            diagnostic_kind: "orphan".to_string(),
            parent_inode: 99,
            child_inode: 2,
            name: "lost".to_string(),
            snap_id: None,
        }]
    };
    let mut manifest = CephFsNamespaceManifest {
        filesystem_identity: FILESYSTEM.to_string(),
        data_source_id: SOURCE.to_string(),
        filesystem_id: 1,
        fsmap_epoch: 17,
        root_inode: 1,
        input_sha256: input_sha256.to_string(),
        projection_sha256: String::new(),
        schema_version: CEPHFS_NAMESPACE_SCHEMA_VERSION,
        decoder_profile: CEPHFS_NAMESPACE_DECODER_PROFILE.to_string(),
        completeness: if complete { "closed" } else { "incomplete" }.to_string(),
        published: complete,
        entry_count: dentries.len() as u64,
        inode_count: inodes.len() as u64,
        diagnostic_count: diagnostics.len() as u64,
    };
    manifest.projection_sha256 =
        cephfs_namespace_projection_sha256(&manifest, &inodes, &layouts, &dentries, &diagnostics);
    CephFsNamespaceProjection {
        manifest,
        inodes,
        layouts,
        dentries,
        diagnostics,
    }
}

#[test]
fn namespace_projection_round_trips_and_replaces_idempotently() {
    let conn = setup();
    let repo = CephFsNamespaceRepo::new(&conn);
    let first = projection(&"a".repeat(64), true);
    assert_eq!(
        repo.replace(&first).unwrap(),
        CephFsNamespaceWriteOutcome::Replaced
    );
    assert_eq!(repo.find(FILESYSTEM, SOURCE).unwrap(), Some(first.clone()));
    assert_eq!(
        repo.replace(&first).unwrap(),
        CephFsNamespaceWriteOutcome::Unchanged
    );

    let replacement = projection(&"b".repeat(64), true);
    assert_eq!(
        repo.replace(&replacement).unwrap(),
        CephFsNamespaceWriteOutcome::Replaced
    );
    assert_eq!(repo.find(FILESYSTEM, SOURCE).unwrap(), Some(replacement));
}

#[test]
fn published_verification_rejects_semantically_tampered_rows() {
    let conn = setup();
    let repo = CephFsNamespaceRepo::new(&conn);
    let original = projection(&"a".repeat(64), true);
    repo.replace(&original).unwrap();

    let mut tampered = original.clone();
    tampered.dentries[1].parent_inode = 99;
    tampered.manifest.projection_sha256 = cephfs_namespace_projection_sha256(
        &tampered.manifest,
        &tampered.inodes,
        &tampered.layouts,
        &tampered.dentries,
        &tampered.diagnostics,
    );
    conn.execute(
        "UPDATE ceph_fs_dentries SET parent_inode = ?1 WHERE entry_id = ?2",
        rusqlite::params![
            tampered.dentries[1].parent_inode,
            tampered.dentries[1].entry_id
        ],
    )
    .unwrap();
    conn.execute(
        "UPDATE ceph_fs_namespace_manifests
         SET projection_sha256 = ?1
         WHERE filesystem_identity = ?2 AND data_source_id = ?3",
        rusqlite::params![tampered.manifest.projection_sha256, FILESYSTEM, SOURCE],
    )
    .unwrap();

    assert!(matches!(
        repo.verify_published(FILESYSTEM, SOURCE),
        Err(CephFsNamespaceRepoError::Invalid(_))
    ));
}

#[test]
fn same_input_with_different_rows_is_a_determinism_conflict() {
    let conn = setup();
    let repo = CephFsNamespaceRepo::new(&conn);
    let original = projection(&"a".repeat(64), true);
    repo.replace(&original).unwrap();
    let mut conflicting = original.clone();
    conflicting.dentries[1].name = "changed.txt".to_string();
    conflicting.dentries[1].path = "/changed.txt".to_string();
    conflicting.manifest.projection_sha256 = cephfs_namespace_projection_sha256(
        &conflicting.manifest,
        &conflicting.inodes,
        &conflicting.layouts,
        &conflicting.dentries,
        &conflicting.diagnostics,
    );
    assert!(matches!(
        repo.replace(&conflicting),
        Err(CephFsNamespaceRepoError::DeterminismConflict)
    ));
    assert_eq!(repo.find(FILESYSTEM, SOURCE).unwrap(), Some(original));
}

#[test]
fn published_projection_rejects_cross_row_and_link_count_mismatches() {
    let conn = setup();
    let repo = CephFsNamespaceRepo::new(&conn);

    let mut missing_layout = projection(&"e".repeat(64), true);
    missing_layout.layouts.pop();
    missing_layout.manifest.projection_sha256 = cephfs_namespace_projection_sha256(
        &missing_layout.manifest,
        &missing_layout.inodes,
        &missing_layout.layouts,
        &missing_layout.dentries,
        &missing_layout.diagnostics,
    );
    assert!(matches!(
        repo.replace(&missing_layout),
        Err(CephFsNamespaceRepoError::Invalid(_))
    ));

    let mut bad_link_count = projection(&"f".repeat(64), true);
    bad_link_count.inodes[1].nlink = 2;
    bad_link_count.dentries[1].nlink = Some(2);
    bad_link_count.manifest.projection_sha256 = cephfs_namespace_projection_sha256(
        &bad_link_count.manifest,
        &bad_link_count.inodes,
        &bad_link_count.layouts,
        &bad_link_count.dentries,
        &bad_link_count.diagnostics,
    );
    assert!(matches!(
        repo.replace(&bad_link_count),
        Err(CephFsNamespaceRepoError::Invalid(_))
    ));
}

#[test]
fn incomplete_projection_is_stored_but_not_published() {
    let conn = setup();
    let projection = projection(&"c".repeat(64), false);
    CephFsNamespaceRepo::new(&conn)
        .replace(&projection)
        .unwrap();
    let stored = CephFsNamespaceRepo::new(&conn)
        .find(FILESYSTEM, SOURCE)
        .unwrap()
        .unwrap();
    assert!(!stored.manifest.published);
    assert_eq!(stored.manifest.completeness, "incomplete");
}

#[test]
fn deleting_source_cascades_namespace_projection() {
    let conn = setup();
    let projection = projection(&"d".repeat(64), true);
    CephFsNamespaceRepo::new(&conn)
        .replace(&projection)
        .unwrap();
    conn.execute("DELETE FROM data_sources WHERE id = ?1", [SOURCE])
        .unwrap();
    assert!(CephFsNamespaceRepo::new(&conn)
        .find(FILESYSTEM, SOURCE)
        .unwrap()
        .is_none());
}

fn insert_file_catalog(conn: &Connection) {
    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size)
         VALUES (?1, NULL, ?2, '/', 'CephFS', 'directory', NULL)",
        rusqlite::params!["cephfs:root:1", SOURCE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size)
         VALUES (?1, ?2, ?3, '/file.txt', 'file.txt', 'file', 4)",
        rusqlite::params!["cephfs:1:0:2:file.txt", "cephfs:root:1", SOURCE],
    )
    .unwrap();
}

fn assert_catalog_invalid_after(
    conn: &Connection,
    mutate: impl FnOnce(&Connection),
    restore: impl FnOnce(&Connection),
) {
    mutate(conn);
    assert!(matches!(
        CephFsNamespaceRepo::new(conn).verify_published_catalog(FILESYSTEM, SOURCE, "CephFS"),
        Err(CephFsNamespaceRepoError::Invalid(_))
    ));
    restore(conn);
}

#[test]
fn published_catalog_verification_closes_the_file_entry_boundary() {
    let conn = setup();
    let repo = CephFsNamespaceRepo::new(&conn);
    repo.replace(&projection(&"a".repeat(64), true)).unwrap();
    insert_file_catalog(&conn);

    let verified = repo
        .verify_published_catalog(FILESYSTEM, SOURCE, "CephFS")
        .expect("matching file catalog should verify");
    assert_eq!(verified.summary.file_count, 2);
    assert_eq!(verified.summary.directory_count, 1);
    assert_eq!(verified.summary.total_size, 4);

    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size)
         VALUES ('foreign-entry', NULL, 'foreign-source', '/', 'foreign', 'directory', NULL)",
        [],
    )
    .unwrap();
    assert!(matches!(
        repo.verify_published_catalog(FILESYSTEM, SOURCE, "CephFS"),
        Err(CephFsNamespaceRepoError::Invalid(_))
    ));
    conn.execute("DELETE FROM file_entries WHERE id = 'foreign-entry'", [])
        .unwrap();

    assert_catalog_invalid_after(
        &conn,
        |conn| {
            conn.execute(
                "UPDATE file_entries SET id = 'tampered-id' WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
        |conn| {
            conn.execute(
                "UPDATE file_entries SET id = ?1 WHERE id = 'tampered-id'",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
    );
    assert_catalog_invalid_after(
        &conn,
        |conn| {
            conn.execute(
                "UPDATE file_entries SET parent_id = NULL WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
        |conn| {
            conn.execute(
                "UPDATE file_entries SET parent_id = ?1 WHERE id = ?2",
                rusqlite::params!["cephfs:root:1", "cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
    );
    assert_catalog_invalid_after(
        &conn,
        |conn| {
            conn.execute(
                "UPDATE file_entries SET data_source_id = 'foreign-source' WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
        |conn| {
            conn.execute(
                "UPDATE file_entries SET data_source_id = ?1 WHERE id = ?2",
                rusqlite::params![SOURCE, "cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
    );
    assert_catalog_invalid_after(
        &conn,
        |conn| {
            conn.execute(
                "UPDATE file_entries SET path = '/tampered.txt' WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
        |conn| {
            conn.execute(
                "UPDATE file_entries SET path = '/file.txt' WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
    );
    assert_catalog_invalid_after(
        &conn,
        |conn| {
            conn.execute(
                "UPDATE file_entries SET name = 'tampered.txt' WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
        |conn| {
            conn.execute(
                "UPDATE file_entries SET name = 'file.txt' WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
    );
    assert_catalog_invalid_after(
        &conn,
        |conn| {
            conn.execute(
                "UPDATE file_entries SET entry_type = 'directory' WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
        |conn| {
            conn.execute(
                "UPDATE file_entries SET entry_type = 'file' WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
    );
    assert_catalog_invalid_after(
        &conn,
        |conn| {
            conn.execute(
                "UPDATE file_entries SET size = 5 WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
        |conn| {
            conn.execute(
                "UPDATE file_entries SET size = 4 WHERE id = ?1",
                ["cephfs:1:0:2:file.txt"],
            )
            .unwrap();
        },
    );
}
