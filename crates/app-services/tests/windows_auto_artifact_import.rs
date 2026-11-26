use app_services::{
    active_case, bitlocker_runtime, capability_service, case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job, ImportJobOptions},
    import_precheck::prepare_import_source_config_from_path,
    post_import_service, source_db, task_runtime,
};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_source::repositories::artifact_repo::ArtifactRepo;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::fs;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};

#[path = "support/managed_capabilities.rs"]
mod managed_capabilities;

#[test]
fn windows_import_automatically_extracts_native_artifacts() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.codex-tmp/work");
    fs::create_dir_all(&workspace).unwrap();
    let directory = tempfile::tempdir_in(workspace).unwrap();
    let evidence = directory.path().join("windows-root");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/windows-parsers");
    for (sample, destination) in [
        (
            "notepad-xp.lnk",
            "Users/alice/AppData/Roaming/Microsoft/Windows/Recent/notepad.lnk",
        ),
        ("recycle_v2.bin", "$Recycle.Bin/S-1-5-21-1000/$Iexample.txt"),
        (
            "thumbcache_v21.db",
            "Users/alice/AppData/Local/Microsoft/Windows/Explorer/thumbcache_256.db",
        ),
    ] {
        let path = evidence.join(destination);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(fixtures.join(sample), path).unwrap();
    }
    fs::write(
        evidence.join("Users/alice/AppData/Roaming/Microsoft/Windows/Recent/desktop.ini"),
        "[.ShellClassInfo]\n",
    )
    .unwrap();
    let active = case_service::create_case(
        &directory.path().join("cases"),
        "windows-native-automatic",
        Some("test"),
    )
    .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    active
        .with_conn(|conn| {
            let job = JobRepo::new(conn).create(&active.meta.id.0, "Automatic Windows import")?;
            let config = prepare_import_source_config_from_path(
                &evidence.to_string_lossy(),
                DataSourcePlatform::Windows,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let message = execute_import_job(
                conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: false,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            assert!(message.contains("Timeline:"));
            managed_capabilities::run(&active, None, std::time::Duration::from_secs(30));
            let sources = DataSourceRepo::new(conn).find_by_case(&active.meta.id)?;
            assert_eq!(sources.len(), 1);
            let source = source_db::open_ready_source_by_id(
                conn,
                &active.case_root,
                &active.meta.id,
                &DataSourceId(sources[0].id.0.clone()),
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let artifacts = ArtifactRepo::new(&source.connection);
            for family in ["LNK", "RecycleBin", "Thumbcache"] {
                assert_eq!(artifacts.list_by_family(Some(family))?.len(), 1, "{family}");
            }
            assert!(artifacts.list_by_family(None)?.iter().all(|artifact| {
                artifact
                    .extractor_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("windows.native."))
            }));
            Ok(())
        })
        .unwrap();
}
