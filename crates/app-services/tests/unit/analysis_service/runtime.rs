use super::*;

#[test]
fn default_runtime_does_not_invent_bitlocker_capabilities() {
    let runtime = AnalysisSourceReadRuntime::default();
    assert!(runtime.bitlocker_runtime.is_none());
}

#[test]
fn configured_runtime_retains_the_shared_unlock_registry() {
    let registry = Arc::new(BitLockerUnlockRegistry::default());
    let runtime = AnalysisSourceReadRuntime::with_bitlocker_runtime(Arc::clone(&registry));

    assert!(runtime
        .bitlocker_runtime
        .as_ref()
        .is_some_and(|configured| Arc::ptr_eq(configured, &registry)));
}

#[test]
fn artifact_publication_rejects_revoked_import_generation_at_commit() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, [0; 512]).unwrap();
    let mut source = crate::source_runtime::SourceRuntime::new(
        crate::source_runtime::SourceIdentity {
            case_id: temp.path().to_string_lossy().into_owned(),
            data_source_id: domain::DataSourceId("source".into()),
            revision: 0,
            source_kind: domain::DataSourceKind::Raw,
            path: path.clone(),
        },
        crate::source_runtime::open_factory(&path, &domain::DataSourceKind::Raw).unwrap(),
    );
    let runtime = AnalysisSourceReadRuntime::default().with_publication(source.publication_lease());
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(id INTEGER)").unwrap();
    let _fence = runtime.fence(&c);
    c.execute_batch("BEGIN; INSERT INTO output VALUES(1)")
        .unwrap();
    source.bump_generation();
    assert!(c.execute_batch("COMMIT").is_err());
    assert!(runtime.validate_publication().is_err());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM output", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
}
