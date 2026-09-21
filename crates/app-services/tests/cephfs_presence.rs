use std::path::{Path, PathBuf};

use app_services::ceph_reconstruction::{
    assess_cephfs_presence, assess_cephfs_presence_for_scope, CephFsFilesystemPresenceRecord,
    CephFsMapPresenceSnapshot, CephFsMdsFilesystemPresenceRecord, CephFsMdsMapPresenceSnapshot,
    CephFsPresenceAssessment, CephFsPresenceDiagnostic, CephFsPresenceEvidence,
    CephFsPresenceMapKind, CephFsPresenceState, FSMAP_PRESENCE_KEY, MDSMAP_PRESENCE_KEY,
};
use chrono::Utc;
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    linux_topology_membership_repo::{LinuxTopologyMembershipRecord, LinuxTopologyMembershipRepo},
    linux_topology_scope_repo::{LinuxTopologyScopeRecord, LinuxTopologyScopeRepo},
};
use rusqlite::{params, OpenFlags};

const CAPTURED_AT: &str = "2026-07-19T00:00:00Z";

fn fsmap(
    source_id: &str,
    epoch: u64,
    filesystems: Vec<CephFsFilesystemPresenceRecord>,
) -> CephFsMapPresenceSnapshot {
    CephFsMapPresenceSnapshot {
        schema_version: 1,
        cluster_identity: "ceph-cluster-a".to_string(),
        source_identity: source_id.to_string(),
        inventory_identity: format!("inventory-{source_id}"),
        epoch,
        captured_at: CAPTURED_AT.to_string(),
        filesystems,
    }
}

fn mdsmap(
    source_id: &str,
    fsmap_epoch: u64,
    epoch: u64,
    filesystems: Vec<CephFsMdsFilesystemPresenceRecord>,
) -> CephFsMdsMapPresenceSnapshot {
    CephFsMdsMapPresenceSnapshot {
        schema_version: 1,
        cluster_identity: "ceph-cluster-a".to_string(),
        source_identity: source_id.to_string(),
        inventory_identity: format!("inventory-{source_id}"),
        fsmap_epoch,
        epoch,
        captured_at: CAPTURED_AT.to_string(),
        filesystems,
    }
}

fn evidence(
    source_id: &str,
    fsmap: CephFsMapPresenceSnapshot,
    mdsmap: CephFsMdsMapPresenceSnapshot,
) -> CephFsPresenceEvidence {
    CephFsPresenceEvidence::new(source_id, Some(fsmap), Some(mdsmap))
}

fn valid_filesystem() -> CephFsFilesystemPresenceRecord {
    filesystem_with_id(1)
}

fn filesystem_with_id(filesystem_id: u64) -> CephFsFilesystemPresenceRecord {
    CephFsFilesystemPresenceRecord {
        filesystem_id,
        metadata_pool_id: 10,
        data_pool_ids: vec![11],
    }
}

fn valid_mds_filesystem() -> CephFsMdsFilesystemPresenceRecord {
    mds_filesystem_with_id(1)
}

fn mds_filesystem_with_id(filesystem_id: u64) -> CephFsMdsFilesystemPresenceRecord {
    CephFsMdsFilesystemPresenceRecord {
        filesystem_id,
        rank_count: 0,
    }
}

#[test]
fn complete_empty_maps_prove_cephfs_absent() {
    let assessment = assess_cephfs_presence(
        &[
            evidence(
                "source-a",
                fsmap("source-a", 7, Vec::new()),
                mdsmap("source-a", 7, 9, Vec::new()),
            ),
            evidence(
                "source-b",
                fsmap("source-b", 7, Vec::new()),
                mdsmap("source-b", 7, 9, Vec::new()),
            ),
        ],
        2,
    );

    assert_eq!(assessment.state, CephFsPresenceState::Absent);
    assert_eq!(assessment.filesystem_count, 0);
    assert_eq!(
        assessment.cluster_identity.as_deref(),
        Some("ceph-cluster-a")
    );
    assert_eq!(assessment.source_ids, ["source-a", "source-b"]);
    assert!(assessment.filesystems.is_empty());
    assert!(assessment.diagnostics.is_empty());
}

