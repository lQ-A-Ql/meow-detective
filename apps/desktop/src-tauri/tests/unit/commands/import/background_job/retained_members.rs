use super::*;

#[test]
#[ignore = "requires an explicit disposable retained PVE case with one failed RBD output"]
fn real_pve_remove_failed_rbd_output() {
    let case_root = std::env::var_os(PVE_RBD_CASE_ROOT_ENV)
        .map(PathBuf::from)
        .expect("set FORENSICS_PVE_RBD_CASE_ROOT to a disposable retained PVE case")
        .canonicalize()
        .expect("resolve disposable retained case");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.codex-tmp")
        .canonicalize()
        .expect("resolve test workspace");
    assert!(case_root.starts_with(workspace));
    let connection = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("open disposable retained PVE case");
    let cases = CaseRepo::new(&connection)
        .list_all()
        .expect("read retained case");
    assert_eq!(cases.len(), 1);
    let sources = DataSourceRepo::new(&connection)
        .find_by_case(&cases[0].id)
        .expect("read retained sources");
    let failed = sources
        .iter()
        .filter(|source| source.kind == domain::DataSourceKind::CephRbd)
        .collect::<Vec<_>>();
    assert_eq!(failed.len(), 1);
    let source = failed[0];
    let storage = DataSourceRepo::new(&connection)
        .find_storage(&source.id)
        .expect("read failed RBD state")
        .expect("failed RBD storage");
    assert_eq!(storage.import_state, "failed");
    let preserved = sources
        .iter()
        .filter(|entry| entry.id != source.id)
        .map(|entry| entry.id.0.clone())
        .collect::<HashSet<_>>();
    assert_eq!(preserved.len(), PVE_MEMBER_COUNT + PVE_HOST_COUNT);
    app_services::case_service::delete_data_source_in(&connection, &case_root, &source.id.0)
        .expect("remove failed RBD output through source lifecycle");
    let remaining = DataSourceRepo::new(&connection)
        .find_by_case(&cases[0].id)
        .expect("read preserved sources")
        .into_iter()
        .map(|entry| entry.id.0)
        .collect::<HashSet<_>>();
    assert_eq!(remaining, preserved);
    eprintln!(
        "PVE_FAILED_OUTPUT_REMOVED source={} preservedSources=9",
        source.id.0
    );
}

#[test]
#[ignore = "requires a retained PVE case with imported member and logical source databases"]
fn real_pve_retained_member_catalogs_and_previews() {
    init_test_tracing();
    let case_root = std::env::var_os(PVE_RBD_CASE_ROOT_ENV)
        .map(PathBuf::from)
        .expect("set FORENSICS_PVE_RBD_CASE_ROOT to a retained PVE case");
    let connection =
        persistence_sqlite::connection::open_existing_case_read_only(&case_root.join("app.db"))
            .expect("open retained PVE control database read-only");
    let cases = CaseRepo::new(&connection)
        .list_all()
        .expect("read retained PVE case");
    assert_eq!(cases.len(), 1);
    let case_id = &cases[0].id;
    let mut statement = connection
        .prepare("SELECT id, name, root_path FROM linux_import_sets WHERE case_id = ?1")
        .expect("prepare retained import-set query");
    let sets = statement
        .query_map([&case_id.0], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("query retained import set")
        .collect::<Result<Vec<_>, _>>()
        .expect("read retained import set");
    assert_eq!(sets.len(), 1);
    let (import_set_id, import_set_name, root_path) = sets.into_iter().next().unwrap();
    app_services::cluster_service::validate_linux_evidence_set_manifest(&case_root, &import_set_id)
        .expect("verify retained import manifest");
    let manifest_rel_path = format!("import-sets/{import_set_id}/import-set-manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(case_root.join(&manifest_rel_path)).expect("read retained manifest"),
    )
    .expect("decode retained manifest");
    let members = manifest["members"]
        .as_array()
        .expect("read retained manifest members")
        .iter()
        .cloned()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()
        .expect("decode retained member identities");
    let skipped_entries = serde_json::from_value(manifest["skippedEntries"].clone())
        .expect("decode retained skipped entries");
    let plan = LinuxEvidenceSetImportPlan {
        import_set_id,
        import_set_name,
        root_path: root_path.into(),
        manifest_rel_path,
        members,
        skipped_entries,
    };
    assert_control_database_is_tree_free(&connection);
    assert_manifest(&case_root, &plan);
    assert_member_storage_and_content(&connection, &case_root, case_id, &plan);
    eprintln!("PVE_RETAINED_MEMBERS physical=6 logicalLvm=3 catalogAndPreview=verified");
}
