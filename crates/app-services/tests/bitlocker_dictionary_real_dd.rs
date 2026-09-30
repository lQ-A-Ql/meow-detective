//! Real BitLocker raw-image dictionary benchmark.
//!
//! Set `FORENSICS_BITLOCKER_DICTIONARY_IMAGE` and
//! `FORENSICS_BITLOCKER_DICTIONARY_PATH` before running with `--include-ignored`.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Instant;

use app_services::bitlocker_runtime::BitLockerUnlockRegistry;
use app_services::bitlocker_service::{
    BitLockerKeyStore, BitLockerKeyStoreError, BitLockerRuntimeContext, DictionaryAttackOutcome,
    DictionaryAttackRequest,
};
use app_services::file_service::PreviewRuntimeRegistry;
use domain::{DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::{
    audit_repo::AuditRepo,
    datasource_repo::DataSourceRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use sha2::{Digest, Sha256};
use volume_bitlocker::{MetadataFingerprint, PersistedKeyBlob};

#[derive(Default)]
struct DiscardingKeyStore {
    stores: AtomicUsize,
}

impl BitLockerKeyStore for DiscardingKeyStore {
    fn load(
        &self,
        _: &MetadataFingerprint,
    ) -> Result<Option<PersistedKeyBlob>, BitLockerKeyStoreError> {
        Ok(None)
    }
    fn store(
        &self,
        _: &MetadataFingerprint,
        _: PersistedKeyBlob,
    ) -> Result<(), BitLockerKeyStoreError> {
        self.stores.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn delete(&self, _: &MetadataFingerprint) -> Result<bool, BitLockerKeyStoreError> {
        Ok(false)
    }
}

fn env_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {name}"))
}

fn sha256_file(path: &PathBuf) -> String {
    let mut file = File::open(path).expect("open file for hash");
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).expect("read file for hash");
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    hex::encode(digest.finalize())
}

#[test]
#[ignore = "requires real BitLocker raw image and dictionary paths"]
fn real_raw_bitlocker_dictionary_attack_uses_parallel_kdf() {
    let image = env_path("FORENSICS_BITLOCKER_DICTIONARY_IMAGE");
    let dictionary = env_path("FORENSICS_BITLOCKER_DICTIONARY_PATH");
    let max_candidates = std::env::var("FORENSICS_BITLOCKER_MAX_CANDIDATES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());
    let length = std::fs::metadata(&image).expect("image metadata").len();
    let image_sha256_before = sha256_file(&image);
    let dictionary_sha256 = sha256_file(&dictionary);
    let temp = tempfile::TempDir::new().expect("temp root");
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "bitlocker-dictionary",
        Some("test"),
    )
    .expect("case");
    let source = active
        .with_conn(|conn| {
            app_services::datasource_service::attach_data_source(
                conn,
                &active.meta.id,
                "BitLocker raw",
                &image,
                DataSourceKind::Raw,
                DataSourcePlatform::Windows,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("attach source");
    let source_conn =
        app_services::source_db::open_source_db(&active.case_root, &source.id).expect("source db");
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&active.meta.id, &source)
        .expect("source metadata");
    PartitionRepo::new(&source_conn)
        .replace_for_data_source(
            &source.id.0,
            &[DataSourcePartitionRecord {
                id: format!("{}:partition:0", source.id.0),
                data_source_id: source.id.0.clone(),
                partition_index: 0,
                name: "BitLocker raw".to_string(),
                kind_label: "BitLocker".to_string(),
                status: "encrypted_bitlocker".to_string(),
                type_guid: None,
                offset: 0,
                length,
                filesystem: Some("BitLocker".to_string()),
                unlock_hint: None,
                lvm_vg_uuid: None,
                lvm_vg_name: None,
                lvm_lv_uuid: None,
                lvm_lv_name: None,
                lvm_pv_offsets_json: None,
                lvm_pv_sources_json: None,
            }],
        )
        .expect("partition metadata");
    active
        .with_conn(|conn| DataSourceRepo::new(conn).update_import_state(&source.id, "ready", None))
        .expect("ready state");
    let preview = Arc::new(PreviewRuntimeRegistry::default());
    let unlock = Arc::new(BitLockerUnlockRegistry::default());
    let store = DiscardingKeyStore::default();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let started = Instant::now();
    let mut last_report = Instant::now();
    let result = active
        .with_conn(|conn| {
            app_services::bitlocker_service::try_password_dictionary(
                DictionaryAttackRequest {
                    case_conn: conn,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                    data_source_id: &source.id,
                    partition_index: 0,
                    dictionary_path: &dictionary,
                    backend: match std::env::var("FORENSICS_BITLOCKER_DICTIONARY_BACKEND")
                        .as_deref()
                    {
                        Ok("gpu") => transport::dto::BitLockerDictionaryBackendDto::Gpu,
                        Ok("cpu") | Err(_) => transport::dto::BitLockerDictionaryBackendDto::Cpu,
                        _ => panic!("backend must be cpu or gpu"),
                    },
                    runtimes: BitLockerRuntimeContext::new(&preview, &unlock, &store),
                    cancel_token: &cancel,
                },
                |progress| {
                    if max_candidates.is_some_and(|limit| progress.tested_candidates >= limit) {
                        cancel.store(true, std::sync::atomic::Ordering::Release);
                    }
                    if last_report.elapsed().as_secs() >= 5 {
                        let rate =
                            progress.tested_candidates as f64 / started.elapsed().as_secs_f64();
                        eprintln!(
                            "tested={} bytes={} rate={:.2}/s elapsed={:?}",
                            progress.tested_candidates,
                            progress.bytes_processed,
                            rate,
                            started.elapsed()
                        );
                        last_report = Instant::now();
                    }
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .expect("dictionary attack");
    let progress = match &result {
        DictionaryAttackOutcome::Found { progress }
        | DictionaryAttackOutcome::Exhausted { progress }
        | DictionaryAttackOutcome::Cancelled { progress } => *progress,
    };
    let outcome_name = match result {
        DictionaryAttackOutcome::Found { .. } => "found",
        DictionaryAttackOutcome::Exhausted { .. } => "exhausted",
        DictionaryAttackOutcome::Cancelled { .. } => "cancelled",
    };
    eprintln!(
        "result={} tested={} bytes={} elapsed={:?}",
        outcome_name,
        progress.tested_candidates,
        progress.bytes_processed,
        started.elapsed()
    );

    assert!(
        progress.tested_candidates > 0,
        "the real dictionary must test candidates"
    );
    assert!(
        progress.bytes_processed > 0,
        "the real dictionary must be read"
    );
    if let Some(limit) = max_candidates {
        assert!(
            matches!(outcome_name, "found" | "cancelled"),
            "bounded run must either find a credential or stop at its limit"
        );
        if outcome_name == "cancelled" {
            assert!(progress.tested_candidates >= limit);
        } else {
            assert!(progress.tested_candidates <= limit.saturating_add(2048));
        }
    } else {
        assert_eq!(
            outcome_name, "found",
            "full oracle dictionary must unlock the image"
        );
    }

    let image_sha256_after = sha256_file(&image);
    assert_eq!(
        image_sha256_before, image_sha256_after,
        "evidence image changed"
    );
    let dictionary_sha256_after = sha256_file(&dictionary);
    assert_eq!(
        dictionary_sha256, dictionary_sha256_after,
        "dictionary changed"
    );

    let audits = active
        .with_conn(|conn| {
            AuditRepo::new(conn).query(
                Some(&active.meta.id.0),
                Some("bitlocker.password_dictionary"),
                10,
                0,
            )
        })
        .expect("dictionary audit query");
    let audit = audits.first().expect("dictionary attack must be audited");
    assert!(audit.details.contains(&format!(
        "\"testedCandidates\":{}",
        progress.tested_candidates
    )));
    assert!(audit.details.contains(&dictionary_sha256));

    if outcome_name == "found" {
        let status = active
            .with_conn(|conn| {
                app_services::bitlocker_service::inspect_bitlocker_volume(
                    conn,
                    &active.case_root,
                    &active.meta.id,
                    &source.id,
                    0,
                    BitLockerRuntimeContext::new(&preview, &unlock, &store),
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
            })
            .expect("inspect unlocked real image");
        assert!(status.unlocked, "found password must activate the volume");
        assert_eq!(status.plaintext_filesystem.as_deref(), Some("NTFS"));
    }
}
