use app_services::analysis_service;
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo,
};
use std::io::Read;
use std::path::Path;
use tempfile::TempDir;

fn sample_path() -> std::path::PathBuf {
    std::env::var("FORENSICS_LIUYANG_E01_FIXTURE")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("E:/pangushi/刘洋/liuyang_pc.E01"))
}

fn open_ntfs(path: &Path) -> (fs_ntfs::NtfsReader, u64) {
    let mut reader = E01Reader::open(path).unwrap();
    let probe = app_services::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs_candidate = probe
        .candidates
        .into_iter()
        .find(|c| {
            matches!(
                c.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("no NTFS candidate found");
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(path).unwrap());
    (
        fs_ntfs::NtfsReader::open(boxed, ntfs_candidate.offset).unwrap(),
        ntfs_candidate.offset,
    )
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
        data_source_id: "liuyang-ds".to_string(),
        partition_index: None,
        path: path.to_string(),
        size: 0,
        content_identity: format!("test:{file_id}"),
        evidence_kind: "registry_hive".to_string(),
        parser: "registry.hive".to_string(),
        category: "Registry".to_string(),
    }
}

// Local run:
//   $env:FORENSICS_LIUYANG_E01_FIXTURE='E:/pangushi/刘洋/liuyang_pc.E01'
//   cargo test -p app-services --test liuyang_registry_extract_test -- --ignored --nocapture
#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real E01 sample"]
fn liuyang_registry_extractors_surface_families() {
    let fixture_path = sample_path();
    let (mut fs, offset) = open_ntfs(&fixture_path);
    eprintln!("Liu Yang NTFS offset: {offset}");

    let tmp = TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-registry-direct",
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
                    name: "liuyang-registry".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let system_bytes = read_fs_file(&mut fs, "Windows/System32/config/SYSTEM");
            let boot_key = artifacts_windows::extract_boot_key(&system_bytes);
            let software_bytes = read_fs_file(&mut fs, "Windows/System32/config/SOFTWARE");
            let sam_bytes = read_fs_file(&mut fs, "Windows/System32/config/SAM");
            let security_bytes = read_fs_file(&mut fs, "Windows/System32/config/SECURITY");
            let amcache_bytes = read_fs_file(&mut fs, "Windows/appcompat/Programs/Amcache.hve");

            let mut all_artifacts = Vec::new();

            let system_outcome = analysis_service::extract_registry_candidate(
                &make_candidate("Windows/System32/config/SYSTEM", "system"),
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

            let software_outcome = analysis_service::extract_registry_candidate(
                &make_candidate("Windows/System32/config/SOFTWARE", "software"),
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

            let sam_outcome = analysis_service::extract_registry_candidate(
                &make_candidate("Windows/System32/config/SAM", "sam"),
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

            let security_outcome = analysis_service::extract_registry_candidate(
                &make_candidate("Windows/System32/config/SECURITY", "security"),
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

            let amcache_outcome = analysis_service::extract_registry_candidate(
                &make_candidate("Windows/appcompat/Programs/Amcache.hve", "amcache"),
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

            // Persist full extraction results to output/liuyang_registry_extract.
            let out_dir = std::path::PathBuf::from("output/liuyang_registry_extract");
            std::fs::create_dir_all(&out_dir)?;
            std::fs::write(
                out_dir.join("summary.json"),
                serde_json::to_string_pretty(&summary)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?,
            )?;
            std::fs::write(
                out_dir.join("artifacts_all.json"),
                serde_json::to_string_pretty(&all_artifacts)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?,
            )?;

            let mut by_family: std::collections::BTreeMap<String, Vec<&domain::Artifact>> =
                std::collections::BTreeMap::new();
            for a in &all_artifacts {
                by_family.entry(a.family.clone()).or_default().push(a);
            }
            let mut family_counts = Vec::new();
            for (family, items) in &by_family {
                family_counts.push((family.clone(), items.len()));
                let safe = family
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '_' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect::<String>();
                std::fs::write(
                    out_dir.join(format!("family_{safe}.json")),
                    serde_json::to_string_pretty(&items)
                        .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?,
                )?;
            }
            std::fs::write(
                out_dir.join("family_counts.json"),
                serde_json::to_string_pretty(&family_counts)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?,
            )?;
            eprintln!("Wrote full extraction results to {}", out_dir.display());

            Ok(()) as Result<(), persistence_sqlite::DbError>
        })
        .unwrap();
}
