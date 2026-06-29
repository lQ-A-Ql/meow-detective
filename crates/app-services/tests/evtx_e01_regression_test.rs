use app_services::analysis_service::{extract_evtx_candidate, get_evtx_event_summary};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo,
};
use std::io::Read;
use std::path::Path;
use tempfile::TempDir;

fn jc2_sample_path() -> std::path::PathBuf {
    std::env::var("FORENSICS_JC2_E01_FIXTURE")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("D:/獬豸杯/检材2.E01"))
}

const JC2_MAIN_NTFS_OFFSET: u64 = 608_174_080;

fn liuyang_sample_path() -> std::path::PathBuf {
    testing::fixtures::local_liuyang_e01_fixture().unwrap_or_else(|| {
        panic!("set FORENSICS_LIUYANG_E01_FIXTURE to run ignored Liu Yang EVTX tests")
    })
}

fn open_ntfs_reader(path: &Path, offset: u64) -> fs_ntfs::NtfsReader {
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(path).unwrap());
    fs_ntfs::NtfsReader::open(boxed, offset).unwrap()
}

fn read_fs_file(fs: &mut fs_ntfs::NtfsReader, path: &str) -> Vec<u8> {
    let mut file = fs
        .open_file(path)
        .unwrap_or_else(|e| panic!("open {path}: {e}"));
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    buf
}

fn make_evtx_candidate(
    path: &str,
    file_id: &str,
) -> app_services::analysis_service::EvidenceCandidate {
    app_services::analysis_service::EvidenceCandidate {
        file_id: domain::FileEntryId(file_id.to_string()),
        data_source_id: "evtx-ds".to_string(),
        path: path.to_string(),
        size: 0,
        evidence_kind: "evtx_log".to_string(),
        parser: "evtx.structured".to_string(),
        category: "EventLogs".to_string(),
    }
}

fn extract_and_store_evtx(
    fs: &mut fs_ntfs::NtfsReader,
    path: &str,
    file_id: &str,
    all_artifacts: &mut Vec<domain::Artifact>,
    all_timeline: &mut Vec<domain::TimelineEvent>,
) {
    let bytes = read_fs_file(fs, path);
    let candidate = make_evtx_candidate(path, file_id);
    let outcome = extract_evtx_candidate(&candidate, &bytes);

    eprintln!(
        "EVTX {path}: bytes={} artifacts={} timeline={} warnings={}",
        bytes.len(),
        outcome.artifacts.len(),
        outcome.timeline_events.len(),
        outcome.warnings.len()
    );
    for warning in &outcome.warnings {
        eprintln!("  warning: {warning}");
    }

    all_artifacts.extend(outcome.artifacts);
    all_timeline.extend(outcome.timeline_events);
}

// Local run example:
//   $env:FORENSICS_JC2_E01_FIXTURE='D:/獬豸杯/检材2.E01'
//   cargo test -p app-services --test evtx_e01_regression_test -- --ignored --nocapture
#[test]
#[ignore = "requires FORENSICS_JC2_E01_FIXTURE real E01 sample"]
fn jc2_evtx_extraction_surfaces_boot_and_security_events() {
    let fixture_path = jc2_sample_path();
    let mut fs = open_ntfs_reader(&fixture_path, JC2_MAIN_NTFS_OFFSET);

    let tmp = TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &tmp.path().join("cases"),
        "jc2-evtx-direct",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "jc2-evtx".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let mut all_artifacts = Vec::new();
            let mut all_timeline = Vec::new();

            extract_and_store_evtx(
                &mut fs,
                "Windows/System32/winevt/Logs/System.evtx",
                "jc2-system-evtx",
                &mut all_artifacts,
                &mut all_timeline,
            );
            extract_and_store_evtx(
                &mut fs,
                "Windows/System32/winevt/Logs/Security.evtx",
                "jc2-security-evtx",
                &mut all_artifacts,
                &mut all_timeline,
            );
            extract_and_store_evtx(
                &mut fs,
                "Windows/System32/winevt/Logs/Application.evtx",
                "jc2-application-evtx",
                &mut all_artifacts,
                &mut all_timeline,
            );

            assert!(
                !all_artifacts.is_empty(),
                "jc2 EVTX extraction should produce at least one artifact"
            );
            assert!(
                !all_timeline.is_empty(),
                "jc2 EVTX extraction should produce at least one timeline event"
            );

            ArtifactRepo::new(conn)
                .insert_batch(&all_artifacts, &case_id.0, &data_source_id.0)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let summary = get_evtx_event_summary(conn, 0, 100)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            eprintln!(
                "jc2 EVTX summary: total={} boot={} security={} application={}",
                summary.total_count,
                summary.boot_shutdown_count,
                summary.security_events.len(),
                summary.application_events.len()
            );

            assert!(
                summary.total_count > 0,
                "jc2 EVTX summary should report at least one event"
            );
            assert!(
                summary.boot_shutdown_count > 0,
                "jc2 System.evtx should produce boot/shutdown events"
            );

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_evtx_extraction_surfaces_boot_and_security_events() {
    let fixture_path = liuyang_sample_path();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = app_services::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| {
            matches!(
                c.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("Liu Yang sample should include a readable NTFS candidate");

    let mut fs = open_ntfs_reader(&fixture_path, ntfs.offset);

    let tmp = TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-evtx-direct",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-evtx".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let mut all_artifacts = Vec::new();
            let mut all_timeline = Vec::new();

            extract_and_store_evtx(
                &mut fs,
                "Windows/System32/winevt/Logs/System.evtx",
                "liuyang-system-evtx",
                &mut all_artifacts,
                &mut all_timeline,
            );
            extract_and_store_evtx(
                &mut fs,
                "Windows/System32/winevt/Logs/Security.evtx",
                "liuyang-security-evtx",
                &mut all_artifacts,
                &mut all_timeline,
            );
            extract_and_store_evtx(
                &mut fs,
                "Windows/System32/winevt/Logs/Application.evtx",
                "liuyang-application-evtx",
                &mut all_artifacts,
                &mut all_timeline,
            );

            assert!(
                !all_artifacts.is_empty(),
                "Liu Yang EVTX extraction should produce at least one artifact"
            );
            assert!(
                !all_timeline.is_empty(),
                "Liu Yang EVTX extraction should produce at least one timeline event"
            );

            ArtifactRepo::new(conn)
                .insert_batch(&all_artifacts, &case_id.0, &data_source_id.0)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let summary = get_evtx_event_summary(conn, 0, 100)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            eprintln!(
                "Liu Yang EVTX summary: total={} boot={} security={} application={}",
                summary.total_count,
                summary.boot_shutdown_count,
                summary.security_events.len(),
                summary.application_events.len()
            );

            assert!(
                summary.total_count > 0,
                "Liu Yang EVTX summary should report at least one event"
            );
            assert!(
                summary.boot_shutdown_count > 0,
                "Liu Yang System.evtx should produce boot/shutdown events"
            );

            Ok(())
        })
        .unwrap();
}
