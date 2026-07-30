// Whole-volume synthesis needs a boot sector, a metadata block, and the AES-CCM
// wrap, so it lives in its own file. The test-layout guard requires the `src`
// bridge to be exactly `mod tests;`, which means a helper here cannot be shared
// with the other test modules — each of those builds only the fixture its own
// module needs instead.
#[path = "support.rs"]
mod support;

use std::io::Cursor;

use super::*;
use crate::secret::RecoveredVmk;
use crate::unlock_vmk::unlock_volume_with_recovered_vmk;
use crate::EncryptionMethod;
use support::{build_volume, Credential, VolumeSpec, META_BLOCK_OFFSET, TEST_ITERATIONS};

/// The password every synthetic password-protected volume uses.
const TEST_PASSWORD: &str = "bde-TEST";
/// A structurally valid recovery password for the synthetic volumes.
const TEST_RECOVERY: &str = "590238-514580-359986-088242-029766-319495-410509-636911";

fn identity_of(image: Vec<u8>) -> Result<VolumeIdentity> {
    read_volume_identity(&mut Cursor::new(image))
}

/// Derives a key package at the reduced test stretch cost.
///
/// The synthetic volumes are built with [`TEST_ITERATIONS`], so the unlock must
/// use the same count. `full_stretch_matches_the_production_path` is what covers
/// the real 0x100000-round path.
fn unlock_at_test_cost(metadata: &FveMetadata, password: &Passphrase) -> Result<VolumeKeyPackage> {
    let hash = password_hash(password.expose_for_derivation());
    derive_key_package(
        metadata,
        ProtectorKind::Password,
        PROTECTION_PASSWORD,
        &hash,
        TEST_ITERATIONS,
    )
}

/// [`unlock_at_test_cost`] for the recovery-password protector.
fn unlock_recovery_at_test_cost(
    metadata: &FveMetadata,
    recovery: &Passphrase,
) -> Result<VolumeKeyPackage> {
    let hash = recovery_key_hash(recovery.expose_for_derivation())
        .map_err(|_| BitLockerError::CredentialRejected)?;
    derive_key_package(
        metadata,
        ProtectorKind::RecoveryPassword,
        PROTECTION_RECOVERY,
        &hash,
        TEST_ITERATIONS,
    )
}

/// Unwraps the error from a key-derivation result.
///
/// `Result::expect_err` requires `T: Debug`, and `VolumeKeyPackage` deliberately
/// has no `Debug` — deriving one to make tests convenient is exactly what
/// `check-bitlocker-credential-guard.ps1` forbids. Matching avoids the bound.
fn expect_error(result: Result<VolumeKeyPackage>, context: &str) -> BitLockerError {
    match result {
        Ok(_) => panic!("expected a failure: {context}"),
        Err(error) => error,
    }
}

#[test]
fn reads_the_identity_of_a_synthetic_volume() {
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("synthetic volume is readable");
    assert_eq!(identity.bytes_per_sector, 512);
    assert_eq!(
        identity.metadata.encryption_method,
        EncryptionMethod::Aes128CbcDiffuser
    );
    assert!(identity
        .metadata
        .protector_inventory()
        .has_unlockable_protector());
}

#[test]
fn recovered_vmk_must_authenticate_the_wrapped_fvek() {
    let volume = build_volume(&VolumeSpec {
        method: 0x8002,
        fvek_len: 16,
        with_tweak: false,
        protectors: &[Credential::Recovery(TEST_RECOVERY)],
        inventory_only: &[],
        with_fvek_entry: true,
        with_block_signature: true,
        iterations: TEST_ITERATIONS,
        with_encrypted_content: false,
    });
    let mut reader = std::io::Cursor::new(volume.image.clone());
    let unlock = unlock_volume_with_recovered_vmk(&mut reader, &RecoveredVmk::new([0x44; 32]))
        .expect("the structurally recovered VMK must authenticate the FVEK");
    assert_eq!(unlock.identity().metadata.encryption_method_code, 0x8002);

    let mut reader = std::io::Cursor::new(volume.image);
    let error = match unlock_volume_with_recovered_vmk(&mut reader, &RecoveredVmk::new([0x45; 32]))
    {
        Ok(_) => panic!("a wrong VMK must fail the FVEK CCM tag"),
        Err(error) => error,
    };
    assert!(matches!(error, BitLockerError::CredentialRejected));
}

#[test]
fn identity_requires_a_valid_metadata_block() {
    // An MSWIN4.1 boot sector with no FVE block is plain FAT, not BitLocker.
    let volume = build_volume(&VolumeSpec {
        with_block_signature: false,
        ..VolumeSpec::default()
    });
    let error = identity_of(volume.image).expect_err("plain FAT must not read as BitLocker");
    assert_eq!(error.code(), "BITLOCKER_METADATA_UNREADABLE");
}