#[test]
fn complete_non_empty_maps_prove_cephfs_present_even_without_active_mds() {
    let assessment = assess_cephfs_presence(
        &[
            evidence(
                "source-a",
                fsmap("source-a", 7, vec![valid_filesystem()]),
                mdsmap("source-a", 7, 9, vec![valid_mds_filesystem()]),
            ),
            evidence(
                "source-b",
                fsmap("source-b", 7, vec![valid_filesystem()]),
                mdsmap("source-b", 7, 9, vec![valid_mds_filesystem()]),
            ),
        ],
        2,
    );

    assert_eq!(assessment.state, CephFsPresenceState::Present);
    assert_eq!(assessment.filesystem_count, 1);
    assert_eq!(
        assessment.cluster_identity.as_deref(),
        Some("ceph-cluster-a")
    );
    assert_eq!(assessment.filesystems, vec![valid_filesystem()]);
    assert_eq!(assessment.fsmap_epoch, Some(7));
    assert_eq!(assessment.mdsmap_epoch, Some(9));
}

#[test]
fn duplicate_or_overlapping_pool_bindings_are_indeterminate() {
    let mut filesystem = valid_filesystem();
    filesystem.data_pool_ids = vec![11, 11, 10];
    let assessment = assess_cephfs_presence(
        &[evidence(
            "source-a",
            fsmap("source-a", 7, vec![filesystem]),
            mdsmap("source-a", 7, 9, vec![valid_mds_filesystem()]),
        )],
        1,
    );

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            CephFsPresenceDiagnostic::InvalidFilesystemBinding {
                filesystem_id: 1,
                ..
            }
        )
    }));
}

#[test]
fn missing_snapshot_is_indeterminate() {
    let assessment =
        assess_cephfs_presence(&[CephFsPresenceEvidence::new("source-a", None, None)], 1);

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(diagnostic, CephFsPresenceDiagnostic::MissingSnapshot { .. })
    }));
}

#[test]
fn freshness_failure_is_indeterminate() {
    let mut invalid_fsmap = fsmap("source-a", 7, vec![valid_filesystem()]);
    invalid_fsmap.captured_at = "not-a-timestamp".to_string();
    let assessment = assess_cephfs_presence(
        &[evidence(
            "source-a",
            invalid_fsmap,
            mdsmap("source-a", 7, 9, vec![valid_mds_filesystem()]),
        )],
        1,
    );

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            CephFsPresenceDiagnostic::FreshnessUnproven { .. }
        )
    }));
}

#[test]
fn incomplete_source_set_is_indeterminate() {
    let assessment = assess_cephfs_presence(
        &[
            evidence(
                "source-a",
                fsmap("source-a", 7, vec![valid_filesystem()]),
                mdsmap("source-a", 7, 9, vec![valid_mds_filesystem()]),
            ),
            evidence(
                "source-b",
                fsmap("source-b", 7, vec![valid_filesystem()]),
                mdsmap("source-b", 7, 9, vec![valid_mds_filesystem()]),
            ),
        ],
        3,
    );

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            CephFsPresenceDiagnostic::SourceSetIncomplete { .. }
        )
    }));
}

#[test]
fn duplicate_source_evidence_is_indeterminate() {
    let source = evidence(
        "source-a",
        fsmap("source-a", 7, vec![valid_filesystem()]),
        mdsmap("source-a", 7, 9, vec![valid_mds_filesystem()]),
    );
    let assessment = assess_cephfs_presence(&[source.clone(), source], 2);

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            CephFsPresenceDiagnostic::SourceSetIncomplete {
                expected: 2,
                observed: 1,
            }
        )
    }));
}

