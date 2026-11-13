use app_services::{
    analysis_service::{
        get_source_android_device_info, get_source_android_package_summary,
        AnalysisSourceReadRuntime,
    },
    case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job, ImportJobOptions},
    import_precheck::{prepare_import_source_config_from_path, ImportSourceMode},
    source_db,
};
use domain::{DataSourceKind, DataSourcePlatform, FileEntryId};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
use transport::dto::AnalysisParseStatusDto;

const INPUTS: &[(&str, &str)] = &[
    (
        "system/build.prop",
        "ro.product.model=Pixel Test\nro.product.manufacturer=Google\nro.build.version.release=15\nro.build.version.sdk=35\nro.build.id=AP3A\n",
    ),
    (
        "system/users/0/settings_secure.xml",
        "<settings><setting name=\"android_id\" value=\"abcdef0123456789\"/></settings>",
    ),
    (
        "system/users/0/settings_global.xml",
        "<settings><setting name=\"device_name\" value=\"Forensic Pixel\"/></settings>",
    ),
    (
        "system/packages.xml",
        "<packages><package name=\"org.example.app\" codePath=\"/data/app/org.example.app\" version=\"42\" userId=\"10042\" it=\"18d3e123456\" ut=\"18d4e123456\"/></packages>",
    ),
    (
        "system/packages.list",
        "org.example.app 10042 0 /data/user/0/org.example.app\norg.example.listonly 10043 0 /data/user/0/org.example.listonly\n",
    ),
    (
        "system/users/0/package-restrictions.xml",
        "<package-restrictions><pkg name=\"org.example.app\" installed=\"true\" enabled=\"2\"/></package-restrictions>",
    ),
];

#[test]
fn android_logical_import_automatically_populates_device_and_package_queries() {
    let workspace =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.codex-tmp/android-auto-import");
    fs::create_dir_all(&workspace).expect("create test workspace");
    let directory = tempfile::tempdir_in(workspace).expect("create isolated import workspace");
    let evidence = directory.path().join("android-root");
    for (relative, content) in INPUTS {
        let path = evidence.join(relative);
        fs::create_dir_all(path.parent().expect("input parent")).expect("create input directory");
        fs::write(path, content).expect("write Android system input");
    }
    let config = prepare_import_source_config_from_path(
        &evidence.to_string_lossy(),
        DataSourcePlatform::Android,
    )
    .expect("prepare Android logical-directory import");
    assert_eq!(config.kind, DataSourceKind::LogicalDirectory);
    assert_eq!(config.platform, DataSourcePlatform::Android);
    assert!(matches!(config.mode, ImportSourceMode::LogicalDirectory));

    let active = case_service::create_case(
        &directory.path().join("cases"),
        "android-automatic-analysis",
        Some("test"),
    )
    .expect("create case");
    let cancel = Arc::new(AtomicBool::new(false));
    active
        .with_conn(|conn| {
            let job = JobRepo::new(conn).create(&active.meta.id.0, "Automatic Android import")?;
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
            assert!(
                message.contains("Android metadata auto-analyzed: facts=7 packages=2"),
                "{message}"
            );
            let sources = DataSourceRepo::new(conn).find_by_case(&active.meta.id)?;
            assert_eq!(sources.len(), 1);
            let source_id = &sources[0].id;
            assert_eq!(sources[0].kind, DataSourceKind::LogicalDirectory);
            let ready = source_db::open_ready_source_by_id(
                conn,
                &active.case_root,
                &active.meta.id,
                source_id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(ready.platform, DataSourcePlatform::Android);
            let device =
                get_source_android_device_info(conn, &active.case_root, &active.meta.id, source_id)
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(device.status, AnalysisParseStatusDto::Parsed);
            assert_eq!(device.model.as_deref(), Some("Pixel Test"));
            assert_eq!(device.manufacturer.as_deref(), Some("Google"));
            assert_eq!(device.android_version.as_deref(), Some("15"));
            assert_eq!(device.sdk_int.as_deref(), Some("35"));
            assert_eq!(device.build_id.as_deref(), Some("AP3A"));
            assert_eq!(device.android_id.as_deref(), Some("abcdef0123456789"));
            assert_eq!(device.imei, None);
            assert_eq!(device.facts.len(), 7);
            assert!(device
                .facts
                .iter()
                .any(|fact| fact.field == "deviceName" && fact.value == "Forensic Pixel"));
            let files = FileRepo::new(&ready.connection);
            for fact in &device.facts {
                assert!(files
                    .find_by_id(&FileEntryId(fact.source_file_id.clone()))?
                    .is_some());
            }
            let packages = get_source_android_package_summary(
                conn,
                &active.case_root,
                &active.meta.id,
                source_id,
                0,
                50,
                &AnalysisSourceReadRuntime::default(),
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(packages.total_count, 2);
            assert_eq!(packages.packages.len(), 2);
            let application = packages
                .packages
                .iter()
                .find(|package| package.package_name == "org.example.app")
                .expect("PackageManager application");
            assert_eq!(application.version_code.as_deref(), Some("42"));
            assert_eq!(application.uid, Some(10_042));
            assert_eq!(application.user_id, Some(0));
            assert_eq!(application.user_state.as_deref(), Some("disabled"));
            assert!(packages
                .packages
                .iter()
                .any(|package| package.package_name == "org.example.listonly"));
            for package in &packages.packages {
                assert!(files
                    .find_by_id(&FileEntryId(package.source_file_id.clone()))?
                    .is_some());
            }
            Ok(())
        })
        .expect("import Android source and read automatic results");
    for (relative, content) in INPUTS {
        assert_eq!(
            fs::read(evidence.join(relative)).unwrap(),
            content.as_bytes()
        );
    }
}