#[test]
fn identity_skips_a_zero_offset_and_uses_a_later_copy() {
    // The three metadata copies exist so a damaged block does not lose the
    // volume. Blank the first offset and point the second at the real block.
    let mut volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    volume.image[440..448].copy_from_slice(&0u64.to_le_bytes());
    volume.image[448..456].copy_from_slice(&META_BLOCK_OFFSET.to_le_bytes());
    assert!(identity_of(volume.image).is_ok());
}

#[test]
fn identity_tries_every_copy_before_failing() {
    // First two offsets point at unsignatured garbage, the third at the block.
    let mut volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    volume.image[440..448].copy_from_slice(&0x2000u64.to_le_bytes());
    volume.image[448..456].copy_from_slice(&0x2800u64.to_le_bytes());
    volume.image[456..464].copy_from_slice(&META_BLOCK_OFFSET.to_le_bytes());
    assert!(identity_of(volume.image).is_ok());
}

#[test]
fn identity_continues_after_a_copy_read_failure() {
    let mut volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    volume.image[440..448].copy_from_slice(&0x8000u64.to_le_bytes());
    volume.image[448..456].copy_from_slice(&META_BLOCK_OFFSET.to_le_bytes());
    assert!(identity_of(volume.image).is_ok());
}

#[test]
fn volume_unlock_falls_back_after_a_complete_copy_fails_key_validation() {
    let mut volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    let first = META_BLOCK_OFFSET as usize;
    let metadata_size = u32::from_le_bytes(
        volume.image[first + 64..first + 68]
            .try_into()
            .expect("fixed metadata-size slice"),
    ) as usize;
    let block_len = 64 + metadata_size;
    let healthy_copy = volume.image[first..first + block_len].to_vec();
    let second = 0x2000usize;
    volume.image[second..second + block_len].copy_from_slice(&healthy_copy);
    volume.image[448..456].copy_from_slice(&(second as u64).to_le_bytes());

    // Both copies remain structurally valid, but the first advertises a cipher
    // that cannot build an unlocked reader. The second must still be attempted.
    volume.image[first + 64 + 36..first + 64 + 38].copy_from_slice(&0x8001u16.to_le_bytes());
    let hash = password_hash(TEST_PASSWORD);
    let unlocked = unlock_volume_with_hash(
        &mut Cursor::new(volume.image),
        ProtectorKind::Password,
        PROTECTION_PASSWORD,
        &hash,
        TEST_ITERATIONS,
    )
    .expect("healthy redundant copy unlocks");
    assert_eq!(
        unlocked.identity().metadata.encryption_method,
        EncryptionMethod::Aes128CbcDiffuser
    );
}

#[test]
fn identity_error_names_the_candidate_offsets() {
    let volume = build_volume(&VolumeSpec {
        with_block_signature: false,
        ..VolumeSpec::default()
    });
    let error = identity_of(volume.image).expect_err("must fail");
    let rendered = error.to_string();
    assert!(rendered.contains("candidate offsets"), "got: {rendered}");
}

#[test]
fn identity_propagates_a_reader_failure() {
    // A truncated image cannot even yield the header sector.
    let error = read_volume_identity(&mut Cursor::new(vec![0u8; 16]))
        .expect_err("a 16-byte image has no header");
    assert_eq!(error.code(), "BITLOCKER_EVIDENCE_READ_FAILED");
}

#[test]
fn unlocks_a_diffuser_volume_with_a_password() {
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    let expected_fvek = volume.fvek.clone();
    let expected_tweak = volume.tweak.clone();
    let identity = identity_of(volume.image).expect("readable");

    let package = unlock_at_test_cost(
        &identity.metadata,
        &Passphrase::new(TEST_PASSWORD.to_string()),
    )
    .expect("correct password must unlock");
    assert_eq!(package.expose_fvek(), expected_fvek.as_slice());
    assert_eq!(package.expose_tweak(), expected_tweak.as_deref());
}

#[test]
fn unlocks_with_a_recovery_password() {
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Recovery(TEST_RECOVERY)],
        ..VolumeSpec::default()
    });
    let expected_fvek = volume.fvek.clone();
    let identity = identity_of(volume.image).expect("readable");

    let package = unlock_recovery_at_test_cost(
        &identity.metadata,
        &Passphrase::new(TEST_RECOVERY.to_string()),
    )
    .expect("correct recovery password must unlock");
    assert_eq!(package.expose_fvek(), expected_fvek.as_slice());
}

