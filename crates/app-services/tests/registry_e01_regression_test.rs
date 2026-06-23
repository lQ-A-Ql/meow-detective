use app_services::analysis_service;
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use std::io::Read;
use std::path::Path;
use tempfile::TempDir;

fn sample_path() -> std::path::PathBuf {
    std::env::var("FORENSICS_JC2_E01_FIXTURE")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("D:/獬豸杯/检材2.E01"))
}

const MAIN_NTFS_OFFSET: u64 = 608_174_080;

fn open_ntfs(path: &Path, offset: u64) -> fs_ntfs::NtfsReader {
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(path).unwrap());
    fs_ntfs::NtfsReader::open(boxed, offset).unwrap()
}

fn read_fs_file(fs: &mut dyn FileSystemReader, path: &str) -> Vec<u8> {
    let mut file = fs
        .open_file(path)
        .unwrap_or_else(|e| panic!("open {path}: {e}"));
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    buf
}

fn make_candidate(path: &str, file_id: &str) -> app_services::analysis_service::EvidenceCandidate {
    app_services::analysis_service::EvidenceCandidate {
        file_id: domain::FileEntryId(file_id.to_string()),
        data_source_id: "jc2-ds".to_string(),
        path: path.to_string(),
        size: 0,
        evidence_kind: "registry_hive".to_string(),
        parser: "registry.hive".to_string(),
        category: "Registry".to_string(),
    }
}

// Local run example:
//   $env:FORENSICS_JC2_E01_FIXTURE='D:/獬豸杯/检材2.E01'
//   cargo test -p app-services --test registry_e01_regression_test -- --ignored --nocapture
#[test]
#[ignore = "requires FORENSICS_JC2_E01_FIXTURE real E01 sample"]
fn jc2_registry_extractors_surface_new_families() {
    let fixture_path = sample_path();
    let mut fs = open_ntfs(&fixture_path, MAIN_NTFS_OFFSET);

    let tmp = TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &tmp.path().join("cases"),
        "jc2-registry-direct",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "jc2-registry".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Pre-load SYSTEM hive bytes so SAM can derive the BootKey.
            let system_bytes = read_fs_file(&mut fs, "Windows/System32/config/SYSTEM");
            let boot_key = artifacts_windows::extract_boot_key(&system_bytes);
            let software_bytes = read_fs_file(&mut fs, "Windows/System32/config/SOFTWARE");
            let sam_bytes = read_fs_file(&mut fs, "Windows/System32/config/SAM");
            let security_bytes = read_fs_file(&mut fs, "Windows/System32/config/SECURITY");
            let amcache_bytes = read_fs_file(&mut fs, "Windows/appcompat/Programs/Amcache.hve");

            let mut all_artifacts = Vec::new();

            let system_candidate = make_candidate(
                "Windows/System32/config/SYSTEM",
                "jc2-system",
            );
            let system_outcome = analysis_service::extract_registry_candidate(
                &system_candidate,
                &system_bytes,
                None,
                None,
                None,
            );
            eprintln!(
                "SYSTEM: {} artifacts, {} warnings",
                system_outcome.artifacts.len(),
                system_outcome.warnings.len()
            );
            all_artifacts.extend(system_outcome.artifacts);

            let software_candidate = make_candidate(
                "Windows/System32/config/SOFTWARE",
                "jc2-software",
            );
            let software_outcome = analysis_service::extract_registry_candidate(
                &software_candidate,
                &software_bytes,
                None,
                None,
                None,
            );
            eprintln!(
                "SOFTWARE: {} artifacts, {} warnings",
                software_outcome.artifacts.len(),
                software_outcome.warnings.len()
            );
            all_artifacts.extend(software_outcome.artifacts);

            let sam_candidate = make_candidate("Windows/System32/config/SAM", "jc2-sam");
            let sam_outcome = analysis_service::extract_registry_candidate(
                &sam_candidate,
                &sam_bytes,
                boot_key,
                None,
                None,
            );
            eprintln!(
                "SAM: {} artifacts, {} warnings",
                sam_outcome.artifacts.len(),
                sam_outcome.warnings.len()
            );
            all_artifacts.extend(sam_outcome.artifacts);

            let security_candidate = make_candidate(
                "Windows/System32/config/SECURITY",
                "jc2-security",
            );
            let security_outcome = analysis_service::extract_registry_candidate(
                &security_candidate,
                &security_bytes,
                boot_key,
                None,
                None,
            );
            eprintln!(
                "SECURITY: {} artifacts, {} warnings",
                security_outcome.artifacts.len(),
                security_outcome.warnings.len()
            );
            all_artifacts.extend(security_outcome.artifacts);

            let amcache_candidate = make_candidate(
                "Windows/appcompat/Programs/Amcache.hve",
                "jc2-amcache",
            );
            let amcache_outcome = analysis_service::extract_registry_candidate(
                &amcache_candidate,
                &amcache_bytes,
                None,
                None,
                None,
            );
            eprintln!(
                "Amcache: {} artifacts, {} warnings",
                amcache_outcome.artifacts.len(),
                amcache_outcome.warnings.len()
            );
            all_artifacts.extend(amcache_outcome.artifacts);

            if !all_artifacts.is_empty() {
                ArtifactRepo::new(conn)
                    .insert_batch(&all_artifacts, &case_id.0, &data_source_id.0)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            }

            let summary = analysis_service::get_registry_structured_summary(conn)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            eprintln!(
                "Structured summary: services={} usb={} mounted={} shutdown={} shimcache={} run_keys={} winlogon={} lsa_packages={} network={} shellbags={} muicache={} amcache_apps={} amcache_files={} appcompat={} sec_policy={} lsa_secrets={} cached_creds={}",
                summary.system_services.len(),
                summary.usb_devices.len(),
                summary.mounted_devices.len(),
                summary.shutdown_times.len(),
                summary.shimcache_entries.len(),
                summary.run_keys.len(),
                summary.winlogon_config.as_ref().map(|_| 1).unwrap_or(0),
                summary.lsa_packages.len(),
                summary.network_profiles.len(),
                summary.shellbag_entries.len(),
                summary.muicache_entries.len(),
                summary.amcache_applications.len(),
                summary.amcache_application_files.len(),
                summary.appcompat_layers.len(),
                summary.security_policies.len(),
                summary.lsa_secrets.len(),
                summary.cached_credentials.len(),
            );

            assert!(
                !summary.system_services.is_empty(),
                "SYSTEM hive should expose services"
            );
            assert!(
                !summary.mounted_devices.is_empty(),
                "SYSTEM hive should expose mounted devices"
            );
            assert!(
                !summary.lsa_packages.is_empty(),
                "SYSTEM hive should expose LSA packages"
            );
            assert!(
                !summary.run_keys.is_empty(),
                "SOFTWARE hive should expose machine Run keys"
            );
            assert!(
                summary.winlogon_config.is_some(),
                "SOFTWARE hive should expose Winlogon config"
            );
            assert!(
                !summary.network_profiles.is_empty(),
                "SOFTWARE hive should expose NetworkList profiles"
            );

            Ok(())
        })
        .unwrap();
}
