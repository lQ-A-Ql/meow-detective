use app_services::{
    bitlocker_runtime::BitLockerUnlockRegistry,
    bitlocker_service::{BitLockerKeyStore, BitLockerKeyStoreError, BitLockerRuntimeContext},
    datasource_service::{detect_image_filesystem, PartitionRecord, PartitionStatus},
    file_service::PreviewRuntimeRegistry,
};
use domain::{DataSourceKind, DataSourcePlatform};
use evidence_core::{EvidenceReader, FileSystemReader, PartitionWindowReader, ReaderInfo};
use image_e01::E01Reader;
use memory_windows::{scan_bitlocker_key_candidates, AesKeyBits, RawMemoryImage};
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
use tempfile::TempDir;
use volume_bitlocker::{
    build_memory_candidate_unlock, read_volume_identities, unlock_volume_with_recovery_password,
    BitLockerReader, EncryptionMethod, MemoryCandidateUnlock, MetadataFingerprint, Passphrase,
    PersistedKeyBlob,
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

struct CandidateEvidenceReader {
    inner: BitLockerReader<PartitionWindowReader>,
    info: ReaderInfo,
}

impl Read for CandidateEvidenceReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl Seek for CandidateEvidenceReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

impl EvidenceReader for CandidateEvidenceReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
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
#[ignore = "requires Liu Yang E01 and raw memory fixture; no credential is used"]
fn private_liuyang_memory_image_recovers_an_ntfs_boot_sector() {
    let e01_path = env_path("FORENSICS_BITLOCKER_PRIVATE_LIUYANG_E01");
    let memory_path = env_path("FORENSICS_LIUYANG_MEMORY_FIXTURE");
    let mut probe_reader = E01Reader::open(&e01_path).expect("open private E01 read-only");
    let probe = detect_image_filesystem(&mut probe_reader).expect("probe private E01");
    let partition = probe
        .partitions
        .iter()
        .find(|partition| partition.index == 5)
        .expect("expected Liu Yang BitLocker partition");
    let reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&e01_path).expect("reopen E01"));
    let mut window = PartitionWindowReader::new(reader, partition.offset, Some(partition.length))
        .expect("build bounded partition window");
    let identity = read_volume_identities(&mut window)
        .expect("read BitLocker metadata")
        .into_iter()
        .next()
        .expect("metadata identity");
    assert_eq!(
        identity.metadata.encryption_method,
        EncryptionMethod::XtsAes128
    );

    let mut memory = RawMemoryImage::open(&memory_path).expect("open raw memory read-only");
    let candidates = scan_bitlocker_key_candidates(&mut memory, 8_192, 256)
        .expect("scan bounded BitLocker key candidates");
    for (left_index, left) in candidates.iter().enumerate() {
        if left.bits() != AesKeyBits::Aes128 {
            continue;
        }
        for (right_index, right) in candidates.iter().enumerate() {
            if left_index == right_index
                || right.bits() != AesKeyBits::Aes128
                || left.pool_physical_address() != right.pool_physical_address()
            {
                continue;
            }
            let pending = build_memory_candidate_unlock(
                identity.clone(),
                left.recovered_key(),
                Some(right.recovered_key()),
            )
            .expect("construct XTS-128 candidate reader");
            if validates_independent_ntfs_oracles(&e01_path, partition, &pending) {
                return;
            }
        }
    }
    panic!("no memory-recovered AES candidate decrypted the BitLocker volume as NTFS");
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
            assert!(status.stored_key_available);
            assert_eq!(status.plaintext_filesystem.as_deref(), Some("NTFS"));
            assert_eq!(key_store.stores.load(Ordering::Relaxed), 1);
            Ok(())
        })
        .expect("complete isolated service regression");
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

fn validates_independent_ntfs_oracles(
    e01_path: &std::path::Path,
    partition: &PartitionRecord,
    pending: &MemoryCandidateUnlock,
) -> bool {
    validate_ntfs_file(
        e01_path,
        partition,
        pending,
        "$UpCase",
        Some(&[0, 0, 1, 0, 2, 0, 3, 0]),
    ) && validate_ntfs_file(e01_path, partition, pending, "$Bitmap", None)
}

fn validate_ntfs_file(
    e01_path: &std::path::Path,
    partition: &PartitionRecord,
    pending: &MemoryCandidateUnlock,
    file_path: &str,
    expected_prefix: Option<&[u8]>,
) -> bool {
    let reader: Box<dyn EvidenceReader> =
        Box::new(E01Reader::open(e01_path).expect("reopen E01 for candidate"));
    let window = PartitionWindowReader::new(reader, partition.offset, Some(partition.length))
        .expect("rebuild bounded partition window");
    let info = window.info().clone();
    let inner = match pending.reader(window) {
        Ok(reader) => reader,
        Err(_) => return false,
    };
    let plaintext = CandidateEvidenceReader { inner, info };
    let fs = match fs_ntfs::NtfsReader::open(Box::new(plaintext), 0) {
        Ok(reader) => reader,
        Err(_) => return false,
    };
    let mut file = match fs.open_file(file_path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut prefix = [0u8; 8];
    file.read_exact(&mut prefix).is_ok()
        && expected_prefix.is_none_or(|expected| prefix.starts_with(expected))
}

#[test]
#[ignore = "requires private JC2 BitLocker E01 path and recovery password"]
fn private_jc2_e01_recovery_password_unlocks_partition() {
    assert_private_sample(&PRIVATE_SAMPLES[1]);
}
