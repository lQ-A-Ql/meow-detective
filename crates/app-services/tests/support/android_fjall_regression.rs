use app_services::{
    analysis_service::{
        get_source_android_device_info, get_source_android_package_summary,
        run_source_android_analysis, AnalysisSourceReadRuntime,
    },
    source_db,
};
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_source::fjall::{ParserRecord, ParserRecordStore, PersistMode, ScanDirection};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use transport::dto::{AnalysisParseStatusDto, AndroidDeviceInfoDto, AndroidPackageSummaryDto};

struct Fixture {
    connection: rusqlite::Connection,
    root: PathBuf,
    evidence: PathBuf,
    case: CaseId,
    source: DataSourceId,
    _temporary: tempfile::TempDir,
}

impl Fixture {
    fn new(configure: impl FnOnce(&Path)) -> Self {
        let scratch =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/android");
        fs::create_dir_all(&scratch).unwrap();
        let temporary = tempfile::tempdir_in(scratch).unwrap();
        let root = temporary.path().join("case");
        let evidence = temporary.path().join("evidence");
        fs::create_dir_all(root.join("artifacts/objects")).unwrap();
        fs::create_dir_all(evidence.join("system/users/0")).unwrap();
        fs::write(
            evidence.join("system/build.prop"),
            "ro.product.model=Primary Model\nro.product.manufacturer=Maker\n",
        )
        .unwrap();
        fs::write(
            evidence.join("system/users/0/settings_secure.xml"),
            "<settings><setting name=\"android_id\" value=\"abc123\"/></settings>",
        )
        .unwrap();
        fs::write(
            evidence.join("system/users/0/settings_global.xml"),
            "<settings><setting name=\"device_name\" value=\"Android fixture\"/></settings>",
        )
        .unwrap();
        fs::write(evidence.join("system/packages.xml"), "<packages><package name=\"org.example.app\" codePath=\"/data/app/org.example.app\" version=\"42\" userId=\"10042\"/></packages>").unwrap();
        fs::write(evidence.join("system/packages.list"), "org.example.app 10042 0 /data/user/0/org.example.app\norg.example.old 10043 0 /data/user/0/org.example.old\n").unwrap();
        fs::write(evidence.join("system/users/0/package-restrictions.xml"), "<package-restrictions><pkg name=\"org.example.app\" installed=\"true\" enabled=\"2\"/></package-restrictions>").unwrap();
        configure(&evidence);
        let connection = persistence_sqlite::open_or_create(&root.join("app.db")).unwrap();
        persistence_sqlite::runner::run_all(&connection).unwrap();
        let case = CaseId("android-fjall-case".into());
        CaseRepo::new(&connection)
            .create(&CaseMeta {
                id: case.clone(),
                name: "Android".into(),
                number: None,
                examiner: None,
                notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .unwrap();
        let source = DataSource {
            id: DataSourceId("android-fjall-source".into()),
            name: "Android".into(),
            kind: DataSourceKind::LogicalDirectory,
            source_path: evidence.clone(),
            imported_at: chrono::Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(&source.id.0, Some("android"), None);
        storage.import_state = "ready".into();
        DataSourceRepo::new(&connection)
            .insert_with_storage(&case, &source, &storage)
            .unwrap();
        let source_connection = source_db::open_source_db(&root, &source.id).unwrap();
        DataSourceRepo::new(&source_connection)
            .upsert_source_local_metadata(&case, &source)
            .unwrap();
        let reader = evidence_core::LogicalFsReader::open(&evidence, "Android").unwrap();
        app_services::file_service::enumerate_filesystem(&source_connection, &source.id, &reader)
            .unwrap();
        let store = persistence_source::detail::require_source_store(&source_connection).unwrap();
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
                PersistMode::SyncAll,
            )
            .unwrap();
        persistence_source::fjall::seal_source_generation_with_control(
            &store,
            &source.id.0,
            "android-fixture",
            &hex::encode(receipt.batch_digest),
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        Self {
            connection,
            root,
            evidence,
            case,
            source: source.id,
            _temporary: temporary,
        }
    }

    fn run(&self) {
        run_source_android_analysis(
            &self.connection,
            &self.root,
            &self.case,
            &self.source,
            &AnalysisSourceReadRuntime::default(),
        )
        .unwrap();
    }

    fn device(&self) -> AndroidDeviceInfoDto {
        get_source_android_device_info(&self.connection, &self.root, &self.case, &self.source)
            .unwrap()
    }

    fn packages(&self) -> AndroidPackageSummaryDto {
        get_source_android_package_summary(
            &self.connection,
            &self.root,
            &self.case,
            &self.source,
            0,
            20,
            &AnalysisSourceReadRuntime::default(),
        )
        .unwrap()
    }

    fn store(&self) -> ParserRecordStore {
        let handle =
            source_db::open_registered_fjall_source_read_write(&self.root, &self.source).unwrap();
        ParserRecordStore::from_source_store(handle.source_store().clone())
    }

    fn overwrite(&self, relative: &str, content: &[u8]) {
        let path = self.evidence.join(relative);
        let size = fs::metadata(&path).unwrap().len() as usize;
        assert!(content.len() <= size);
        let mut bytes = vec![b' '; size];
        bytes[..content.len()].copy_from_slice(content);
        fs::write(path, bytes).unwrap();
    }
}

#[test]
fn android_reanalysis_replaces_removed_facts_and_packages_and_can_publish_empty_results() {
    let fixture = Fixture::new(|_| {});
    assert_eq!(fixture.device().status, AnalysisParseStatusDto::NotParsed);
    assert_eq!(fixture.packages().total_count, 0);
    fixture.run();
    assert_eq!(fixture.device().facts.len(), 4);
    let packages = fixture.packages();
    assert_eq!(packages.total_count, 2);
    assert_eq!(packages.packages[0].user_id, Some(0));
    assert_eq!(packages.packages[0].user_state.as_deref(), Some("disabled"));
    let first_page = get_source_android_package_summary(
        &fixture.connection,
        &fixture.root,
        &fixture.case,
        &fixture.source,
        0,
        1,
        &AnalysisSourceReadRuntime::default(),
    )
    .unwrap();
    let second_page = get_source_android_package_summary(
        &fixture.connection,
        &fixture.root,
        &fixture.case,
        &fixture.source,
        1,
        1,
        &AnalysisSourceReadRuntime::default(),
    )
    .unwrap();
    assert_eq!(first_page.total_count, 2);
    assert_eq!(second_page.total_count, 2);
    assert_eq!(first_page.packages.len(), 1);
    assert_eq!(second_page.packages.len(), 1);
    assert_eq!(first_page.packages[0].package_name, "org.example.app");
    assert_eq!(second_page.packages[0].package_name, "org.example.old");
    assert!(!first_page.packages[0].source_file_id.is_empty());
    assert_eq!(first_page.packages[0].source_path, "system/packages.xml");
    fixture.overwrite("system/build.prop", b"ro.product.model=Only Model\n");
    fixture.overwrite(
        "system/users/0/settings_secure.xml",
        b"<settings></settings>",
    );
    fixture.overwrite(
        "system/users/0/settings_global.xml",
        b"<settings></settings>",
    );
    fixture.overwrite("system/packages.list", b"");
    fixture.run();
    assert_eq!(fixture.device().facts.len(), 1);
    assert_eq!(fixture.device().model.as_deref(), Some("Only Model"));
    assert_eq!(fixture.packages().total_count, 1);
    fixture.overwrite("system/build.prop", b"");
    fixture.overwrite("system/packages.xml", b"<packages></packages>");
    fixture.run();
    assert_eq!(fixture.device().status, AnalysisParseStatusDto::NotParsed);
    assert!(fixture.device().facts.is_empty());
    assert_eq!(fixture.packages().total_count, 0);
    let store = fixture.store();
    assert!(store
        .list_by_family(
            &store.read_snapshot(),
            &fixture.source.0,
            "android",
            None,
            ScanDirection::Forward,
            20
        )
        .unwrap()
        .records
        .is_empty());
}

#[test]
fn android_queries_ignore_other_families_with_the_same_kind_and_reject_bad_payloads() {
    let fixture = Fixture::new(|_| {});
    fixture.run();
    let store = fixture.store();
    for kind in ["system_fact", "package"] {
        let payload = serde_json::json!({});
        store
            .upsert(
                &ParserRecord {
                    id: format!("other-{kind}"),
                    data_source_id: fixture.source.0.clone(),
                    family: "other".into(),
                    kind: kind.into(),
                    status: "ready".into(),
                    summary_sha256: infrastructure::hashing::sha256_bytes(
                        payload.to_string().as_bytes(),
                    ),
                    payload,
                },
                PersistMode::SyncAll,
            )
            .unwrap();
    }
    assert_eq!(fixture.device().facts.len(), 4);
    assert_eq!(fixture.packages().total_count, 2);
    let mut fact = store
        .list_by_kind(
            &store.read_snapshot(),
            &fixture.source.0,
            "system_fact",
            None,
            ScanDirection::Forward,
            20,
        )
        .unwrap()
        .records
        .into_iter()
        .find(|record| record.family == "android")
        .unwrap();
    fact.payload = serde_json::json!({});
    fact.summary_sha256 =
        infrastructure::hashing::sha256_bytes(fact.payload.to_string().as_bytes());
    store.upsert(&fact, PersistMode::SyncAll).unwrap();
    assert!(get_source_android_device_info(
        &fixture.connection,
        &fixture.root,
        &fixture.case,
        &fixture.source
    )
    .is_err());
    let mut package = store
        .list_by_family(
            &store.read_snapshot(),
            &fixture.source.0,
            "android",
            None,
            ScanDirection::Forward,
            20,
        )
        .unwrap()
        .records
        .into_iter()
        .find(|record| record.kind == "package")
        .unwrap();
    package.payload = serde_json::json!({"packageName":"org.invalid", "uid":"invalid",
        "sourceFileId":"file", "sourcePath":"system/packages.xml", "parser":"android.system-packages.v1"});
    package.summary_sha256 =
        infrastructure::hashing::sha256_bytes(package.payload.to_string().as_bytes());
    store.upsert(&package, PersistMode::SyncAll).unwrap();
    assert!(get_source_android_package_summary(
        &fixture.connection,
        &fixture.root,
        &fixture.case,
        &fixture.source,
        0,
        20,
        &AnalysisSourceReadRuntime::default()
    )
    .is_err());
}

#[test]
fn android_property_precedence_and_source_file_attribution_are_preserved() {
    let fixture = Fixture::new(|root| {
        fs::create_dir(root.join("vendor")).unwrap();
        fs::write(
            root.join("vendor/build.prop"),
            "ro.product.model=Vendor Model\nro.product.brand=Vendor Brand\n",
        )
        .unwrap();
    });
    fixture.run();
    let device = fixture.device();
    assert_eq!(device.model.as_deref(), Some("Primary Model"));
    let model = device
        .facts
        .iter()
        .find(|fact| fact.field == "model")
        .unwrap();
    assert_eq!(model.source_path, "system/build.prop");
    let store = fixture.store();
    let records = store
        .list_by_kind(
            &store.read_snapshot(),
            &fixture.source.0,
            "system_fact",
            None,
            ScanDirection::Forward,
            20,
        )
        .unwrap()
        .records;
    let model = records
        .iter()
        .find(|record| {
            record
                .payload
                .get("field")
                .and_then(serde_json::Value::as_str)
                == Some("model")
        })
        .unwrap();
    assert_eq!(
        model
            .payload
            .get("sourceRank")
            .and_then(serde_json::Value::as_i64),
        Some(0)
    );
}