#[test]
fn a_wrong_password_is_rejected_by_the_authentication_tag() {
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("readable");
    let error = expect_error(
        unlock_at_test_cost(&identity.metadata, &Passphrase::new("wrong".to_string())),
        "a wrong password must not unlock",
    );
    assert_eq!(error.code(), "BITLOCKER_CREDENTIAL_REJECTED");
    assert!(error.is_retryable_with_credential());
}

#[test]
fn a_malformed_recovery_password_is_rejected_as_a_credential_failure() {
    // Structurally invalid and simply wrong must be indistinguishable to a
    // caller, so neither reveals which of the two it was.
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Recovery(TEST_RECOVERY)],
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("readable");
    let error = expect_error(
        unlock_recovery_at_test_cost(
            &identity.metadata,
            &Passphrase::new("000001-000000-000000-000000-000000-000000-000000-000000".to_string()),
        ),
        "a failed checksum must be rejected",
    );
    assert_eq!(error.code(), "BITLOCKER_CREDENTIAL_REJECTED");
}

#[test]
fn a_missing_protector_reports_the_inventory_instead() {
    // Asking for a password on a recovery-only volume must say what the volume
    // does carry, which is the actionable forensic answer.
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Recovery(TEST_RECOVERY)],
        inventory_only: &[0x0100],
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("readable");
    let error = expect_error(
        unlock_at_test_cost(
            &identity.metadata,
            &Passphrase::new(TEST_PASSWORD.to_string()),
        ),
        "no password protector exists",
    );
    assert_eq!(error.code(), "BITLOCKER_UNSUPPORTED_PROTECTOR");
    let rendered = error.to_string();
    assert!(rendered.contains("recovery password"), "got: {rendered}");
    assert!(rendered.contains("TPM"), "got: {rendered}");
}

