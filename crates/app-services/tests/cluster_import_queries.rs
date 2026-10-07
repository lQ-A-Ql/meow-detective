use app_services::{
    case_service,
    cluster_service::{self, ClusterServiceError, LinuxImportPhaseInput},
};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
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
