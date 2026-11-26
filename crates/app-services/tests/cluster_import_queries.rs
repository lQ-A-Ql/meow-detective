use app_services::{
    case_service,
    cluster_service::{
        self, get_linux_evidence_set_summary, write_mdraid_descriptor, ClusterServiceError,
        LinuxImportPhaseInput, MdRaidArrayPlan, MdRaidMemberPlan,
    },
    datasource_service::{LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource},
};
use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, DataSourceProvenanceStatus,
};
use fs_lvm::mdraid::MdRaidSuperblock;
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    linux_import_set_repo::{LinuxImportSetMemberRecord, LinuxImportSetRecord, LinuxImportSetRepo},
    notebook_repo::{NotebookRepo, StepFilters},
};
use rusqlite::Connection;
use serde_json::Value;

#[test]
fn phase_ledger_preserves_results_and_retries_only_its_own_import_set_phase() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "Phase Ledger", None).unwrap();
    let conn = case.connection().unwrap();
    for (set, phase, success, expected_attempt) in [
        ("set-a", "initialize", false, 1),
        ("set-a", "initialize", true, 2),
        ("set-b", "initialize", true, 1),
        ("set-a", "finalize", true, 1),
    ] {
        let error = (!success).then_some("import-failed");
        let step = cluster_service::record_linux_import_phase(
            &conn,
            &case.case_root,
            LinuxImportPhaseInput {
                case_id: &case.meta.id,
                import_set_id: set,
                phase,
                success,
                error,
                ready_count: 2,
                failed_count: u32::from(!success),
            },
        )
        .unwrap();
        let params: Value = serde_json::from_str(&step.params_json).unwrap();
        assert_eq!(params["attempt"], expected_attempt);
        assert_eq!(params["importSetId"], set);
        assert_eq!(params["phase"], phase);
        assert_eq!(params["readyCount"], 2);
        assert_eq!(params["failedCount"], u32::from(!success));
        assert_eq!(params["error"], serde_json::to_value(error).unwrap());
        assert_eq!(step.success, success);
        assert_eq!(step.error_code.as_deref(), error);
        assert!(step.case_state_hash.is_some());
    }
    assert_eq!(
        NotebookRepo::new(&conn)
            .list_steps(&case.meta.id.0, &StepFilters::default())
            .unwrap()
            .len(),
        4
    );
    conn.execute("UPDATE investigation_steps SET params_json='invalid-json' WHERE id=(SELECT id FROM investigation_steps LIMIT 1)", []).unwrap();
    assert!(matches!(
        cluster_service::record_linux_import_phase(
            &conn,
            &case.case_root,
            LinuxImportPhaseInput {
                case_id: &case.meta.id,
                import_set_id: "set-a",
                phase: "initialize",
                success: true,
                error: None,
                ready_count: 2,
                failed_count: 0,
            }
        ),
        Err(ClusterServiceError::Db(_))
    ));
    assert_eq!(
        NotebookRepo::new(&conn)
            .list_steps(&case.meta.id.0, &StepFilters::default())
            .unwrap()
            .len(),
        4,
        "Query failure must not append a false first attempt"
    );
}

fn import_set(conn: &Connection, case_id: &str, id: &str, source: &str) {
    LinuxImportSetRepo::new(conn)
        .insert(&LinuxImportSetRecord {
            id: id.into(),
            case_id: case_id.into(),
            name: id.into(),
            root_path: "evidence".into(),
            import_state: "importing".into(),
            member_count: 2,
            ready_count: 0,
            failed_count: 0,
            last_error: None,
        })
        .unwrap();
    for (index, source) in [Some(source), None].into_iter().enumerate() {
        LinuxImportSetRepo::new(conn)
            .insert_member(&LinuxImportSetMemberRecord {
                import_set_id: id.into(),
                member_index: index as u32,
                source_path: format!("{id}/disk{index}.E01"),
                source_kind: "e01".into(),
                data_source_id: source.map(str::to_owned),
                import_state: "pending".into(),
                last_error: None,
            })
            .unwrap();
    }
}