#[test]
fn an_unsupported_method_is_refused_before_any_credential_work() {
    // 0x8001 is recognized but has no validated decrypt path. Refusing before
    // the stretch also means an unsupported volume fails fast rather than after
    // a million SHA-256 rounds.
    let volume = build_volume(&VolumeSpec {
        method: 0x8001,
        fvek_len: 32,
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("readable");
    let error = expect_error(
        unlock_at_test_cost(
            &identity.metadata,
            &Passphrase::new(TEST_PASSWORD.to_string()),
        ),
        "0x8001 must be refused",
    );
    assert_eq!(error.code(), "BITLOCKER_UNSUPPORTED_METHOD");
    assert!(
        !error.is_retryable_with_credential(),
        "no credential can make an unsupported cipher work"
    );
    assert!(error.to_string().contains("0x8001"));
}

#[test]
fn an_unknown_method_is_refused() {
    let volume = build_volume(&VolumeSpec {
        method: 0x8009,
        protectors: &[Credential::Password(TEST_PASSWORD)],
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("readable");
    let error = expect_error(
        unlock_at_test_cost(
            &identity.metadata,
            &Passphrase::new(TEST_PASSWORD.to_string()),
        ),
        "an unknown cipher must be refused",
    );
    assert_eq!(error.code(), "BITLOCKER_UNSUPPORTED_METHOD");
}

#[test]
fn a_missing_fvek_entry_is_reported_as_unreadable_metadata() {
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        with_fvek_entry: false,
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("readable");
    let error = expect_error(
        unlock_at_test_cost(
            &identity.metadata,
            &Passphrase::new(TEST_PASSWORD.to_string()),
        ),
        "no FVEK entry",
    );
    assert_eq!(error.code(), "BITLOCKER_METADATA_UNREADABLE");
    assert!(error.to_string().contains("FVEK"));
}

#[test]
fn a_clear_key_only_volume_cannot_be_unlocked_with_a_password() {
    // v1 reports clear key but never uses it, so a clear-key-only volume has no
    // password protector to try.
    let volume = build_volume(&VolumeSpec {
        inventory_only: &[0x0000],
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("readable");
    assert_eq!(
        identity.metadata.protector_inventory().protectors(),
        &[ProtectorKind::ClearKey]
    );
    let error = expect_error(
        unlock_at_test_cost(
            &identity.metadata,
            &Passphrase::new(TEST_PASSWORD.to_string()),
        ),
        "clear key is not an unlock path in v1",
    );
    assert_eq!(error.code(), "BITLOCKER_UNSUPPORTED_PROTECTOR");
}

#[test]
fn selects_the_matching_protector_when_several_are_present() {
    // Both credentials wrap the same VMK, so both must reach the same FVEK.
    let volume = build_volume(&VolumeSpec {
        protectors: &[
            Credential::Password(TEST_PASSWORD),
            Credential::Recovery(TEST_RECOVERY),
        ],
        ..VolumeSpec::default()
    });
    let expected_fvek = volume.fvek.clone();
    let identity = identity_of(volume.image).expect("readable");

    let by_password = unlock_at_test_cost(
        &identity.metadata,
        &Passphrase::new(TEST_PASSWORD.to_string()),
    )
    .expect("password protector present");
    let by_recovery = unlock_recovery_at_test_cost(
        &identity.metadata,
        &Passphrase::new(TEST_RECOVERY.to_string()),
    )
    .expect("recovery protector present");

    assert_eq!(by_password.expose_fvek(), expected_fvek.as_slice());
    assert_eq!(by_recovery.expose_fvek(), expected_fvek.as_slice());
}

#[test]
fn full_stretch_matches_the_production_path() {
    // The one test that pays the real 0x100000-round cost. Everything else uses a
    // reduced count, so without this the shipped derivation would be asserted only
    // by a constant check and never actually executed end to end.
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        iterations: crate::kdf::STRETCH_ITERATIONS,
        ..VolumeSpec::default()
    });
    let expected_fvek = volume.fvek.clone();
    let identity = identity_of(volume.image).expect("readable");

    let hash = password_hash(TEST_PASSWORD);
    let package = derive_key_package(
        &identity.metadata,
        ProtectorKind::Password,
        PROTECTION_PASSWORD,
        &hash,
        STRETCH_ITERATIONS,
    )
    .expect("the production derivation must unlock a volume built at the same count");
    assert_eq!(package.expose_fvek(), expected_fvek.as_slice());
}

#[test]
fn a_reduced_stretch_does_not_unlock_a_real_volume() {
    // Guards the inverse: if the production count were ever lowered to match the
    // test count, this would start passing.
    let volume = build_volume(&VolumeSpec {
        protectors: &[Credential::Password(TEST_PASSWORD)],
        iterations: crate::kdf::STRETCH_ITERATIONS,
        ..VolumeSpec::default()
    });
    let identity = identity_of(volume.image).expect("readable");
    let error = expect_error(
        unlock_at_test_cost(
            &identity.metadata,
            &Passphrase::new(TEST_PASSWORD.to_string()),
        ),
        "a short stretch must not derive the real key",
    );
    assert_eq!(error.code(), "BITLOCKER_CREDENTIAL_REJECTED");
}

#[test]
fn the_whole_chain_reaches_plaintext() {
    // The Stage 2 integration proof: parse the volume, unlock it with a password,
    // build the reader, and read real plaintext back. Each layer is unit-tested
    // separately; this is the one test that shows they compose.
    //
    // Logical offset 0 is the relocated header, so a pass here also confirms the
    // physical offset — not the logical one — reached the cipher.
    use std::io::Cursor;
    use std::sync::Arc;

    let volume = build_volume(&VolumeSpec {
        method: 0x8002,
        fvek_len: 16,
        with_tweak: false,
        protectors: &[Credential::Password(TEST_PASSWORD)],
        with_encrypted_content: true,
        ..VolumeSpec::default()
    });
    let image = volume.image.clone();

    let identity = read_volume_identity(&mut Cursor::new(image.clone())).expect("volume parses");
    let keys = unlock_at_test_cost(
        &identity.metadata,
        &Passphrase::new(TEST_PASSWORD.to_string()),
    )
    .expect("the password unlocks");

    let unlocked = Arc::new(
        crate::reader::UnlockedVolume::new(&identity.metadata, &keys).expect("cipher builds"),
    );
    let mut reader =
        crate::reader::BitLockerReader::new(unlocked, Cursor::new(image)).expect("reader opens");

    let mut got = [0u8; 512];
    reader.read_at(0, &mut got).expect("read succeeds");
    assert_eq!(
        got,
        crate::unlock::tests::support::expected_relocated_plaintext(),
        "the decrypted relocated header must match the plaintext it was built from"
    );
}

#[test]
fn key_lengths_follow_the_encryption_method() {
    for (method, fvek_len, with_tweak) in [
        (0x8000u16, 16usize, true),
        (0x8002, 16, false),
        (0x8003, 32, false),
        (0x8004, 32, false),
        (0x8005, 64, false),
    ] {
        let volume = build_volume(&VolumeSpec {
            method,
            fvek_len,
            with_tweak,
            protectors: &[Credential::Password(TEST_PASSWORD)],
            ..VolumeSpec::default()
        });
        let identity = identity_of(volume.image).expect("readable");
        let package = unlock_at_test_cost(
            &identity.metadata,
            &Passphrase::new(TEST_PASSWORD.to_string()),
        )
        .unwrap_or_else(|error| panic!("method {method:#06X} must unlock: {error}"));
        assert_eq!(
            package.expose_fvek().len(),
            fvek_len,
            "method {method:#06X} FVEK length"
        );
        assert_eq!(
            package.expose_tweak().is_some(),
            with_tweak,
            "method {method:#06X} tweak presence"
        );
    }
}
