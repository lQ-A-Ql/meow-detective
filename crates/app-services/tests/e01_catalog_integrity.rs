//! Real E01 catalog integrity regression tests.
//!
//! Each test runs the production filesystem enumeration into an isolated
//! source database, then validates the persisted catalog as a graph rather
//! than accepting only a non-empty file count. BitLocker samples additionally
//! validate runtime-bound analysis and deleted-recovery access.
//!
//! Run all four samples serially:
//!   $env:FORENSICS_WINDOWS1_E01_FIXTURE='...'
//!   $env:FORENSICS_WINDOWS2_E01_FIXTURE='...'
//!   $env:FORENSICS_LINUX1_E01_FIXTURE='...'
//!   $env:FORENSICS_LINUX2_E01_FIXTURE='...'
//!   cargo test -p app-services --test e01_catalog_integrity -- --ignored --nocapture --test-threads=1

use app_services::bitlocker_runtime::BitLockerUnlockRegistry;
use app_services::bitlocker_service::{
    BitLockerKeyStore, BitLockerKeyStoreError, BitLockerRuntimeContext,
};
use app_services::datasource_service::{
    detect_image_filesystem, expand_lvm_pool_candidates, ImageFilesystemKind,
};
use app_services::file_service::PreviewRuntimeRegistry;
use app_services::{file_service, parallel_enum, staging};
use domain::{DataSourceId, DataSourcePlatform};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, staging_repo::StagingRepo,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use volume_bitlocker::{MetadataFingerprint, Passphrase, PersistedKeyBlob};

const WINDOWS1_ENV: &str = "FORENSICS_WINDOWS1_E01_FIXTURE";
const WINDOWS2_ENV: &str = "FORENSICS_WINDOWS2_E01_FIXTURE";
const LINUX1_ENV: &str = "FORENSICS_LINUX1_E01_FIXTURE";
const LINUX2_ENV: &str = "FORENSICS_LINUX2_E01_FIXTURE";
const WINDOWS1_RECOVERY_ENV: &str = "FORENSICS_BITLOCKER_PRIVATE_LIUYANG_RECOVERY_PASSWORD";
const WINDOWS2_RECOVERY_ENV: &str = "FORENSICS_BITLOCKER_PRIVATE_JC2_RECOVERY_PASSWORD";
const ENUMERATION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
struct TestKeyStore {
    blobs: Mutex<HashMap<String, Vec<u8>>>,
}

impl BitLockerKeyStore for TestKeyStore {
    fn load(
        &self,
        fingerprint: &MetadataFingerprint,
    ) -> Result<Option<PersistedKeyBlob>, BitLockerKeyStoreError> {
        self.blobs
            .lock()
            .expect("test key store lock")
            .get(fingerprint.as_str())
            .cloned()
            .map(PersistedKeyBlob::from_storage)
            .transpose()
            .map_err(BitLockerKeyStoreError::CorruptBlob)
    }

    fn store(
        &self,
        fingerprint: &MetadataFingerprint,
        blob: PersistedKeyBlob,
    ) -> Result<(), BitLockerKeyStoreError> {
        self.blobs.lock().expect("test key store lock").insert(
            fingerprint.as_str().to_string(),
            blob.expose_for_storage().to_vec(),
        );
        Ok(())
    }

    fn delete(&self, fingerprint: &MetadataFingerprint) -> Result<bool, BitLockerKeyStoreError> {
        Ok(self
            .blobs
            .lock()
            .expect("test key store lock")
            .remove(fingerprint.as_str())
            .is_some())
    }
}

#[derive(Debug, Default)]
struct CatalogCounts {
    rows: u64,
    files: u64,
    directories: u64,
    roots: u64,
    root_children: u64,
    reachable: u64,
    orphans: u64,
    foreign_rows: u64,
    invalid_types: u64,
    empty_non_root_paths: u64,
    files_without_size: u64,
}

fn fixture_path(env_name: &str) -> PathBuf {
    std::env::var_os(env_name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {env_name} to run this ignored real E01 test"))
}

fn recovery_password_env(env_name: &str) -> Option<&'static str> {
    match env_name {
        WINDOWS1_ENV => Some(WINDOWS1_RECOVERY_ENV),
        WINDOWS2_ENV => Some(WINDOWS2_RECOVERY_ENV),
        _ => None,
    }
}

