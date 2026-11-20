//! Real XFS E01 validation of the host-side log-clear repair: register the
//! image's XFS volumes, repair through the COW overlay, set one account's
//! password, verify repeatability, then optionally materialize the repaired
//! disk for a VMware boot test. Run with FORENSICS_XFS_E01_FIXTURE set:
//!
//! ```text
//! cargo test -p app-services --test xfs_log_repair_e01 -- --include-ignored --nocapture
//! ```
//!
//! Invariance guarantees:
//! * The E01 container's length and streaming SHA-256 are recorded before
//!   import and re-checked after every operation; the original evidence must
//!   never change.
//! * Fixed parent regions are sampled through the COW overlay before any write
//!   and re-sampled afterwards; parent bytes must be identical
//!   (`ParentIdentity` semantics).
//!
//! Result semantics (new plan/apply contract):
//! * `status=Applied && method=HostInPlace && password_set=true` continues the
//!   original flow; the repeat call must report `already_configured=true` and
//!   `password_set=false`.
//! * `status=Unavailable` (SharedExtents/DirtyLog/UnsupportedLayout/...) prints
//!   the structured verdict and fails explicitly; it is never reported as a
//!   pass.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use sha2::{Digest, Sha256};
use transport::dto::{
    EmulationFsVolumeStateDto, EmulationLinuxBypassMethodDto, EmulationLinuxBypassStatusDto,
};

fn sample_path() -> PathBuf {
    std::env::var_os("FORENSICS_XFS_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_XFS_E01_FIXTURE")
}

/// Stream the whole image and return (bytes read, lowercase hex SHA-256).
fn sha256_file(path: &Path) -> std::io::Result<(u64, String)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((total, hex::encode(hasher.finalize())))
}

/// Sample fixed regions through the COW overlay; before any write these read
/// back parent bytes, and afterwards they must still read back identical
/// parent bytes.
fn sample_regions(
    disk: &Arc<evidence_emulation::CowDisk>,
    regions: &[(u64, usize)],
) -> Vec<(u64, Vec<u8>)> {
    let mut reader = app_services::emulation_cow_reader::CowDiskReader::new(Arc::clone(disk));
    regions
        .iter()
        .map(|(offset, length)| {
            let mut buffer = vec![0u8; *length];
            reader
                .seek(SeekFrom::Start(*offset))
                .expect("seek the parent sample region");
            reader
                .read_exact(&mut buffer)
                .expect("read the parent sample region");
            (*offset, buffer)
        })
        .collect()
}

fn assert_samples_match(label: &str, before: &[(u64, Vec<u8>)], after: &[(u64, Vec<u8>)]) {
    assert_eq!(
        before.len(),
        after.len(),
        "{label}: the sampled region set changed"
    );
    for ((offset_before, before_bytes), (offset_after, after_bytes)) in
        before.iter().zip(after.iter())
    {
        assert_eq!(
            offset_before, offset_after,
            "{label}: sample offset shifted"
        );
        assert_eq!(
            before_bytes, after_bytes,
            "{label}: parent bytes at offset {offset_before} changed; evidence was modified"
        );
    }
}

