use std::fs;

use app_services::analysis_service::{
    get_source_android_device_info, get_source_android_package_summary,
    run_source_android_analysis, AnalysisSourceReadRuntime,
};
use domain::{
    CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType,
    FileEntry, FileEntryId,
};
use persistence_source::fjall::ScanDirection;
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};

#[path = "support/android_fjall_regression.rs"]
mod fjall_regression;

#[test]
fn android_analysis_persists_standard_device_and_package_manager_metadata() {
    let case_root = temporary_directory();
    let evidence_root = temporary_directory();
    write_android_artifacts(evidence_root.path());

    let case_id = CaseId("case-android-analysis".to_string());
    let case_connection = case_connection(&case_id);

    let source = register_ready_source(
        &case_connection,
        &case_id,
        case_root.path(),
        evidence_root.path(),
    );
    let runtime = AnalysisSourceReadRuntime::default();
    let before = source_manifest(case_root.path(), &source.id);
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

    run_source_android_analysis(
        &case_connection,
        case_root.path(),
        &case_id,
        &source.id,
        &runtime,
    )
    .expect("rerun Android analysis");
    let fjall = app_services::source_db::open_registered_fjall_source_read_only(
        case_root.path(),
        &source.id,
    )
    .expect("open Android parser projection");
    let snapshot = fjall.snapshot();
    let parser_records = fjall
        .parser_list_by_family(
            &snapshot,
            &source.id.0,
            "android",
            None,
            ScanDirection::Forward,
            32,
        )
        .expect("read Android parser projection");
    assert_eq!(parser_records.records.len(), 9);
    assert!(parser_records.next_cursor.is_none());
    assert_eq!(source_manifest(case_root.path(), &source.id), before);
}

fn temporary_directory() -> tempfile::TempDir {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../output/repair-2026-11-13/android");
    fs::create_dir_all(&path).expect("create Android test directory");
    tempfile::tempdir_in(path).expect("create isolated Android fixture")
}

fn case_connection(case_id: &CaseId) -> rusqlite::Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&connection).expect("run case migrations");
    CaseRepo::new(&connection)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Android analysis".into(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("create case");
    connection
}

#[test]
fn android_analysis_rejects_missing_generation_manifest_before_writing_results() {
    let case_root = temporary_directory();
    let evidence_root = temporary_directory();
    write_android_artifacts(evidence_root.path());
    let case_id = CaseId("case-android-missing-manifest".into());
    let connection = case_connection(&case_id);
    let source = register_ready_source(
        &connection,
        &case_id,
        case_root.path(),
        evidence_root.path(),
    );
    let generation = case_root
        .path()
        .join("sources")
        .join(&source.id.0)
        .join("source.fjall");
    fs::remove_file(generation.join(persistence_source::fjall::SOURCE_GENERATION_MANIFEST_FILE))
        .unwrap();
    let error = run_source_android_analysis(
        &connection,
        case_root.path(),
        &case_id,
        &source.id,
        &AnalysisSourceReadRuntime::default(),
    )
    .expect_err("missing publication must fail");
    assert!(error.to_string().contains("manifest"));
    let source_connection = app_services::source_db::open_registered_source_db_read_only(
        &connection,
        case_root.path(),
        &source.id,
    )
    .unwrap();
    let store = persistence_source::detail::require_source_store(&source_connection).unwrap();
    let records = persistence_source::fjall::ParserRecordStore::from_source_store((*store).clone());
    assert!(records
        .list_by_family(
            &records.read_snapshot(),
            &source.id.0,
            "android",
            None,
            ScanDirection::Forward,
            100
        )
        .unwrap()
        .records
        .is_empty());
    assert!(
        get_source_android_device_info(&connection, case_root.path(), &case_id, &source.id)
            .is_err()
    );
    assert!(get_source_android_package_summary(
        &connection,
        case_root.path(),
        &case_id,
        &source.id,
        0,
        10,
        &AnalysisSourceReadRuntime::default()
    )
    .is_err());
}

fn source_manifest(
    root: &std::path::Path,
    source_id: &DataSourceId,
) -> persistence_source::fjall::SourceGenerationManifest {
    app_services::source_db::open_registered_fjall_source_read_only(root, source_id)
        .expect("open published Android source")
        .verify_source_generation_with_control(
            &source_id.0,
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        )
        .expect("validate Android generation")
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
    let store = persistence_source::detail::open_store_for_write(&source_connection)
        .expect("open Android builder")
        .expect("Android source has a builder");
    let key = persistence_source::fjall::encode_key(
        "projection",
        &[persistence_source::fjall::KeyPart::Text(
            source.id.0.clone(),
        )],
    )
    .unwrap();
    let marker = persistence_source::fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", source.id.0),
        &source.id.0,
        1,
        source.id.0.as_bytes().to_vec(),
    );
    let receipt = store
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .expect("commit Android projection marker");
    persistence_source::fjall::seal_source_generation_with_control(
        &store,
        &source.id.0,
        "android-fixture",
        &hex::encode(receipt.batch_digest),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .expect("seal Android catalog");
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
