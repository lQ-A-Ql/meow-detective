use std::fs;

use app_services::analysis_service::{
    get_source_android_device_info, get_source_android_package_summary,
    run_source_android_analysis, AnalysisSourceReadRuntime,
};
use domain::{
    CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType,
    FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};

#[test]
fn android_analysis_persists_standard_device_and_package_manager_metadata() {
    let case_root = tempfile::TempDir::new().expect("case root");
    let evidence_root = tempfile::TempDir::new().expect("logical evidence root");
    write_android_artifacts(evidence_root.path());

    let case_connection = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&case_connection).expect("run case migrations");
    let case_id = CaseId("case-android-analysis".to_string());
    CaseRepo::new(&case_connection)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Android analysis".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("create case");

    let source = register_ready_source(
        &case_connection,
        &case_id,
        case_root.path(),
        evidence_root.path(),
    );
    let runtime = AnalysisSourceReadRuntime::default();
    let run = run_source_android_analysis(
        &case_connection,
        case_root.path(),
        &case_id,
        &source.id,
        &runtime,
    )
    .expect("run Android analysis");
    assert_eq!(run.device_fact_count, 7);
    assert_eq!(run.package_count, 2);

    let device =
        get_source_android_device_info(&case_connection, case_root.path(), &case_id, &source.id)
            .expect("read device facts");
    assert_eq!(device.model.as_deref(), Some("Pixel Test"));
    assert_eq!(device.android_version.as_deref(), Some("15"));
    assert_eq!(device.android_id.as_deref(), Some("abcdef0123456789"));
    assert_eq!(device.imei, None);
    assert!(device
        .facts
        .iter()
        .any(|fact| fact.field == "deviceName" && fact.value == "Forensic Pixel"));

    let packages = get_source_android_package_summary(
        &case_connection,
        case_root.path(),
        &case_id,
        &source.id,
        0,
        50,
        &runtime,
    )
    .expect("read package metadata");
    assert_eq!(packages.total_count, 2);
    assert_eq!(packages.packages[0].package_name, "org.example.app");
    assert_eq!(packages.packages[0].version_code.as_deref(), Some("42"));
    assert_eq!(packages.packages[0].uid, Some(10_042));
    assert!(packages.packages[0].install_time.is_some());
    assert_eq!(packages.packages[0].user_id, Some(0));
    assert_eq!(packages.packages[0].user_state.as_deref(), Some("disabled"));
    assert!(packages
        .packages
        .iter()
        .any(|package| package.package_name == "org.example.listonly"));
}

fn register_ready_source(
    case_connection: &rusqlite::Connection,
    case_id: &CaseId,
    case_root: &std::path::Path,
    evidence_root: &std::path::Path,
) -> DataSource {
    let source = DataSource {
        id: DataSourceId("android-source".to_string()),
        name: "Android fixture".to_string(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: evidence_root.to_path_buf(),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source.id.0, Some("android"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(case_connection)
        .insert_with_storage(case_id, &source, &storage)
        .expect("register source");

    let source_connection = app_services::source_db::open_source_db(case_root, &source.id)
        .expect("open source database");
    DataSourceRepo::new(&source_connection)
        .upsert_source_local_metadata(case_id, &source)
        .expect("persist source metadata");
    FileRepo::new(&source_connection)
        .insert_batch(&[
            fixture_file(evidence_root, "build-prop", "system/build.prop"),
            fixture_file(
                evidence_root,
                "settings",
                "system/users/0/settings_secure.xml",
            ),
            fixture_file(
                evidence_root,
                "settings-global",
                "system/users/0/settings_global.xml",
            ),
            fixture_file(evidence_root, "packages", "system/packages.xml"),
            fixture_file(evidence_root, "packages-list", "system/packages.list"),
            fixture_file(
                evidence_root,
                "package-restrictions",
                "system/users/0/package-restrictions.xml",
            ),
        ])
        .expect("catalog fixture files");
    drop(source_connection);
    source
}

fn fixture_file(root: &std::path::Path, id: &str, path: &str) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: None,
        data_source_id: DataSourceId("android-source".to_string()),
        path: path.to_string(),
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        entry_type: EntryType::File,
        size: Some(
            fs::metadata(root.join(path))
                .expect("read fixture metadata")
                .len(),
        ),
        ext: path
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_string()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: true,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn write_android_artifacts(root: &std::path::Path) {
    let system = root.join("system");
    let settings = root.join("system/users/0");
    let package_dir = root.join("system");
    fs::create_dir_all(&system).expect("create system directory");
    fs::create_dir_all(&settings).expect("create settings directory");
    fs::create_dir_all(&package_dir).expect("create package directory");
    fs::write(
        system.join("build.prop"),
        "ro.product.model=Pixel Test\nro.product.manufacturer=Google\nro.build.version.release=15\nro.build.version.sdk=35\nro.build.id=AP3A\n",
    )
    .expect("write properties");
    fs::write(
        settings.join("settings_secure.xml"),
        "<settings><setting name=\"android_id\" value=\"abcdef0123456789\"/></settings>",
    )
    .expect("write settings");
    fs::write(
        settings.join("settings_global.xml"),
        "<settings><setting name=\"device_name\" value=\"Forensic Pixel\"/></settings>",
    )
    .expect("write global settings");
    fs::write(
        package_dir.join("packages.xml"),
        "<packages><package name=\"org.example.app\" codePath=\"/data/app/org.example.app\" version=\"42\" userId=\"10042\" it=\"18d3e123456\" ut=\"18d4e123456\"/></packages>",
    )
    .expect("write packages");
    fs::write(
        package_dir.join("packages.list"),
        "org.example.app 10042 0 /data/user/0/org.example.app\norg.example.listonly 10043 0 /data/user/0/org.example.listonly\n",
    )
    .expect("write packages list");
    fs::write(
        settings.join("package-restrictions.xml"),
        "<package-restrictions><pkg name=\"org.example.app\" installed=\"true\" enabled=\"2\"/></package-restrictions>",
    )
    .expect("write package restrictions");
}