#[test]
fn member_binding_requires_matching_case_set_index_and_source_ownership() {
    let temporary = tempfile::tempdir().unwrap();
    let first = case_service::create_case(temporary.path(), "First", None).unwrap();
    let second = case_service::create_case(temporary.path(), "Second", None).unwrap();
    let conn = first.connection().unwrap();
    persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
        .create(&second.meta)
        .unwrap();
    for (case, id) in [(&first.meta.id, "source-a"), (&second.meta.id, "source-b")] {
        let source = DataSource {
            id: DataSourceId(id.into()),
            name: id.into(),
            kind: DataSourceKind::E01,
            source_path: temporary.path().join(format!("{id}.E01")),
            imported_at: chrono::Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        DataSourceRepo::new(&conn)
            .insert_with_storage(
                case,
                &source,
                &DataSourceStorage::source_db(id, Some("linux"), None),
            )
            .unwrap();
    }
    import_set(&conn, &first.meta.id.0, "set-a", "source-a");
    import_set(&conn, &second.meta.id.0, "set-b", "source-b");
    let lookup = |case, set, index| {
        cluster_service::get_linux_import_member_source_id(&conn, case, set, index).unwrap()
    };
    assert_eq!(
        lookup(&first.meta.id, "set-a", 0),
        Some(DataSourceId("source-a".into()))
    );
    assert_eq!(
        lookup(&second.meta.id, "set-b", 0),
        Some(DataSourceId("source-b".into()))
    );
    assert_eq!(lookup(&second.meta.id, "set-a", 0), None);
    assert_eq!(lookup(&first.meta.id, "set-a", 1), None);
    assert_eq!(lookup(&first.meta.id, "set-a", 2), None);
    assert_eq!(lookup(&first.meta.id, "set-a' OR 1=1 --", 0), None);
    assert!(matches!(
        cluster_service::get_linux_import_member_source_id(
            &conn,
            &first.meta.id,
            "set-a",
            usize::MAX
        ),
        Err(ClusterServiceError::InvalidMemberIndex)
    ));
    conn.execute("UPDATE linux_import_set_members SET data_source_id='source-b' WHERE import_set_id='set-a' AND member_index=0", []).unwrap();
    assert_eq!(
        lookup(&first.meta.id, "set-a", 0),
        None,
        "A member cannot expose a source from another case"
    );
    assert!(cluster_service::get_linux_import_member_source_id(
        &Connection::open_in_memory().unwrap(),
        &first.meta.id,
        "set-a",
        0
    )
    .is_err());
}

#[test]
fn summary_reads_persisted_mdraid_without_materializing_a_source() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "MD RAID summary", None).unwrap();
    let conn = case.connection().unwrap();
    let import_set_id = "mdraid-summary-set";
    LinuxImportSetRepo::new(&conn)
        .insert(&LinuxImportSetRecord {
            id: import_set_id.into(),
            case_id: case.meta.id.0.clone(),
            name: "MD RAID summary".into(),
            root_path: "evidence".into(),
            import_state: "ready".into(),
            member_count: 1,
            ready_count: 1,
            failed_count: 0,
            last_error: None,
        })
        .unwrap();
    LinuxImportSetRepo::new(&conn)
        .insert_member(&LinuxImportSetMemberRecord {
            import_set_id: import_set_id.into(),
            member_index: 0,
            source_path: "member.raw".into(),
            source_kind: "raw".into(),
            data_source_id: None,
            import_state: "ready".into(),
            last_error: None,
        })
        .unwrap();

    let plan = MdRaidArrayPlan {
        array_uuid: [7; 16],
        name: "array".into(),
        level: 0,
        layout: 1,
        chunk_sectors: 8,
        raid_disks: 1,
        min_data_size_sectors: 16,
        capacity_mismatch: false,
        source_snapshot: Default::default(),
        members: vec![MdRaidMemberPlan {
            superblock: MdRaidSuperblock {
                metadata_version: fs_lvm::mdraid::MdRaidMetadataVersion::V1_2,
                major_version: 1,
                feature_map: 0,
                array_uuid: [7; 16],
                name: "array".into(),
                level: 0,
                layout: 1,
                size_sectors: 16,
                chunk_sectors: 8,
                raid_disks: 1,
                data_offset_sectors: 2048,
                data_size_sectors: 16,
                super_offset_sectors: 8,
                dev_number: 0,
                device_uuid: [8; 16],
                events: 10,
                recovery_offset_sectors: u64::MAX,
                resync_offset_sectors: u64::MAX,
                reshape_position_sectors: u64::MAX,
                dev_roles: vec![0],
                checksum_valid: true,
            },
            origin: app_services::cluster_service::MdRaidMemberOrigin::Lvm {
                vg_name: "vg".into(),
                vg_uuid: "vg-1".into(),
                lv_name: "lv".into(),
                lv_uuid: "lv-1".into(),
                identity: LvmLogicalVolumeIdentity {
                    vg_uuid: "vg-1".into(),
                    vg_name: "vg".into(),
                    lv_uuid: "lv-1".into(),
                    lv_name: "lv".into(),
                    pv_offsets: vec![0],
                    pv_sources: vec![LvmPhysicalVolumeSource {
                        source_path: "member.raw".into(),
                        source_kind: Some(DataSourceKind::Raw),
                        offset: 0,
                        pv_uuid: "pv-1".into(),
                        pv_name: None,
                    }],
                },
            },
        }],
    };
    let descriptor = write_mdraid_descriptor(temporary.path(), &plan).unwrap();
    let fingerprint = evidence_access::mdraid::mdraid_array_fingerprint(&plan).unwrap();
    let source_id = DataSourceId(format!(
        "mdraid-07070707070707070707070707070707-{fingerprint}"
    ));
    let source = DataSource {
        id: source_id.clone(),
        name: "MD RAID array".into(),
        kind: DataSourceKind::MdRaid,
        source_path: descriptor,
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance {
            provenance_status: DataSourceProvenanceStatus::Recorded,
            ..DataSourceProvenance::unknown()
        },
    };
    let mut storage =
        DataSourceStorage::source_db(&source_id.0, Some("linux"), Some("mdraid".into()));
    storage.import_state = "pending".into();
    DataSourceRepo::new(&conn)
        .insert_with_storage(&case.meta.id, &source, &storage)
        .unwrap();

    let summary =
        get_linux_evidence_set_summary(&conn, &case.case_root, &case.meta.id, import_set_id)
            .unwrap();
    assert_eq!(summary.mdraid_arrays.len(), 1);
    assert_eq!(
        summary.mdraid_arrays[0].array_uuid,
        "07070707070707070707070707070707"
    );
    assert!(!app_services::source_db::source_db_path(temporary.path(), &source_id).exists());

    let sibling = format!("{import_set_id}-other");
    LinuxImportSetRepo::new(&conn)
        .insert(&LinuxImportSetRecord {
            id: sibling.clone(),
            case_id: case.meta.id.0.clone(),
            name: sibling.clone(),
            root_path: "other".into(),
            import_state: "ready".into(),
            member_count: 1,
            ready_count: 1,
            failed_count: 0,
            last_error: None,
        })
        .unwrap();
    LinuxImportSetRepo::new(&conn)
        .insert_member(&LinuxImportSetMemberRecord {
            import_set_id: sibling.clone(),
            member_index: 0,
            source_path: "other.raw".into(),
            source_kind: "raw".into(),
            data_source_id: None,
            import_state: "ready".into(),
            last_error: None,
        })
        .unwrap();
    use persistence_sqlite::repositories::linux_topology_scope_repo::{
        LinuxTopologyScopeRecord, LinuxTopologyScopeRepo,
    };
    for set in [import_set_id, sibling.as_str()] {
        for (prefix, kind, suffix) in [("pve", "pve", ""), ("os", "os_instance", ":0")] {
            LinuxTopologyScopeRepo::new(&conn)
                .insert(&LinuxTopologyScopeRecord {
                    id: format!("scope:{prefix}:{set}{suffix}"),
                    case_id: case.meta.id.0.clone(),
                    scope_kind: kind.into(),
                    name: set.into(),
                    identity_state: "candidate".into(),
                    identity_fingerprint: None,
                    status: "partial".into(),
                    evidence_completeness: "partial".into(),
                    diagnostics_json: "[]".into(),
                })
                .unwrap();
        }
    }
    let summary =
        get_linux_evidence_set_summary(&conn, &case.case_root, &case.meta.id, import_set_id)
            .unwrap();
    assert_eq!(summary.scopes.len(), 2);
    assert_eq!(summary.derived_sources.len(), 1);
    let other =
        get_linux_evidence_set_summary(&conn, &case.case_root, &case.meta.id, &sibling).unwrap();
    assert_eq!(other.scopes.len(), 2);
    assert!(other.derived_sources.is_empty());
    assert!(other.mdraid_arrays.is_empty());
    assert_eq!(other.capability_level, "metadata_only");

    let mut outside = plan.clone();
    outside.array_uuid = [9; 16];
    let cluster_service::MdRaidMemberOrigin::Lvm { identity, .. } = &mut outside.members[0].origin
    else {
        panic!("expected logical volume origin")
    };
    identity.pv_sources.push(LvmPhysicalVolumeSource {
        source_path: "other.raw".into(),
        source_kind: Some(DataSourceKind::Raw),
        offset: 0,
        pv_uuid: "pv-other".into(),
        pv_name: None,
    });
    let other_source = DataSource {
        id: DataSourceId("cross-set-array".into()),
        source_path: write_mdraid_descriptor(temporary.path(), &outside).unwrap(),
        ..source.clone()
    };
    let mut other_storage =
        DataSourceStorage::source_db(&other_source.id.0, Some("linux"), Some("mdraid".into()));
    other_storage.import_state = "ready".into();
    DataSourceRepo::new(&conn)
        .insert_with_storage(&case.meta.id, &other_source, &other_storage)
        .unwrap();
    let summary =
        get_linux_evidence_set_summary(&conn, &case.case_root, &case.meta.id, import_set_id)
            .unwrap();
    assert_eq!(summary.derived_sources.len(), 1);
    assert_eq!(summary.mdraid_arrays.len(), 1);
    assert_eq!(summary.capability_level, "metadata_only");
}

