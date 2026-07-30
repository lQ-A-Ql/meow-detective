use app_services::{
    bitlocker_runtime::BitLockerUnlockRegistry,
    bitlocker_service::{BitLockerKeyStore, BitLockerKeyStoreError, BitLockerRuntimeContext},
    datasource_service::{detect_image_filesystem, PartitionRecord, PartitionStatus},
    file_service::PreviewRuntimeRegistry,
};
use domain::{DataSourceKind, DataSourcePlatform};
use evidence_core::{EvidenceReader, PartitionWindowReader};
use image_e01::E01Reader;
use memory_windows::{recover_vmks_structurally, TargetedKernelSearchLimits};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use volume_bitlocker::{
    read_volume_identities, recover_recovery_password, recovery_password_protectors,
    unlock_volume_with_recovered_vmk, unlock_volume_with_recovery_password, BitLockerReader,
    MetadataFingerprint, Passphrase, PersistedKeyBlob, RecoveredVmk,
};

#[derive(Default)]
struct DiscardingKeyStore {
    stores: AtomicUsize,
}

impl BitLockerKeyStore for DiscardingKeyStore {
    fn load(
        &self,
        _fingerprint: &MetadataFingerprint,
    ) -> Result<Option<PersistedKeyBlob>, BitLockerKeyStoreError> {
        Ok(None)
    }

    fn store(
        &self,
        _fingerprint: &MetadataFingerprint,
        _blob: PersistedKeyBlob,
    ) -> Result<(), BitLockerKeyStoreError> {
        self.stores.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn delete(&self, _fingerprint: &MetadataFingerprint) -> Result<bool, BitLockerKeyStoreError> {
        Ok(false)
    }
}

struct PrivateBitLockerSample {
    path_env: &'static str,
    credential_env: &'static str,
    partition_index: u32,
}

const PRIVATE_SAMPLES: &[PrivateBitLockerSample] = &[
    PrivateBitLockerSample {
        path_env: "FORENSICS_BITLOCKER_PRIVATE_LIUYANG_E01",
        credential_env: "FORENSICS_BITLOCKER_PRIVATE_LIUYANG_RECOVERY_PASSWORD",
        partition_index: 5,
    },
    PrivateBitLockerSample {
        path_env: "FORENSICS_BITLOCKER_PRIVATE_JC2_E01",
        credential_env: "FORENSICS_BITLOCKER_PRIVATE_JC2_RECOVERY_PASSWORD",
        partition_index: 4,
    },
];

fn env_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {name} to run the private BitLocker E01 regression"))
}

fn assert_private_sample(sample: &PrivateBitLockerSample) {
    let path = env_path(sample.path_env);
    let recovery_password = std::env::var(sample.credential_env).unwrap_or_else(|_| {
        panic!(
            "set {} to run the private BitLocker E01 regression",
            sample.credential_env
        )
    });

    let mut probe_reader = E01Reader::open(&path).expect("open private E01 read-only");
    let probe = detect_image_filesystem(&mut probe_reader).expect("probe private E01");
    let partition = probe
        .partitions
        .iter()
        .find(|partition| partition.index == sample.partition_index as usize)
        .unwrap_or_else(|| panic!("expected BitLocker partition {}", sample.partition_index));
    assert!(
        matches!(partition.status, PartitionStatus::EncryptedBitLocker),
        "partition {} must remain classified as BitLocker",
        sample.partition_index
    );

    let reader: Box<dyn EvidenceReader> =
        Box::new(E01Reader::open(&path).expect("reopen private E01 read-only"));
    let mut window = PartitionWindowReader::new(reader, partition.offset, Some(partition.length))
        .expect("build bounded BitLocker partition window");
    let identities = read_volume_identities(&mut window).expect("read BitLocker metadata");
    eprintln!(
        "private BitLocker sample partition={} metadata_copies={} method={:?} protectors={:?}",
        sample.partition_index,
        identities.len(),
        identities[0].metadata.encryption_method,
        identities[0].metadata.protector_inventory().protectors()
    );
    window
        .seek(SeekFrom::Start(0))
        .expect("rewind BitLocker partition window");
    let credential = Passphrase::new(recovery_password);
    let verified = unlock_volume_with_recovery_password(&mut window, &credential)
        .expect("private recovery password must unlock the BitLocker partition");
    let (_, volume) = verified.into_unlocked_volume();
    let mut plaintext = BitLockerReader::new(volume, window)
        .expect("build the read-only plaintext BitLocker reader");
    let mut boot_sector = [0u8; 512];
    plaintext
        .seek(SeekFrom::Start(0))
        .expect("seek plaintext boot sector");
    plaintext
        .read_exact(&mut boot_sector)
        .expect("read plaintext boot sector");
    assert_eq!(&boot_sector[510..512], &[0x55, 0xAA]);
    assert_ne!(&boot_sector[3..11], b"-FVE-FS-");
    assert!(
        [
            &b"NTFS    "[..],
            &b"EXFAT   "[..],
            &b"FAT32   "[..],
            &b"MSDOS5.0"[..]
        ]
        .iter()
        .any(|marker| &boot_sector[3..11] == *marker),
        "decrypted partition must expose a recognized filesystem OEM marker"
    );
}