#[test]
fn conflicting_closed_source_set_is_indeterminate() {
    let mut conflicting = fsmap("source-b", 8, vec![valid_filesystem()]);
    conflicting.cluster_identity = "other-cluster".to_string();
    let assessment = assess_cephfs_presence(
        &[
            evidence(
                "source-a",
                fsmap("source-a", 7, vec![valid_filesystem()]),
                mdsmap("source-a", 7, 9, vec![valid_mds_filesystem()]),
            ),
            evidence(
                "source-b",
                conflicting,
                mdsmap("source-b", 8, 10, vec![valid_mds_filesystem()]),
            ),
        ],
        2,
    );

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            CephFsPresenceDiagnostic::ConflictingClusterIdentity { .. }
                | CephFsPresenceDiagnostic::ConflictingMapEpoch { .. }
        )
    }));
}

#[test]
fn conflicting_closed_source_filesystem_sets_are_indeterminate() {
    let assessment = assess_cephfs_presence(
        &[
            evidence(
                "source-a",
                fsmap(
                    "source-a",
                    7,
                    vec![filesystem_with_id(2), filesystem_with_id(1)],
                ),
                mdsmap(
                    "source-a",
                    7,
                    9,
                    vec![mds_filesystem_with_id(2), mds_filesystem_with_id(1)],
                ),
            ),
            evidence(
                "source-b",
                fsmap(
                    "source-b",
                    7,
                    vec![filesystem_with_id(3), filesystem_with_id(1)],
                ),
                mdsmap(
                    "source-b",
                    7,
                    9,
                    vec![mds_filesystem_with_id(3), mds_filesystem_with_id(1)],
                ),
            ),
        ],
        2,
    );

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    for map in [CephFsPresenceMapKind::Fsmap, CephFsPresenceMapKind::Mdsmap] {
        assert!(assessment.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic,
                CephFsPresenceDiagnostic::ConflictingFilesystemSet {
                    source_id,
                    map: diagnostic_map,
                    expected,
                    observed,
                } if source_id == "source-b"
                    && *diagnostic_map == map
                    && expected == &vec![1, 2]
                    && observed == &vec![1, 3]
            )
        }));
    }
}

#[test]
fn missing_mds_binding_is_indeterminate() {
    let assessment = assess_cephfs_presence(
        &[evidence(
            "source-a",
            fsmap("source-a", 7, vec![valid_filesystem()]),
            mdsmap("source-a", 7, 9, Vec::new()),
        )],
        1,
    );

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            CephFsPresenceDiagnostic::MissingMdsBinding { filesystem_id: 1 }
        )
    }));
}

#[test]
fn cluster_assessment_reads_real_source_meta_and_stays_indeterminate_without_snapshots() {
    let case_root = tempfile::TempDir::new().expect("case root");
    let case_conn = persistence_sqlite::open_in_memory().expect("case database");
    persistence_sqlite::runner::run_all(&case_conn).expect("case migrations");
    let case_id = CaseId("case-cephfs-presence".to_string());
    CaseRepo::new(&case_conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "CephFS presence test".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .expect("create case");

    let ceph_scope_id = "scope:ceph:presence";
    let scope_repo = LinuxTopologyScopeRepo::new(&case_conn);
    let membership_repo = LinuxTopologyMembershipRepo::new(&case_conn);
    scope_repo
        .insert(&LinuxTopologyScopeRecord {
            id: ceph_scope_id.to_string(),
            case_id: case_id.0.clone(),
            scope_kind: "ceph".to_string(),
            name: "Ceph".to_string(),
            identity_state: "unproven".to_string(),
            identity_fingerprint: None,
            status: "ready".to_string(),
            evidence_completeness: "complete".to_string(),
            diagnostics_json: "[]".to_string(),
        })
        .expect("register Ceph scope");

    let source_id = DataSourceId("source-cephfs-presence".to_string());
    let source = DataSource {
        id: source_id.clone(),
        name: "source".to_string(),
        kind: DataSourceKind::E01,
        source_path: PathBuf::from("source.E01"),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage =
        DataSourceStorage::source_db(&source_id.0, Some("linux"), Some("ceph_osd".to_string()));
    storage.import_state = "ready_metadata".to_string();
    DataSourceRepo::new(&case_conn)
        .insert_with_storage(&case_id, &source, &storage)
        .expect("register source");
    membership_repo
        .insert(&LinuxTopologyMembershipRecord {
            scope_id: ceph_scope_id.to_string(),
            data_source_id: source_id.0.clone(),
            role: "storage_node".to_string(),
            member_index: Some(0),
            confidence: "candidate".to_string(),
            provenance_json: "{}".to_string(),
        })
        .expect("bind source to Ceph scope");
    drop(app_services::source_db::open_source_db(
        case_root.path(),
        &source_id,
    ));

    let assessment = assess_cephfs_presence_for_scope(
        &case_conn,
        case_root.path(),
        &case_id,
        &domain::CephScopeId(ceph_scope_id.to_string()),
    )
    .expect("assess CephFS presence");

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(diagnostic, CephFsPresenceDiagnostic::MissingSnapshot { .. })
    }));
    let stored_kinds = DataSourceRepo::new(&case_conn)
        .find_by_case(&case_id)
        .expect("list sources")
        .into_iter()
        .map(|source| source.kind)
        .collect::<Vec<_>>();
    assert_eq!(stored_kinds, vec![DataSourceKind::E01]);
}

