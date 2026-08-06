//! Real-image validation of the emulation preflight (import-catalog driven)
//! and the WinPE maintenance operations (OSDATA removal, Utilman bypass)
//! against files extracted from the image. Evidence is only ever read.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_pipeline::{execute_import_job_with_counts, ImportJobOptions};
use domain::{DataSourceId, DataSourcePlatform};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_EMULATION_TEST_E01")
        .map(PathBuf::from)
        .or_else(testing::fixtures::local_e01_fixture)
        .expect("set FORENSICS_EMULATION_TEST_E01 or FORENSICS_E01_FIXTURE")
}

fn import_image(active: &app_services::active_case::ActiveCase, image: &Path) -> DataSourceId {
    active
        .with_conn(|case_conn| {
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &image.to_string_lossy(),
                DataSourcePlatform::Windows,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let job_id = JobRepo::new(case_conn).create(&active.meta.id.0, "preflight import")?;
            let cancel = Arc::new(AtomicBool::new(false));
            execute_import_job_with_counts(
                case_conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job_id,
                ImportJobOptions {
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            let sources = DataSourceRepo::new(case_conn).find_by_case(&active.meta.id)?;
            assert_eq!(sources.len(), 1, "exactly one source must be registered");
            Ok(sources[0].id.clone())
        })
        .expect("import the real image")
}

#[test]
#[ignore = "requires a real Windows E01 image"]
fn preflight_reads_the_import_catalog_of_a_real_image() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "emulation-preflight",
        Some("tester"),
    )
    .unwrap();
    let data_source_id = import_image(&active, &sample_path());

    let preflight = active
        .with_conn(|case_conn| {
            app_services::mount_service::emulation_preflight(
                case_conn,
                &active.case_root,
                &active.meta.id,
                &data_source_id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("preflight over the import catalog");

    assert!(
        !preflight.installs.is_empty(),
        "a Windows system image must expose at least one installation"
    );
    for install in &preflight.installs {
        eprintln!(
            "install P{}: osdata={} sam={} utilman_bypass={}",
            install.partition_index,
            install.osdata_present,
            install.sam_present,
            install.utilman_bypass_available
        );
        assert!(
            install.sam_present,
            "a Windows install must expose the SAM hive"
        );
    }
    if preflight
        .installs
        .iter()
        .any(|install| !install.utilman_bypass_available)
    {
        active
            .with_conn(|case_conn| {
                let source = app_services::source_db::open_ready_source_read_only_by_id(
                    case_conn,
                    &active.case_root,
                    &active.meta.id,
                    &data_source_id,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                let mut statement = source.connection.prepare(
                    "SELECT path, deleted FROM file_entries
                     WHERE data_source_id = ?1 AND (path LIKE '%utilman%' OR path LIKE '%cmd.exe%')
                     LIMIT 20",
                )?;
                let rows = statement.query_map(rusqlite::params![data_source_id.0], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?;
                for row in rows {
                    let (path, deleted) = row?;
                    eprintln!("catalog match: deleted={deleted} {path}");
                }
                Ok(())
            })
            .unwrap();
    }
    eprintln!(
        "recommended boot route: {:?}, installs={}",
        preflight.recommended_boot_route,
        preflight.installs.len()
    );
}

fn open_windows_fs(image: &Path) -> Option<(fs_ntfs::NtfsReader, u64)> {
    let mut reader = E01Reader::open(image).ok()?;
    let probe = app_services::datasource_service::detect_image_filesystem(&mut reader).ok()?;
    for (ordinal, candidate) in probe.candidates.iter().enumerate() {
        if !matches!(
            candidate.kind,
            app_services::datasource_service::ImageFilesystemKind::Ntfs
        ) {
            continue;
        }
        let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(image).ok()?);
        let Ok(fs) = fs_ntfs::NtfsReader::open(boxed, candidate.offset) else {
            continue;
        };
        let Ok(children) = fs.list_children("") else {
            continue;
        };
        if children
            .iter()
            .any(|child| child.is_dir && child.name.eq_ignore_ascii_case("Windows"))
        {
            eprintln!(
                "Windows volume at candidate #{ordinal}, offset={}",
                candidate.offset
            );
            return Some((fs, candidate.offset));
        }
    }
    None
}

fn extract(fs: &dyn FileSystemReader, volume_root: &Path, relative: &str) -> Option<PathBuf> {
    let mut source = fs.open_file(relative).ok()?;
    let target = volume_root.join(relative);
    std::fs::create_dir_all(target.parent()?).ok()?;
    let mut output = std::fs::File::create(&target).ok()?;
    std::io::copy(&mut source, &mut output).ok()?;
    Some(target)
}

#[test]
#[ignore = "requires a real Windows E01 image"]
fn osdata_removal_and_utilman_bypass_run_on_real_system_files() {
    let image = sample_path();
    let image_size = std::fs::metadata(&image).unwrap().len();
    let (fs, _) = open_windows_fs(&image).expect("the image must expose a Windows NTFS volume");

    let volume = tempfile::TempDir::new().unwrap();
    let root = volume.path();
    extract(&fs, root, "Windows/System32/config/SYSTEM").expect("SYSTEM hive must be readable");
    let config_listing = fs
        .list_children("Windows/System32/config")
        .expect("config directory listing");
    let osdata_node = config_listing
        .iter()
        .find(|node| node.name.eq_ignore_ascii_case("OSDATA"));
    let osdata_on_image = osdata_node.is_some();
    eprintln!("OSDATA present on image: {osdata_on_image}");
    if let Some(node) = osdata_node {
        if node.is_dir {
            let dir = root.join("Windows/System32/config/OSDATA");
            std::fs::create_dir_all(&dir).unwrap();
            if let Ok(children) = fs.list_children("Windows/System32/config/OSDATA") {
                for child in children.iter().filter(|child| !child.is_dir) {
                    extract(
                        &fs,
                        root,
                        &format!("Windows/System32/config/OSDATA/{}", child.name),
                    );
                }
            }
        } else {
            extract(&fs, root, "Windows/System32/config/OSDATA").expect("OSDATA must be readable");
        }
    }
    extract(&fs, root, "Windows/System32/utilman.exe").expect("utilman.exe must be readable");
    extract(&fs, root, "Windows/System32/cmd.exe").expect("cmd.exe must be readable");
    let original_utilman = std::fs::read(root.join("Windows/System32/utilman.exe")).unwrap();

    let located = winpe_maintenance::find_single_windows_installation(vec![root.to_path_buf()])
        .expect("the extracted tree must be detected as a Windows installation");
    assert_eq!(located, root);

    let state = winpe_maintenance::inspect_osdata(root).unwrap();
    eprintln!("inspect_osdata on extracted tree: {state:?}");
    match state {
        winpe_maintenance::OsdataState::File | winpe_maintenance::OsdataState::EmptyDirectory => {
            winpe_maintenance::remove_osdata(root).unwrap();
            assert_eq!(
                winpe_maintenance::inspect_osdata(root).unwrap(),
                winpe_maintenance::OsdataState::Missing,
                "OSDATA must be gone after removal"
            );
        }
        winpe_maintenance::OsdataState::NonEmptyDirectory => {
            assert!(
                winpe_maintenance::remove_osdata(root).is_err(),
                "a non-empty OSDATA directory must be refused"
            );
        }
        winpe_maintenance::OsdataState::Missing => {}
    }

    assert_eq!(
        winpe_maintenance::inspect_bypass(root).unwrap(),
        winpe_maintenance::BypassState::NotApplied
    );
    assert_eq!(
        winpe_maintenance::apply_bypass(root).unwrap(),
        winpe_maintenance::BypassState::Applied
    );
    let cmd_bytes = std::fs::read(root.join("Windows/System32/cmd.exe")).unwrap();
    assert_eq!(
        std::fs::read(root.join("Windows/System32/utilman.exe")).unwrap(),
        cmd_bytes,
        "after apply, utilman.exe must be the command shell"
    );
    assert_eq!(
        winpe_maintenance::restore_bypass(root).unwrap(),
        winpe_maintenance::BypassState::NotApplied
    );
    assert_eq!(
        std::fs::read(root.join("Windows/System32/utilman.exe")).unwrap(),
        original_utilman,
        "restore must bring back the byte-identical utilman.exe"
    );

    assert_eq!(
        std::fs::metadata(&image).unwrap().len(),
        image_size,
        "the evidence image must remain untouched"
    );
}