#[test]
#[ignore = "requires private Liu Yang BitLocker E01 path and recovery password"]
fn private_liuyang_e01_recovery_password_unlocks_partition() {
    assert_private_sample(&PRIVATE_SAMPLES[0]);
}

#[test]
#[ignore = "requires private Liu Yang E01, VMK, and recovery-password oracle"]
fn private_liuyang_vmk_reconstructs_the_metadata_bound_recovery_password() {
    let e01_path = env_path("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_E01");
    let vmk = decode_vmk_env("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_VMK_HEX");
    let expected = std::env::var("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_RECOVERY_PASSWORD")
        .expect("set the private Liu Yang recovery-password oracle");
    let mut probe_reader = E01Reader::open(&e01_path).expect("open private E01 read-only");
    let probe = detect_image_filesystem(&mut probe_reader).expect("probe private E01");
    let partition = probe
        .partitions
        .iter()
        .find(|partition| partition.index == 5)
        .expect("expected Liu Yang BitLocker partition");
    let reader: Box<dyn EvidenceReader> =
        Box::new(E01Reader::open(&e01_path).expect("reopen private E01 read-only"));
    let mut window = PartitionWindowReader::new(reader, partition.offset, Some(partition.length))
        .expect("build bounded BitLocker partition window");
    let identities = read_volume_identities(&mut window).expect("read BitLocker metadata copies");
    let metadata = &identities
        .first()
        .expect("at least one valid metadata copy")
        .metadata;
    let protectors =
        recovery_password_protectors(metadata).expect("read recovery-password protectors");
    assert_eq!(
        protectors.len(),
        1,
        "private oracle must select one exact protector identity"
    );
    let recovered = recover_recovery_password(metadata, protectors[0], &RecoveredVmk::new(vmk))
        .expect("private VMK must authenticate the reverse recovery datum");

    assert_eq!(
        recovered.password().expose_for_authorized_reveal(),
        expected
    );
    assert_eq!(
        recovered.provenance().metadata_fingerprint(),
        MetadataFingerprint::from_metadata(metadata).as_str()
    );
}

fn decode_vmk_env(name: &str) -> [u8; 32] {
    let encoded = std::env::var(name).unwrap_or_else(|_| panic!("set {name} to the private VMK"));
    assert_eq!(encoded.len(), 64, "private VMK must be 32-byte hex");
    let mut vmk = [0u8; 32];
    for (index, byte) in vmk.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16)
            .expect("private VMK must contain only hexadecimal digits");
    }
    vmk
}