#[test]
fn presence_snapshot_keys_are_stable_contract_values() {
    assert_eq!(FSMAP_PRESENCE_KEY, "ceph.fsmap.presence.v1");
    assert_eq!(MDSMAP_PRESENCE_KEY, "ceph.mdsmap.presence.v1");
}

#[test]
fn presence_assessment_serializes_and_round_trips() {
    let assessment = assess_cephfs_presence(
        &[evidence(
            "source-a",
            fsmap("source-a", 7, Vec::new()),
            mdsmap("source-a", 7, 9, Vec::new()),
        )],
        1,
    );
    let encoded = serde_json::to_string(&assessment).expect("serialize assessment");
    let decoded: CephFsPresenceAssessment =
        serde_json::from_str(&encoded).expect("deserialize assessment");

    assert_eq!(decoded, assessment);
}

#[test]
#[ignore = "requires a retained PVE cluster case database"]
fn retained_pve_cluster_has_no_cephfs_presence_proof() {
    let case_root = std::env::var_os("FORENSICS_PVE_RBD_CASE_ROOT")
        .map(PathBuf::from)
        .expect("FORENSICS_PVE_RBD_CASE_ROOT must point to a retained PVE case");
    assert!(case_root.join("app.db").is_file());
    let case_conn = rusqlite::Connection::open_with_flags(
        case_root.join("app.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("open retained case database read-only");
    case_conn
        .execute_batch("PRAGMA query_only=ON; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
        .expect("configure retained case read-only connection");
    let case_id = CaseRepo::new(&case_conn)
        .list_all()
        .expect("list retained cases")
        .into_iter()
        .next()
        .expect("retained case");
    let ceph_scope_id: String = case_conn
        .query_row(
            "SELECT id FROM linux_topology_scopes
             WHERE case_id = ?1 AND scope_kind = 'ceph' ORDER BY id LIMIT 1",
            params![case_id.id.0],
            |row| row.get(0),
        )
        .expect("retained cluster");
    let source_count_before: i64 = case_conn
        .query_row("SELECT COUNT(*) FROM data_sources", [], |row| row.get(0))
        .expect("count retained sources before assessment");

    let assessment = assess_cephfs_presence_for_scope(
        &case_conn,
        Path::new(&case_root),
        &case_id.id,
        &domain::CephScopeId(ceph_scope_id),
    )
    .expect("assess retained cluster");

    assert_eq!(assessment.state, CephFsPresenceState::Indeterminate);
    assert!(assessment.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            CephFsPresenceDiagnostic::MissingSnapshot { .. }
                | CephFsPresenceDiagnostic::SourceUnavailable { .. }
        )
    }));
    let source_count_after: i64 = case_conn
        .query_row("SELECT COUNT(*) FROM data_sources", [], |row| row.get(0))
        .expect("count retained sources after assessment");
    let cephfs_source_count: i64 = case_conn
        .query_row(
            "SELECT COUNT(*) FROM data_sources WHERE kind = 'ceph_fs'",
            [],
            |row| row.get(0),
        )
        .expect("count retained CephFS sources");
    assert_eq!(source_count_after, source_count_before);
    assert_eq!(cephfs_source_count, 0);
}
