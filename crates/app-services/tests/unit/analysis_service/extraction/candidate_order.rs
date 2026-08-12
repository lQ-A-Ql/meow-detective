use super::*;
use domain::FileEntryId;
use persistence_sqlite::repositories::filesystem_locator_repo::{
    FilesystemFileLocatorRecord, FilesystemLocatorRepo,
};

const CATALOG_FINGERPRINT: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn candidate(id: &str, partition_index: Option<usize>, path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(id.to_string()),
        data_source_id: "derived-linux".to_string(),
        partition_index,
        path: path.to_string(),
        size: 32,
        encrypted: false,
        content_identity: format!("test:{id}"),
        modified_at: None,
        evidence_kind: "linux".to_string(),
        parser: "linux.web".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

fn derived_source_connection() -> Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES ('derived-linux', 'case-1', 'VM root', 'ceph_rbd', 'derived.rbd',
                     '2026-07-18T00:00:00Z')",
            [],
        )
        .expect("insert derived source");
    for partition_index in [1, 2] {
        connection
            .execute(
                "INSERT INTO data_source_partitions
                 (id, data_source_id, partition_index, name, kind_label, status,
                  offset, length, filesystem)
                 VALUES (?1, 'derived-linux', ?2, ?3, 'XFS', 'ready', 0, 4096, 'XFS')",
                rusqlite::params![
                    format!("partition-{partition_index}"),
                    partition_index,
                    format!("Partition {partition_index}")
                ],
            )
            .expect("insert XFS partition");
    }
    let manifest = serde_json::json!({
        "materializerVersion": crate::derived_source_catalog::CATALOG_MATERIALIZER_VERSION,
        "inputFingerprint": CATALOG_FINGERPRINT,
        "recordCount": 0,
        "directoryCount": 0,
        "totalSize": 0,
        "createdCount": 0,
        "modifiedCount": 0,
        "accessedCount": 0,
        "changedCount": 0,
        "catalogDigest": "",
        "partitionCount": 2,
        "partitionDigest": "",
    });
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES ('derived.catalog.manifest', ?1)",
            [manifest.to_string()],
        )
        .expect("insert derived Catalog manifest");
    connection
}

fn locator_scope(partition_index: usize) -> String {
    crate::file_service::filesystem_locators::derived_filesystem_locator_scope(
        CATALOG_FINGERPRINT,
        &crate::file_service::PreviewPartitionCandidate {
            partition_index,
            filesystem_kind: "XFS".to_string(),
            offset: 0,
            lvm_identity: None,
        },
    )
    .expect("build locator scope")
}

#[test]
fn derived_xfs_order_uses_partition_inode_then_path_without_dropping_candidates() {
    let connection = derived_source_connection();
    let repo = FilesystemLocatorRepo::new(&connection);
    repo.replace_file_locators(
        "derived-linux",
        1,
        "XFS",
        &locator_scope(1),
        &[FilesystemFileLocatorRecord {
            path: "var/www/first.php".to_string(),
            locator: "500".to_string(),
        }],
    )
    .expect("persist partition one locator");
    repo.replace_file_locators(
        "derived-linux",
        2,
        "XFS",
        &locator_scope(2),
        &[
            FilesystemFileLocatorRecord {
                path: "var/www/a.php".to_string(),
                locator: "20".to_string(),
            },
            FilesystemFileLocatorRecord {
                path: "var/www/invalid.php".to_string(),
                locator: "not-an-inode".to_string(),
            },
            FilesystemFileLocatorRecord {
                path: "var/www/z.php".to_string(),
                locator: "10".to_string(),
            },
        ],
    )
    .expect("persist partition two locators");
    let mut candidates = vec![
        candidate("missing", Some(2), "var/www/missing.php"),
        candidate("inode-20", Some(2), "var/www/a.php"),
        candidate("null-partition", None, "var/www/zero.php"),
        candidate("partition-one", Some(1), "var/www/first.php"),
        candidate("invalid-locator", Some(2), "var/www/invalid.php"),
        candidate("inode-10", Some(2), "var/www/z.php"),
    ];
    let original_ids = candidates
        .iter()
        .map(|candidate| candidate.file_id.0.clone())
        .collect::<BTreeSet<_>>();

    order_candidates_for_extraction(&connection, DataSourcePlatform::Linux, &mut candidates);

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.file_id.0.as_str())
            .collect::<Vec<_>>(),
        vec![
            "partition-one",
            "inode-10",
            "inode-20",
            "invalid-locator",
            "missing",
            "null-partition",
        ]
    );
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.file_id.0.clone())
            .collect::<BTreeSet<_>>(),
        original_ids
    );
}