#[test]
#[ignore = "requires Liu Yang E01 and raw memory fixture; no credential is used"]
fn private_liuyang_memory_image_completes_the_production_unlock_service() {
    let e01_path = env_path("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_E01");
    let memory_path = env_path("FORENSICS_LIUYANG_MEMORY_FIXTURE");
    let mut probe_reader = E01Reader::open(&e01_path).expect("open private E01 read-only");
    let probe = detect_image_filesystem(&mut probe_reader).expect("probe private E01");
    let partition = probe
        .partitions
        .iter()
        .find(|partition| partition.index == 5)
        .expect("expected Liu Yang BitLocker partition");

    let temp = TempDir::new().expect("temporary isolated case root");
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "bitlocker-memory-service",
        Some("private opt-in service regression"),
    )
    .expect("create isolated case");
    let case_id = active.meta.id.clone();
    let preview_runtime = Arc::new(PreviewRuntimeRegistry::default());
    let bitlocker_runtime = Arc::new(BitLockerUnlockRegistry::default());
    let key_store = DiscardingKeyStore::default();

    active
        .with_conn(|case_conn| {
            let source = app_services::datasource_service::attach_data_source(
                case_conn,
                &case_id,
                "liuyang-memory-service",
                &e01_path,
                DataSourceKind::E01,
                DataSourcePlatform::Windows,
            )
            .expect("attach private E01");
            let source_conn =
                app_services::source_db::open_source_db(&active.case_root, &source.id)
                    .expect("open isolated source DB");
            DataSourceRepo::new(&source_conn)
                .upsert_source_local_metadata(&case_id, &source)
                .expect("persist source-local metadata");
            PartitionRepo::new(&source_conn)
                .replace_for_data_source(
                    &source.id.0,
                    &[bitlocker_partition_record(&source.id.0, partition)],
                )
                .expect("persist BitLocker partition metadata");
            DataSourceRepo::new(case_conn)
                .update_import_state(&source.id, "ready", None)
                .expect("mark source ready");

            let runtimes =
                BitLockerRuntimeContext::new(&preview_runtime, &bitlocker_runtime, &key_store);
            let started = Instant::now();
            let status = app_services::bitlocker_service::unlock_bitlocker_with_memory_image(
                case_conn,
                &active.case_root,
                &case_id,
                &source.id,
                partition.index as u32,
                &memory_path,
                runtimes,
            )
            .expect("recover and activate verified BitLocker volume");
            let elapsed = started.elapsed();

            assert!(status.unlocked);
            assert!(status.stored_key_available);
            assert_eq!(status.plaintext_filesystem.as_deref(), Some("NTFS"));
            assert_eq!(key_store.stores.load(Ordering::Relaxed), 1);
            assert!(elapsed <= Duration::from_secs(120));
            Ok(())
        })
        .expect("complete isolated service regression");
}

#[test]
#[ignore = "requires Liu Yang E01 and raw memory fixture; no credential is used"]
fn private_liuyang_structural_vmk_authentication_oracles() {
    let e01_path = env_path("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_E01");
    let memory_path = env_path("FORENSICS_LIUYANG_MEMORY_FIXTURE");
    let mut probe_reader = E01Reader::open(&e01_path).expect("open private E01 read-only");
    let probe = detect_image_filesystem(&mut probe_reader).expect("probe private E01");
    let partition = probe
        .partitions
        .iter()
        .find(|partition| partition.index == 5)
        .expect("expected Liu Yang BitLocker partition");
    let reader: Box<dyn EvidenceReader> =
        Box::new(E01Reader::open(&e01_path).expect("reopen private E01 read-only"));
    let mut window = PartitionWindowReader::new(reader, partition.offset, Some(partition.length))
        .expect("open BitLocker partition window");
    let identities = read_volume_identities(&mut window).expect("read BitLocker metadata");
    let target = identities.first().expect("at least one metadata copy");

    let profile = memory_windows::resolve_profile_for_image(&memory_path)
        .expect("resolve recovery profile from the memory image");
    let recovery = recover_vmks_structurally(
        &memory_path,
        &profile,
        target.metadata.volume_guid,
        TargetedKernelSearchLimits::default(),
    )
    .expect("recover structural VMKs");

    let mut fvek_authentications = 0usize;
    let mut reverse_authentications = 0usize;
    for vmk in recovery.into_vmks() {
        let reader: Box<dyn EvidenceReader> =
            Box::new(E01Reader::open(&e01_path).expect("reopen private E01 read-only"));
        let mut candidate_window =
            PartitionWindowReader::new(reader, partition.offset, Some(partition.length))
                .expect("open candidate partition window");
        if unlock_volume_with_recovered_vmk(&mut candidate_window, &vmk).is_ok() {
            fvek_authentications += 1;
        }
        if identities.iter().any(|identity| {
            recovery_password_protectors(&identity.metadata).is_ok_and(|protectors| {
                protectors.into_iter().any(|protector| {
                    recover_recovery_password(&identity.metadata, protector, &vmk).is_ok()
                })
            })
        }) {
            reverse_authentications += 1;
        }
    }
    eprintln!(
        "structural VMK oracle counts fvek={fvek_authentications} reverse={reverse_authentications}"
    );
    assert_eq!(fvek_authentications, 1);
    assert_eq!(
        reverse_authentications, 1,
        "the active device-context VMK both unlocks the volume and authenticates the recovery protector's reverse datum"
    );
}