fn enumerate_staged_catalog(
    case_root: &Path,
    source_conn: &Connection,
    data_source_id: &DataSourceId,
    fixture: &Path,
    probe: &app_services::datasource_service::ImageFilesystemProbe,
    cancel_token: Arc<AtomicBool>,
    env_name: &str,
) -> persistence_sqlite::DbResult<(u64, u64, u64)> {
    file_service::store_data_source_partitions(source_conn, data_source_id, &probe.partitions)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

    let mut work = Vec::new();
    for partition in &probe.partitions {
        if partition.status != app_services::datasource_service::PartitionStatus::Supported {
            continue;
        }
        let Some(fs_kind) = partition.filesystem else {
            continue;
        };
        let fs_kind_label = match fs_kind {
            ImageFilesystemKind::Ntfs => "NTFS",
            ImageFilesystemKind::Fat => "FAT",
            ImageFilesystemKind::Ext4 => "EXT4",
            ImageFilesystemKind::Xfs => "XFS",
            ImageFilesystemKind::Btrfs => "BTRFS",
            ImageFilesystemKind::BitLocker | ImageFilesystemKind::LvmPool => continue,
        };
        if let Some(partition_work) = app_services::import_pipeline::build_partition_work(
            fixture,
            &domain::DataSourceKind::E01,
            partition.index,
            &partition.name,
            fs_kind_label,
            &probe.candidates,
        ) {
            work.push(partition_work);
        }
    }

    let results = parallel_enum::enumerate_partitions_parallel(
        case_root,
        data_source_id,
        work,
        1,
        cancel_token,
        &|partition_index, percent, detail| {
            if percent == 0 || percent >= 60 {
                eprintln!("{env_name}: partition={partition_index} {percent}% {detail}");
            }
        },
    )
    .map_err(persistence_sqlite::DbError::System)?;

    let mut files = 0;
    let mut directories = 0;
    let mut total_size = 0;
    for result in results {
        if let Some(error) = result.error {
            return Err(persistence_sqlite::DbError::System(format!(
                "Partition {} enumeration failed: {error}",
                result.index
            )));
        }
        let partition_name = probe
            .partitions
            .iter()
            .find(|partition| partition.index == result.index)
            .map(|partition| partition.name.as_str())
            .unwrap_or("Windows partition");
        let staging_conn =
            staging::open_partition_staging(case_root, &data_source_id.0, result.index)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        StagingRepo::merge_enum_staging_to_main(
            source_conn,
            &staging_conn,
            &data_source_id.0,
            result.index,
            partition_name,
        )?;
        files += result.file_count;
        directories += result.dir_count;
        total_size += result.total_size;
    }
    Ok((files, directories, total_size))
}

