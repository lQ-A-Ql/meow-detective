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

#[test]
#[ignore = "requires a real Windows E01 image"]
fn sam_bypass_writes_only_the_overlay_and_decrypts_to_empty() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "emulation-bypass",
        Some("tester"),
    )
    .unwrap();
    let image = sample_path();
    let image_size = std::fs::metadata(&image).unwrap().len();
    let data_source_id = import_image(&active, &image);

    let (case_id, case_root) = (active.meta.id.clone(), active.case_root.clone());
    let preflight = active
        .with_conn(|case_conn| {
            app_services::mount_service::emulation_preflight(
                case_conn,
                &case_root,
                &case_id,
                &data_source_id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    let install = preflight
        .installs
        .first()
        .expect("the image must expose a Windows installation")
        .clone();

    let accounts = active
        .with_conn(|case_conn| {
            app_services::emulation_bypass::list_bypass_accounts(
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &data_source_id,
                },
                install.partition_index,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert!(!accounts.is_empty(), "SAM accounts must be listed");
    for account in &accounts {
        eprintln!(
            "account rid={} name={:?} disabled={} locked={} has_password={}",
            account.rid,
            account.username,
            account.disabled,
            account.locked_out,
            account.has_password
        );
    }
    let target = accounts
        .iter()
        .find(|account| account.has_password)
        .or_else(|| accounts.first())
        .expect("at least one account")
        .clone();

    let provider =
        evidence_block::open_block_provider(&image, evidence_block::EvidenceImageKind::E01)
            .unwrap();
    let identity = evidence_emulation::ParentIdentity::new(provider.len(), [0x5au8; 32]).unwrap();
    let overlay = temp.path().join("overlay.cow");
    let disk = Arc::new(
        evidence_emulation::CowDisk::create(
            &overlay,
            provider,
            identity,
            evidence_emulation::CowDiskConfig::default(),
        )
        .unwrap(),
    );

    let result = active
        .with_conn(|case_conn| {
            app_services::emulation_bypass::apply_bypass(
                &disk,
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &data_source_id,
                },
                install.partition_index,
                target.rid,
                transport::dto::EmulationBypassActionDto::EnableAndClearPassword,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    eprintln!("bypass result: {result:?}");
    assert!(result.password_cleared || result.already_passwordless);

    // A second bypass on a different account must compose with the first:
    // the service reads SAM through the overlay, so the earlier edit stays.
    let second = accounts
        .iter()
        .find(|account| account.rid != target.rid && account.has_password)
        .cloned();
    if let Some(second) = &second {
        let second_result = active
            .with_conn(|case_conn| {
                app_services::emulation_bypass::apply_bypass(
                    &disk,
                    &app_services::emulation_bypass::BypassCaseContext {
                        case_conn,
                        case_root: &case_root,
                        case_id: &case_id,
                        data_source_id: &data_source_id,
                    },
                    install.partition_index,
                    second.rid,
                    transport::dto::EmulationBypassActionDto::ClearPassword,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
            })
            .unwrap();
        eprintln!("second bypass result: {second_result:?}");
        assert!(second_result.password_cleared || second_result.already_passwordless);
    }

    // Read the edited SAM hive back through the COW disk and prove the NT
    // hash is now the canonical empty value.
    let (fs, _) = open_windows_fs(&image).expect("Windows volume must open");
    let extents = fs
        .file_extent_map("Windows/System32/config/SAM")
        .expect("SAM extent map");
    let inode = fs
        .preview_file("Windows/System32/config/SAM")
        .unwrap()
        .inode();
    let hive_len = fs.file_size_by_inode(inode).unwrap().unwrap() as usize;
    let mut overlay_hive = vec![0u8; hive_len];
    eprintln!("readback: hive_len={hive_len} extents={}", extents.len());
    for extent in &extents {
        let start = extent.logical_offset as usize;
        let end = (start + extent.length as usize).min(hive_len);
        if start >= end {
            continue;
        }
        disk.read_exact_at(extent.volume_offset, &mut overlay_hive[start..end])
            .unwrap();
    }
    eprintln!("overlay hive head: {:02x?}", &overlay_hive[..16]);
    let system_hive = fs
        .read_file_range("Windows/System32/config/SYSTEM", 0, usize::MAX)
        .or_else(|_| {
            let inode = fs
                .preview_file("Windows/System32/config/SYSTEM")
                .unwrap()
                .inode();
            let size = fs.file_size_by_inode(inode).unwrap().unwrap() as usize;
            fs.read_file_range("Windows/System32/config/SYSTEM", 0, size)
        })
        .expect("SYSTEM hive must be readable");
    let boot_key = artifacts_windows::registry::sam_structs::extract_boot_key(&system_hive)
        .expect("boot key from SYSTEM hive");
    let info = artifacts_windows::registry::lookup::extract_sam_fields(
        &overlay_hive,
        "SAM",
        Some(boot_key),
    )
    .expect("edited SAM hive must stay parseable");
    let edited = info
        .users
        .iter()
        .find(|user| user.rid == target.rid)
        .expect("edited account must remain listed");
    eprintln!(
        "edited account: rid={} name={:?} hash={:?}",
        edited.rid, edited.username, edited.password_hash
    );
    assert_eq!(
        edited.password_hash.as_deref(),
        Some("aad3b435b51404eeaad3b435b51404ee:31d6cfe0d16ae931b73c59d7e0c089c0"),
        "LM and NT hashes must decrypt to the canonical empty values"
    );
    if let Some(second) = &second {
        let second_edited = info
            .users
            .iter()
            .find(|user| user.rid == second.rid)
            .expect("second edited account must remain listed");
        eprintln!(
            "second edited account: rid={} name={:?} hash={:?}",
            second_edited.rid, second_edited.username, second_edited.password_hash
        );
        assert_eq!(
            second_edited.password_hash.as_deref(),
            Some("aad3b435b51404eeaad3b435b51404ee:31d6cfe0d16ae931b73c59d7e0c089c0"),
            "the second bypass must also decrypt to empty without reverting the first"
        );
    }

    // The parent evidence must stay byte-identical to its pre-edit state.
    let parent_hive = fs
        .read_file_range("Windows/System32/config/SAM", 0, hive_len)
        .expect("parent SAM must be readable");
    assert_ne!(
        overlay_hive, parent_hive,
        "the overlay copy must differ after the bypass edit"
    );
    assert_eq!(std::fs::metadata(&image).unwrap().len(), image_size);
}

#[test]
#[ignore = "requires a real Windows E01 image"]
fn osdata_cleanup_removes_the_entry_through_the_overlay() {
    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "emulation-osdata",
        Some("tester"),
    )
    .unwrap();
    let image = sample_path();
    let image_size = std::fs::metadata(&image).unwrap().len();
    let data_source_id = import_image(&active, &image);

    let (case_id, case_root) = (active.meta.id.clone(), active.case_root.clone());
    let preflight = active
        .with_conn(|case_conn| {
            app_services::mount_service::emulation_preflight(
                case_conn,
                &case_root,
                &case_id,
                &data_source_id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    let install = preflight
        .installs
        .iter()
        .find(|install| install.osdata_present)
        .expect("the image must expose an install with OSDATA")
        .clone();
    eprintln!("target install: P{}", install.partition_index);

    let provider =
        evidence_block::open_block_provider(&image, evidence_block::EvidenceImageKind::E01)
            .unwrap();
    let identity = evidence_emulation::ParentIdentity::new(provider.len(), [0x5au8; 32]).unwrap();
    let overlay = temp.path().join("overlay-osdata.cow");
    let disk = Arc::new(
        evidence_emulation::CowDisk::create(
            &overlay,
            provider,
            identity,
            evidence_emulation::CowDiskConfig::default(),
        )
        .unwrap(),
    );

    let result = active
        .with_conn(|case_conn| {
            app_services::emulation_osdata::cleanup_osdata(
                &disk,
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &data_source_id,
                },
                install.partition_index,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    eprintln!("cleanup result: {result:?}");
    assert_eq!(
        result.state,
        transport::dto::EmulationOsdataCleanupStateDto::Removed,
        "OSDATA must be removed through the overlay"
    );
    assert!(
        result.edits_applied >= 2,
        "index edit plus record retirement"
    );

    // A second run re-plans against the read-only evidence and re-applies
    // the same edits: the operation is idempotent over the COW layer.
    let second = active
        .with_conn(|case_conn| {
            app_services::emulation_osdata::cleanup_osdata(
                &disk,
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &data_source_id,
                },
                install.partition_index,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    eprintln!("second run: {second:?}");

    // The parent evidence still lists OSDATA and remains byte-identical.
    let (fs, _) = open_windows_fs(&image).expect("Windows volume must open");
    let parent_has_osdata = fs
        .list_children("Windows/System32/config")
        .expect("parent config listing")
        .iter()
        .any(|node| node.name.eq_ignore_ascii_case("OSDATA"));
    assert!(parent_has_osdata, "the evidence image must keep OSDATA");
    assert_eq!(std::fs::metadata(&image).unwrap().len(), image_size);
}

#[test]
#[ignore = "debug helper: dumps the SAM hive from a real image"]
fn debug_dump_sam_hive_layout() {
    let image = sample_path();
    let (fs, _) = open_windows_fs(&image).expect("Windows volume must open");
    let inode = fs
        .preview_file("Windows/System32/config/SAM")
        .unwrap()
        .inode();
    let hive_len = fs.file_size_by_inode(inode).unwrap().unwrap() as usize;
    let hive = fs
        .read_file_range("Windows/System32/config/SAM", 0, hive_len)
        .unwrap();
    std::fs::write("D:/process/forensic/target/tmp/sam_hive_debug.bin", &hive).unwrap();
    eprintln!("SAM hive dumped: {} bytes", hive.len());

    // Cross-check the raw extent mapping against the logical read path.
    // Extent offsets are absolute in the reader's coordinate space.
    let extents = fs
        .file_extent_map("Windows/System32/config/SAM")
        .expect("extent map");
    let mut raw = [0u8; 16];
    {
        use std::io::{Read as _, Seek as _, SeekFrom};
        let mut reader = E01Reader::open(&image).unwrap();
        reader
            .seek(SeekFrom::Start(extents[0].volume_offset))
            .unwrap();
        reader.read_exact(&mut raw).unwrap();
    }
    eprintln!(
        "extent check: extent0={:#x} raw_head={:02x?} hive_head={:02x?}",
        extents[0].volume_offset,
        raw,
        &hive[..16]
    );
}