#[test]
fn summary_rejects_manifest_contents_with_an_unchanged_digest() {
    let temporary = tempfile::tempdir().unwrap();
    let case = case_service::create_case(temporary.path(), "Manifest verification", None).unwrap();
    let root = temporary.path().join("evidence");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("member-a.raw"), [0; 512]).unwrap();
    std::fs::write(root.join("member-b.raw"), [0; 512]).unwrap();
    let plan = cluster_service::plan_linux_evidence_set_import(&root, None).unwrap();
    let conn = case.connection().unwrap();
    cluster_service::register_linux_evidence_set_import(&conn, &case.meta.id, &plan).unwrap();
    let manifest =
        cluster_service::write_linux_evidence_set_manifest(&case.case_root, &plan).unwrap();
    let summary =
        get_linux_evidence_set_summary(&conn, &case.case_root, &case.meta.id, &plan.import_set_id)
            .unwrap();
    assert!(summary.manifest_digest.is_some());
    let mut document: Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    document["members"][0]["sourceSizeBytes"] = 7.into();
    std::fs::write(&manifest, serde_json::to_vec(&document).unwrap()).unwrap();
    let summary =
        get_linux_evidence_set_summary(&conn, &case.case_root, &case.meta.id, &plan.import_set_id)
            .unwrap();
    assert!(summary.manifest_digest.is_none());
    assert!(summary
        .diagnostics
        .iter()
        .any(|value| value.contains("manifest is missing or invalid")));
}