fn run_catalog_integrity_test(env_name: &str, platform: DataSourcePlatform) {
    let fixture = fixture_path(env_name);
    assert!(
        fixture.is_file(),
        "{env_name} must point to an existing E01 file: {}",
        fixture.display()
    );

    let temp = TempDir::new().expect("create isolated catalog test root");
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "catalog-integrity",
        Some("catalog-integrity-test"),
    )
    .expect("create isolated catalog test case");
    let case_id = active.meta.id.clone();

    active
        .with_conn(|case_conn| {
            let mut probe_reader = E01Reader::open(&fixture)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let mut probe = detect_image_filesystem(&mut probe_reader)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            expand_lvm_pool_candidates(
                &mut probe,
                &fixture,
                &domain::DataSourceKind::E01,
            );
            let bitlocker_indices = probe
                .candidates
                .iter()
                .filter(|candidate| candidate.kind == ImageFilesystemKind::BitLocker)
                .map(|candidate| candidate.partition_index.unwrap_or_default() as u32)
                .collect::<Vec<_>>();

            let source = app_services::datasource_service::attach_data_source(
                case_conn,
                &case_id,
                env_name,
                &fixture,
                domain::DataSourceKind::E01,
                platform,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let source_conn = app_services::source_db::open_source_db(&active.case_root, &source.id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            DataSourceRepo::new(&source_conn)
                .upsert_source_local_metadata(&case_id, &source)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            DataSourceRepo::new(case_conn)
                .update_import_state(&source.id, "ready", None)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            let cancel_token = Arc::new(AtomicBool::new(false));
            let completed = Arc::new(AtomicBool::new(false));
            let timeout_cancel = Arc::clone(&cancel_token);
            let timeout_completed = Arc::clone(&completed);
            let timeout_label = env_name.to_string();
            let timer = thread::spawn(move || {
                let started = Instant::now();
                while !timeout_completed.load(Ordering::Relaxed)
                    && started.elapsed() < ENUMERATION_TIMEOUT
                {
                    thread::sleep(Duration::from_millis(250));
                }
                if !timeout_completed.load(Ordering::Relaxed) {
                    timeout_cancel.store(true, Ordering::Relaxed);
                    eprintln!(
                        "{timeout_label}: enumeration timeout reached; cancellation requested"
                    );
                }
            });
            let enumeration_started = Instant::now();
            let stats = enumerate_staged_catalog(
                &active.case_root,
                &source_conn,
                &source.id,
                &fixture,
                &probe,
                Arc::clone(&cancel_token),
                env_name,
            )
            .map(|(files, directories, total_size)| {
                app_services::file_service::EnumerationStats {
                    file_count: files,
                    dir_count: directories,
                    total_size,
                    warnings: Vec::new(),
                    diagnostics: Vec::new(),
                }
            });
            completed.store(true, Ordering::Relaxed);
            timer.join().expect("enumeration timeout timer must exit");
            let stats = stats?;
            assert!(
                enumeration_started.elapsed() < ENUMERATION_TIMEOUT,
                "{env_name}: filesystem enumeration exceeded the 10 minute test threshold"
            );
            drop(source_conn);

            let bitlocker_runtime = Arc::new(BitLockerUnlockRegistry::default());
            let mut bitlocker_file_count = 0;
            let mut bitlocker_directory_count = 0;
            if !bitlocker_indices.is_empty() {
                let recovery_env = recovery_password_env(env_name).unwrap_or_else(|| {
                    panic!(
                        "{env_name}: BitLocker candidate detected but no recovery-password environment mapping exists"
                    )
                });
                let recovery_password = std::env::var(recovery_env).unwrap_or_else(|_| {
                    panic!(
                        "{env_name}: set {recovery_env} to test the complete BitLocker catalog"
                    )
                });
                let preview_runtime = Arc::new(PreviewRuntimeRegistry::default());
                let key_store = TestKeyStore::default();
                let runtimes = BitLockerRuntimeContext::new(
                    &preview_runtime,
                    &bitlocker_runtime,
                    &key_store,
                );
                for partition_index in bitlocker_indices.iter().copied() {
                    let status = app_services::bitlocker_service::unlock_bitlocker_with_recovery_password(
                        case_conn,
                        &active.case_root,
                        &case_id,
                        &source.id,
                        partition_index,
                        Passphrase::new(recovery_password.clone()),
                        runtimes,
                    )
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                    assert!(
                        status.unlocked,
                        "{env_name}: BitLocker partition {partition_index} did not unlock"
                    );
                    let catalog = app_services::bitlocker_service::import_unlocked_bitlocker_catalog(
                        case_conn,
                        &active.case_root,
                        &case_id,
                        &source.id,
                        partition_index,
                        runtimes,
                    )
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                    assert!(
                        catalog.imported,
                        "{env_name}: BitLocker partition {partition_index} catalog was not imported"
                    );
                    bitlocker_file_count += catalog.file_count.unwrap_or_default();
                    bitlocker_directory_count += catalog.directory_count.unwrap_or_default();
                    eprintln!(
                        "{env_name}: BitLocker partition {partition_index} catalog files={} dirs={}",
                        catalog.file_count.unwrap_or_default(),
                        catalog.directory_count.unwrap_or_default()
                    );
                }
            }

            let source_conn = app_services::source_db::open_source_db(&active.case_root, &source.id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            let counts = read_catalog_counts(&source_conn, &source.id)?;
            assert_catalog_integrity(&counts, env_name, &source.id);
            assert_eq!(
                stats.file_count + bitlocker_file_count,
                counts.files,
                "{env_name}: enumeration file count differs from persisted source DB"
            );
            assert_eq!(
                stats.dir_count + bitlocker_directory_count,
                counts.directories,
                "{env_name}: enumeration directory count differs from persisted source DB"
            );

            eprintln!(
                "catalog integrity: sample={} source={} platform={} rows={} files={} dirs={} roots={} reachable={} orphans={}",
                env_name,
                source.id.0,
                platform,
                counts.rows,
                counts.files,
                counts.directories,
                counts.roots,
                counts.reachable,
                counts.orphans,
            );
            assert_source_file_extraction(
                case_conn,
                &active.case_root,
                &case_id,
                &source.id,
                &source_conn,
                &bitlocker_runtime,
                env_name,
            )?;
            if !bitlocker_indices.is_empty() {
                assert_bitlocker_analysis_and_recovery(
                    case_conn,
                    &active.case_root,
                    &case_id,
                    &source.id,
                    &source_conn,
                    &bitlocker_indices,
                    &bitlocker_runtime,
                    env_name,
                )?;
            }
            Ok(())
        })
        .expect("catalog integrity test failed");
}

#[allow(clippy::too_many_arguments)]
fn assert_source_file_extraction(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    data_source_id: &DataSourceId,
    source_conn: &Connection,
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    env_name: &str,
) -> persistence_sqlite::DbResult<()> {
    let mut statement = source_conn.prepare(
        "SELECT id, size FROM file_entries
         WHERE data_source_id = ?1
           AND entry_type = 'file' COLLATE NOCASE
           AND encrypted = 0 AND size BETWEEN 1 AND 4194304
         ORDER BY size ASC LIMIT 512",
    )?;
    let candidates = statement
        .query_map([&data_source_id.0], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    let export_root = TempDir::new()?;
    let mut failures = Vec::new();
    for (local_id, size) in candidates {
        let global_id = app_services::source_db::GlobalFileId::new(
            data_source_id.clone(),
            domain::FileEntryId(local_id),
        )
        .encode()
        .0;
        let preview = transport::dto::ViewerRangeRequestDto {
            handle_id: format!("file:{global_id}"),
            offset: 0,
            length: size.min(4_096) as u32,
        };
        let preview = match file_service::read_file_range_for_source_case_with_bitlocker(
            bitlocker_runtime,
            case_conn,
            case_root,
            case_id,
            &preview,
        ) {
            Ok(response) => response.raw_bytes.unwrap_or_default(),
            Err(error) => {
                failures.push(error.to_string());
                continue;
            }
        };
        if preview.is_empty() {
            continue;
        }

        let destination = export_root.path().join(format!("{}.bin", failures.len()));
        let extraction = match file_service::extract_file_to_destination_for_case_with_bitlocker(
            bitlocker_runtime,
            case_conn,
            case_root,
            case_id,
            &global_id,
            &destination,
            false,
        ) {
            Ok(result) => result,
            Err(error) => {
                failures.push(error.to_string());
                continue;
            }
        };
        let exported = std::fs::read(&destination)?;
        assert_eq!(&exported[..preview.len()], preview.as_slice());
        assert_eq!(exported.len() as u64, size);
        assert_eq!(extraction.bytes_written, size);
        assert_eq!(extraction.source_size, Some(size));
        assert_eq!(extraction.sha256, hex::encode(Sha256::digest(&exported)));
        assert!(extraction.size_verified);
        eprintln!(
            "{env_name}: physical extraction file={global_id} bytes={size} sha256={}",
            extraction.sha256
        );
        return Ok(());
    }

    Err(persistence_sqlite::DbError::System(format!(
        "{env_name}: no bounded regular file could be previewed and physically extracted; first failures: {}",
        failures.into_iter().take(5).collect::<Vec<_>>().join(" | ")
    )))
}

#[allow(clippy::too_many_arguments)]
fn assert_bitlocker_analysis_and_recovery(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    data_source_id: &DataSourceId,
    source_conn: &Connection,
    partition_indices: &[u32],
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    env_name: &str,
) -> persistence_sqlite::DbResult<()> {
    for partition_index in partition_indices.iter().copied() {
        let recovery_context = app_services::deleted_recovery::DeletedRecoveryContext::new(
            case_conn,
            case_root,
            case_id,
            data_source_id,
        )
        .with_bitlocker_runtime(Arc::clone(bitlocker_runtime));
        let recovery = recovery_context
            .run(Some(partition_index))
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        assert!(
            recovery.failures.is_empty(),
            "{env_name}: BitLocker partition {partition_index} recovery failures: {:?}",
            recovery.failures
        );
        assert_eq!(
            recovery.scans.len(),
            1,
            "{env_name}: BitLocker partition {partition_index} must produce one recovery scan"
        );
        assert_eq!(recovery.scans[0].filesystem_type, "ntfs");
        assert_bitlocker_recovery_content(
            case_conn,
            case_root,
            case_id,
            data_source_id,
            partition_index,
            &recovery_context,
            env_name,
        )?;
    }

    let analysis_partition = partition_indices[0];
    source_conn.execute(
        "DELETE FROM file_entries WHERE partition_index <> ?1",
        [analysis_partition],
    )?;
    let locked_runtime = app_services::analysis_service::AnalysisSourceReadRuntime::default();
    let locked = app_services::analysis_service::get_file_classification_board(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        64,
        &locked_runtime,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    assert!(
        locked
            .warnings
            .iter()
            .any(|warning| warning.contains("BitLocker volume is locked")),
        "{env_name}: analysis without the verified runtime must reject BitLocker content"
    );

    let runtime = app_services::analysis_service::AnalysisSourceReadRuntime::with_bitlocker_runtime(
        Arc::clone(bitlocker_runtime),
    );
    let unlocked = app_services::analysis_service::get_file_classification_board(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        64,
        &runtime,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    assert!(unlocked.total_files > 0);
    assert!(
        unlocked
            .warnings
            .iter()
            .all(|warning| !warning.contains("BitLocker volume is locked")),
        "{env_name}: verified BitLocker runtime was not applied to analysis reads"
    );
    assert_bitlocker_extraction(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        source_conn,
        &runtime,
        env_name,
    )?;
    eprintln!(
        "{env_name}: BitLocker analysis files={} magic={} recoveryPartitions={}",
        unlocked.total_files,
        unlocked.magic_classified_count,
        partition_indices.len()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn assert_bitlocker_recovery_content(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    data_source_id: &DataSourceId,
    partition_index: u32,
    context: &app_services::deleted_recovery::DeletedRecoveryContext<'_>,
    env_name: &str,
) -> persistence_sqlite::DbResult<()> {
    let page = app_services::deleted_recovery::list_deleted_recoveries(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        partition_index,
        0,
        1_000,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let candidate = page.recoveries.iter().find(|candidate| {
        candidate.entry_type.as_deref() == Some("file")
            && candidate.completeness == transport::dto::RecoveryCompletenessDto::Complete
            && candidate.declared_size > 0
            && candidate.declared_size <= 16 * 1024 * 1024
    });
    let Some(candidate) = candidate else {
        eprintln!(
            "{env_name}: BitLocker partition {partition_index} has no bounded complete deleted-file content oracle"
        );
        return Ok(());
    };

    let read_length = u32::try_from(candidate.declared_size.min(4_096)).unwrap_or(4_096);
    let content = context
        .read_range(&candidate.id, 0, read_length)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    assert_eq!(content.bytes_read, read_length);
    assert!(!content.verified_range_ordinals.is_empty());

    let export_path = case_root
        .join("bitlocker-recovery-regression")
        .join(format!("partition-{partition_index}-recovery.bin"));
    let exported = context
        .export(&candidate.id, &export_path, false)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    assert_eq!(exported.bytes_written, candidate.declared_size);
    assert_eq!(
        candidate.content_sha256.as_deref(),
        Some(exported.sha256.as_str())
    );
    assert_eq!(
        std::fs::metadata(&export_path)?.len(),
        candidate.declared_size,
        "{env_name}: exported deleted file length differs from the verified recovery record"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn assert_bitlocker_extraction(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    data_source_id: &DataSourceId,
    source_conn: &Connection,
    runtime: &app_services::analysis_service::AnalysisSourceReadRuntime,
    env_name: &str,
) -> persistence_sqlite::DbResult<()> {
    let categories = ["Registry", "BrowserHistory", "Email", "EventLogs"];
    let selected = categories
        .iter()
        .filter_map(|category| {
            app_services::analysis_service::evidence_candidates_for_categories(
                source_conn,
                &[*category],
            )
            .ok()
            .filter(|candidates| !candidates.is_empty())
            .map(|candidates| (*category, candidates.len()))
        })
        .min_by_key(|(_, count)| *count);
    let Some((category, candidate_count)) = selected else {
        eprintln!("{env_name}: BitLocker catalog has no structured Windows artifact candidate");
        return Ok(());
    };

    let locked_runtime = app_services::analysis_service::AnalysisSourceReadRuntime::default();
    let locked_run_id = format!("{env_name}-bitlocker-locked");
    let locked = app_services::analysis_service::run_source_analysis_extraction_with_progress(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        &[category],
        app_services::analysis_service::AnalysisExtractionProgressContext::new(
            &locked_runtime,
            &locked_run_id,
        ),
        |_| {},
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    assert!(
        locked
            .warnings
            .iter()
            .any(|warning| warning.contains("BitLocker volume is locked")),
        "{env_name}: locked extraction did not report the missing BitLocker runtime"
    );

    let unlocked_run_id = format!("{env_name}-bitlocker-unlocked");
    let unlocked = app_services::analysis_service::run_source_analysis_extraction_with_progress(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        &[category],
        app_services::analysis_service::AnalysisExtractionProgressContext::new(
            runtime,
            &unlocked_run_id,
        ),
        |_| {},
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    assert!(unlocked.scanned_count > 0);
    assert_eq!(
        unlocked.checkpoint_hit_count, 0,
        "{env_name}: locked source reads must not create extraction checkpoints"
    );
    assert!(
        unlocked
            .warnings
            .iter()
            .all(|warning| !warning.contains("BitLocker volume is locked")),
        "{env_name}: unlocked extraction still attempted ciphertext reads"
    );
    eprintln!(
        "{env_name}: BitLocker extraction category={category} candidates={candidate_count} scanned={} artifacts={}",
        unlocked.scanned_count, unlocked.artifact_count
    );
    Ok(())
}

fn read_catalog_counts(
    conn: &Connection,
    data_source_id: &DataSourceId,
) -> persistence_sqlite::DbResult<CatalogCounts> {
    let id = &data_source_id.0;
    let rows = count(
        conn,
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
        id,
    )?;
    let files = count(
        conn,
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1 AND entry_type = 'file' COLLATE NOCASE",
        id,
    )?;
    let directories = count(
        conn,
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1 AND entry_type = 'directory' COLLATE NOCASE",
        id,
    )?;
    let roots = count(
        conn,
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1 AND parent_id IS NULL",
        id,
    )?;
    let root_children = count(
        conn,
        "SELECT COUNT(*) FROM file_entries child
         JOIN file_entries root ON root.id = child.parent_id
         WHERE child.data_source_id = ?1 AND root.data_source_id = ?1
           AND root.parent_id IS NULL",
        id,
    )?;
    let reachable = count(
        conn,
        "WITH RECURSIVE reachable(id) AS (
             SELECT id FROM file_entries
              WHERE data_source_id = ?1 AND parent_id IS NULL
             UNION
             SELECT child.id FROM reachable parent
             JOIN file_entries child INDEXED BY idx_source_file_entries_parent
               ON child.parent_id = parent.id
              WHERE child.data_source_id = ?1
         )
         SELECT COUNT(*) FROM reachable",
        id,
    )?;
    let orphans = count(
        conn,
        "SELECT COUNT(*) FROM file_entries child
         WHERE child.data_source_id = ?1
           AND child.parent_id IS NOT NULL
           AND NOT EXISTS (
               SELECT 1 FROM file_entries parent
                WHERE parent.id = child.parent_id
                  AND parent.data_source_id = ?1
           )",
        id,
    )?;
    let foreign_rows = count(
        conn,
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id <> ?1",
        id,
    )?;
    let invalid_types = count(
        conn,
        "SELECT COUNT(*) FROM file_entries
         WHERE data_source_id = ?1
           AND lower(entry_type) NOT IN ('file', 'directory')",
        id,
    )?;
    let empty_non_root_paths = count(
        conn,
        "SELECT COUNT(*) FROM file_entries
         WHERE data_source_id = ?1 AND parent_id IS NOT NULL AND trim(path) = ''",
        id,
    )?;
    let files_without_size = count(
        conn,
        "SELECT COUNT(*) FROM file_entries
         WHERE data_source_id = ?1 AND lower(entry_type) = 'file' AND size IS NULL",
        id,
    )?;

    Ok(CatalogCounts {
        rows,
        files,
        directories,
        roots,
        root_children,
        reachable,
        orphans,
        foreign_rows,
        invalid_types,
        empty_non_root_paths,
        files_without_size,
    })
}

fn count(conn: &Connection, sql: &str, data_source_id: &str) -> persistence_sqlite::DbResult<u64> {
    let value: i64 = conn.query_row(sql, [data_source_id], |row| row.get(0))?;
    u64::try_from(value).map_err(|error| {
        persistence_sqlite::DbError::System(format!("catalog count is negative: {error}"))
    })
}

fn assert_catalog_integrity(counts: &CatalogCounts, env_name: &str, data_source_id: &DataSourceId) {
    assert!(
        counts.rows > 0,
        "{env_name}: source {data_source_id:?} has no catalog rows"
    );
    assert!(
        counts.files > 0,
        "{env_name}: source {data_source_id:?} has no files"
    );
    assert!(
        counts.directories > 0,
        "{env_name}: source {data_source_id:?} has no directories"
    );
    assert_eq!(
        counts.rows,
        counts.files + counts.directories,
        "{env_name}: file/ directory counts do not cover all catalog rows"
    );
    assert!(counts.roots > 0, "{env_name}: catalog has no root node");
    assert!(
        counts.root_children > 0,
        "{env_name}: catalog root nodes have no children"
    );
    assert_eq!(
        counts.reachable, counts.rows,
        "{env_name}: not every catalog row is reachable from a root"
    );
    assert_eq!(
        counts.orphans, 0,
        "{env_name}: catalog contains orphan rows"
    );
    assert_eq!(
        counts.foreign_rows, 0,
        "{env_name}: source DB contains rows belonging to another source"
    );
    assert_eq!(
        counts.invalid_types, 0,
        "{env_name}: catalog contains an unsupported entry type"
    );
    assert_eq!(
        counts.empty_non_root_paths, 0,
        "{env_name}: non-root catalog rows contain empty paths"
    );
    assert_eq!(
        counts.files_without_size, 0,
        "{env_name}: file rows contain NULL sizes"
    );
}

#[test]
#[ignore = "requires FORENSICS_WINDOWS1_E01_FIXTURE real Windows E01 sample"]
fn windows1_catalog_integrity() {
    run_catalog_integrity_test(WINDOWS1_ENV, DataSourcePlatform::Windows);
}

#[test]
#[ignore = "requires FORENSICS_WINDOWS2_E01_FIXTURE real Windows E01 sample"]
fn windows2_catalog_integrity() {
    run_catalog_integrity_test(WINDOWS2_ENV, DataSourcePlatform::Windows);
}

#[test]
#[ignore = "requires FORENSICS_LINUX1_E01_FIXTURE real Linux E01 sample"]
fn linux1_catalog_integrity() {
    run_catalog_integrity_test(LINUX1_ENV, DataSourcePlatform::Linux);
}

#[test]
#[ignore = "requires FORENSICS_LINUX2_E01_FIXTURE real Linux E01 sample"]
fn linux2_catalog_integrity() {
    run_catalog_integrity_test(LINUX2_ENV, DataSourcePlatform::Linux);
}