fn bitlocker_partition_record(
    data_source_id: &str,
    partition: &PartitionRecord,
) -> DataSourcePartitionRecord {
    DataSourcePartitionRecord {
        id: format!("{data_source_id}:partition:{}", partition.index),
        data_source_id: data_source_id.to_string(),
        partition_index: partition.index as u32,
        name: partition.name.clone(),
        kind_label: "BitLocker".to_string(),
        status: "encrypted_bitlocker".to_string(),
        type_guid: partition.type_guid.clone(),
        offset: partition.offset,
        length: partition.length,
        filesystem: Some("BitLocker".to_string()),
        unlock_hint: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        lvm_pv_offsets_json: None,
        lvm_pv_sources_json: None,
    }
}

#[test]
#[ignore = "requires private JC2 BitLocker E01 path and recovery password"]
fn private_jc2_e01_recovery_password_unlocks_partition() {
    assert_private_sample(&PRIVATE_SAMPLES[1]);
}

#[test]
#[ignore = "requires Liu Yang E01, raw memory fixture, and the recovery-password oracle"]
fn private_liuyang_memory_unlock_reconstructs_the_expected_recovery_password() {
    let e01_path = env_path("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_E01");
    let memory_path = env_path("FORENSICS_LIUYANG_MEMORY_FIXTURE");
    let expected = std::env::var("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_RECOVERY_PASSWORD")
        .expect("set FORENSICS_BITLOCKER_PRIVATE_LIUYANG_RECOVERY_PASSWORD to the 48-digit oracle");
    let mut probe_reader = E01Reader::open(&e01_path).expect("open private E01 read-only");
    let probe = detect_image_filesystem(&mut probe_reader).expect("probe private E01");
    let partition = probe
        .partitions
        .iter()
        .find(|partition| partition.index == 5)
        .expect("expected Liu Yang BitLocker partition");

    let temp = TempDir::new().expect("temporary isolated case root");
    let active = app_services::case_service::create_case(
        &temp.path().join("cases"),
        "bitlocker-memory-recovery-password",
        Some("private opt-in recovery-password regression"),
    )
    .expect("create isolated case");
    let case_id = active.meta.id.clone();
    let preview_runtime = Arc::new(PreviewRuntimeRegistry::default());
    let bitlocker_runtime = Arc::new(BitLockerUnlockRegistry::default());
    let key_store = DiscardingKeyStore::default();

    active
        .with_conn(|case_conn| {
            let source = app_services::datasource_service::attach_data_source(
                case_conn,
                &case_id,
                "liuyang-memory-recovery-password",
                &e01_path,
                DataSourceKind::E01,
                DataSourcePlatform::Windows,
            )
            .expect("attach private E01");
            let source_conn =
                app_services::source_db::open_source_db(&active.case_root, &source.id)
                    .expect("open isolated source DB");
            DataSourceRepo::new(&source_conn)
                .upsert_source_local_metadata(&case_id, &source)
                .expect("persist source-local metadata");
            PartitionRepo::new(&source_conn)
                .replace_for_data_source(
                    &source.id.0,
                    &[bitlocker_partition_record(&source.id.0, partition)],
                )
                .expect("persist BitLocker partition metadata");
            DataSourceRepo::new(case_conn)
                .update_import_state(&source.id, "ready", None)
                .expect("mark source ready");

            let runtimes =
                BitLockerRuntimeContext::new(&preview_runtime, &bitlocker_runtime, &key_store);
            let status = app_services::bitlocker_service::unlock_bitlocker_with_memory_image(
                case_conn,
                &active.case_root,
                &case_id,
                &source.id,
                partition.index as u32,
                &memory_path,
                runtimes,
            )
            .expect("recover and activate verified BitLocker volume");

            assert!(status.unlocked);
            let reconstruction = status
                .recovery_password_reconstruction
                .expect("memory unlock must carry a reconstruction outcome");
            assert_eq!(reconstruction.status, "recovered");
            assert_eq!(reconstruction.password.as_deref(), Some(expected.as_str()));
            Ok(())
        })
        .expect("complete recovery-password regression");
}
