use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use volume_bitlocker::{
    unlock_volume_with_password, BitLockerReader, EncryptionMethod, Passphrase,
};

fn oracle_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {name} to the local public BitLocker oracle image"))
}

fn assert_password_oracle(
    path_env: &str,
    password: &str,
    expected_method: EncryptionMethod,
    expected_oem: Option<&[u8; 8]>,
) {
    let path = oracle_path(path_env);
    let mut evidence = File::open(&path).expect("open read-only BitLocker oracle");
    let credential = Passphrase::new(password.to_string());
    let verified = unlock_volume_with_password(&mut evidence, &credential)
        .expect("public oracle credential must unlock every supported layer");
    assert_eq!(
        verified.identity().metadata.encryption_method,
        expected_method
    );

    let (_, volume) = verified.into_unlocked_volume();
    let mut plaintext = BitLockerReader::new(volume, evidence).expect("build plaintext reader");
    let mut boot_sector = [0u8; 512];
    plaintext
        .read_exact(&mut boot_sector)
        .expect("read decrypted filesystem boot sector");
    assert_eq!(&boot_sector[510..512], &[0x55, 0xAA]);
    assert_ne!(&boot_sector[3..11], b"-FVE-FS-");
    if let Some(oem) = expected_oem {
        assert_eq!(&boot_sector[3..11], oem);
    }
}

#[test]
#[ignore = "requires FORENSICS_BITLOCKER_BDETOGO_ORACLE public dfvfs image"]
fn bdetogo_diffuser_password_oracle_reaches_plaintext() {
    assert_password_oracle(
        "FORENSICS_BITLOCKER_BDETOGO_ORACLE",
        "bde-TEST",
        EncryptionMethod::Aes128CbcDiffuser,
        None,
    );
}

#[test]
#[ignore = "requires FORENSICS_BITLOCKER_BITLOCKER1_ORACLE public picoCTF image"]
fn bitlocker1_cbc_password_oracle_reaches_ntfs() {
    assert_password_oracle(
        "FORENSICS_BITLOCKER_BITLOCKER1_ORACLE",
        "jacqueline",
        EncryptionMethod::Aes128Cbc,
        Some(b"NTFS    "),
    );
}
