use app_services::datasource_service::{detect_image_filesystem, PartitionStatus};
use evidence_core::{EvidenceReader, PartitionWindowReader};
use image_e01::E01Reader;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use volume_bitlocker::{
    read_volume_identities, unlock_volume_with_recovery_password, BitLockerReader, Passphrase,
};

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
#[ignore = "requires private JC2 BitLocker E01 path and recovery password"]
fn private_jc2_e01_recovery_password_unlocks_partition() {
    assert_private_sample(&PRIVATE_SAMPLES[1]);
}