#[test]
#[ignore = "requires the private XFS E01 sample"]
fn xfs_log_repair_and_materialization() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_test_writer()
        .try_init();
    let image = sample_path();

    // Baseline evidence fingerprint before anything touches the disk.
    let (baseline_len, baseline_hash) = sha256_file(&image).expect("hash the E01 before import");
    eprintln!("baseline E01: {baseline_len} bytes sha256={baseline_hash}");

    let temp = tempfile::TempDir::new().unwrap();
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "xfs-log-clear",
        Some("tester"),
    )
    .unwrap();

    let data_source_id = active
        .with_conn(|case_conn| {
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &image.to_string_lossy(),
                domain::DataSourcePlatform::Linux,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let job_id = persistence_sqlite::repositories::job_repo::JobRepo::new(case_conn)
                .create(&active.meta.id.0, "xfs log repair import")?;
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            app_services::import_pipeline::execute_import_job_with_counts(
                case_conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job_id,
                app_services::import_pipeline::ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: None,
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,

                    defer_linux_artifacts: false,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            let sources = DataSourceRepo::new(case_conn).find_by_case(&active.meta.id)?;
            Ok(sources[0].id.clone())
        })
        .expect("import the XFS image metadata-only");

    let (case_id, case_root) = (active.meta.id.clone(), active.case_root.clone());
    let xfs_records: Vec<(u32, u64, u64)> = active
        .with_conn(|case_conn| {
            let source_conn = app_services::source_db::open_ready_source_read_only_by_id(
                case_conn,
                &case_root,
                &case_id,
                &data_source_id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let partitions = app_services::partition_query_service::PartitionQueryRepo::new(
                &source_conn.connection,
            )
            .find_by_data_source(&data_source_id.0)?;
            Ok(partitions
                .iter()
                .filter(|record| record.filesystem.as_deref() == Some("XFS"))
                .map(|record| (record.partition_index, record.offset, record.length))
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert!(!xfs_records.is_empty(), "fixture must contain XFS volumes");
    let xfs_partitions: Vec<u32> = xfs_records.iter().map(|(index, _, _)| *index).collect();

    let provider =
        evidence_block::open_block_provider(&image, evidence_block::EvidenceImageKind::E01)
            .unwrap();
    let identity = evidence_emulation::ParentIdentity::new(provider.len(), [0x5au8; 32]).unwrap();
    let disk = Arc::new(
        evidence_emulation::CowDisk::create(
            &temp.path().join("overlay.cow"),
            provider,
            identity,
            evidence_emulation::CowDiskConfig::default(),
        )
        .unwrap(),
    );

    // Fixed parent regions sampled before any overlay write: the disk head plus
    // the first 64 KiB of every declared XFS partition.
    let disk_len = disk.len();
    let mut regions: Vec<(u64, usize)> = vec![(0, 64 * 1024)];
    for (_, offset, length) in &xfs_records {
        let available = disk_len.saturating_sub(*offset) as usize;
        let region_len = (64 * 1024).min(available).min(*length as usize);
        if region_len > 0 {
            regions.push((*offset, region_len));
        }
    }
    let pre_write_samples = sample_regions(&disk, &regions);

    let repair = active
        .with_conn(|case_conn| {
            app_services::emulation_fs_repair::repair_xfs_logs(
                &disk,
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &data_source_id,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("repair the XFS logs");
    eprintln!("repair result: {repair:?}");
    assert_eq!(repair.items.len(), xfs_partitions.len());
    for item in &repair.items {
        eprintln!(
            "P{} state={:?} repaired={} log_bytes={}",
            item.partition_index, item.state, item.repaired, item.log_bytes
        );
        assert_ne!(item.state, EmulationFsVolumeStateDto::Unsupported);
    }

    let (partition_index, username) = active
        .with_conn(|case_conn| {
            let context = app_services::emulation_bypass::BypassCaseContext {
                case_conn,
                case_root: &case_root,
                case_id: &case_id,
                data_source_id: &data_source_id,
            };
            for partition_index in &xfs_partitions {
                let Ok(accounts) = app_services::emulation_linux_bypass::list_linux_accounts(
                    &context,
                    *partition_index,
                ) else {
                    continue;
                };
                if let Some(account) = accounts.into_iter().find(|account| account.has_password) {
                    return Ok(Some((*partition_index, account.username)));
                }
            }
            Ok(None)
        })
        .unwrap()
        .expect("an XFS root must expose a password-protected shadow account");
    let bypass = active
        .with_conn(|case_conn| {
            app_services::emulation_linux_bypass::apply_linux_bypass(
                &disk,
                &app_services::emulation_bypass::BypassCaseContext {
                    case_conn,
                    case_root: &case_root,
                    case_id: &case_id,
                    data_source_id: &data_source_id,
                },
                partition_index,
                &username,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("run the XFS shadow bypass plan");

    eprintln!(
        "apply result: status={:?} method={:?} password_set={} already_configured={} reason={:?} alternatives={:?} payload_staged={}",
        bypass.status,
        bypass.method,
        bypass.password_set,
        bypass.already_configured,
        bypass.reason,
        bypass.alternatives,
        bypass.payload_staged
    );

    match bypass.status {
        EmulationLinuxBypassStatusDto::Unavailable => {
            // The host in-place channel was refused (SharedExtents / DirtyLog /
            // UnsupportedLayout / ApplianceUnavailable / ...). Nothing was
            // written; prove the evidence still matches the baseline and fail
            // explicitly instead of reporting success.
            let after_samples = sample_regions(&disk, &regions);
            assert_samples_match("unavailable", &pre_write_samples, &after_samples);
            let (final_len, final_hash) =
                sha256_file(&image).expect("hash the E01 after the unavailable verdict");
            assert_eq!(final_len, baseline_len, "the E01 length changed");
            assert_eq!(final_hash, baseline_hash, "the E01 SHA-256 changed");
            let reason = bypass
                .reason
                .map(|reason| format!("{reason:?}"))
                .unwrap_or_else(|| "<none>".to_string());
            panic!(
                "未完成自动绕密：{reason}；需要用户自备 Linux rescue ISO 手动执行 resources/emulation/linux-rescue/BYPASS.SH（见该目录 README）"
            );
        }
        EmulationLinuxBypassStatusDto::Applied => {
            assert_eq!(
                bypass.method,
                EmulationLinuxBypassMethodDto::HostInPlace,
                "Applied bypass must use the host in-place channel"
            );
            assert!(
                bypass.password_set,
                "first apply must set the XFS shadow password"
            );
            assert!(!bypass.already_configured);

            let repeated = active
                .with_conn(|case_conn| {
                    app_services::emulation_linux_bypass::apply_linux_bypass(
                        &disk,
                        &app_services::emulation_bypass::BypassCaseContext {
                            case_conn,
                            case_root: &case_root,
                            case_id: &case_id,
                            data_source_id: &data_source_id,
                        },
                        partition_index,
                        &username,
                    )
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
                })
                .expect("repeat the XFS shadow bypass");
            eprintln!(
                "repeat result: status={:?} method={:?} password_set={} already_configured={}",
                repeated.status,
                repeated.method,
                repeated.password_set,
                repeated.already_configured
            );
            assert_eq!(repeated.status, EmulationLinuxBypassStatusDto::Applied);
            assert_eq!(repeated.method, EmulationLinuxBypassMethodDto::HostInPlace);
            assert!(!repeated.password_set);
            assert!(repeated.already_configured);

            // Parent regions must be unchanged after repair + apply + repeat.
            let after_samples = sample_regions(&disk, &regions);
            assert_samples_match("post-repeat", &pre_write_samples, &after_samples);
        }
    }

    // The original E01 container must be byte-for-byte identical.
    let (final_len, final_hash) = sha256_file(&image).expect("hash the E01 after the bypass");
    assert_eq!(final_len, baseline_len, "the E01 length changed");
    assert_eq!(
        final_hash, baseline_hash,
        "the E01 SHA-256 changed; original evidence was modified"
    );

    if let Some(out_path) = std::env::var_os("BOOT_DIAG_OUT").map(PathBuf::from) {
        use std::io::Write;
        let mut reader = app_services::emulation_cow_reader::CowDiskReader::new(Arc::clone(&disk));
        let total = disk.len();
        let mut out =
            std::io::BufWriter::new(std::fs::File::create(&out_path).expect("create output"));
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let mut done = 0u64;
        while done < total {
            let got = reader.read(&mut buf).expect("read");
            assert!(got > 0, "unexpected EOF");
            out.write_all(&buf[..got]).expect("write");
            done += got as u64;
        }
        eprintln!("wrote {} bytes to {}", done, out_path.display());
    }
}
