use super::*;
use crate::unlock_dictionary::unlock_identities_with_stretch;

fn password_identity() -> VolumeIdentity {
    identity_of(
        build_volume(&VolumeSpec {
            protectors: &[Credential::Password(TEST_PASSWORD)],
            ..VolumeSpec::default()
        })
        .image,
    )
    .expect("synthetic metadata")
}

#[test]
fn acceleration_target_requires_authenticated_keys_and_handles_damaged_copies() {
    let healthy = password_identity();
    let mut damaged = healthy.clone();
    damaged
        .metadata
        .entries
        .iter_mut()
        .find(|entry| entry.entry_type == crate::metadata::ENTRY_TYPE_FVEK)
        .expect("FVEK")
        .data[12] ^= 1;
    let target = crate::PasswordAccelerationTarget::new(&[damaged, healthy]).expect("target");
    assert_eq!(target.salts(), &[support::SALT]);
    assert!(matches!(
        target.verify(&[]),
        Err(BitLockerError::CredentialRejected)
    ));
    assert!(matches!(
        target.verify(&[[0; 32]]),
        Err(BitLockerError::CredentialRejected)
    ));
    let key = crate::kdf::stretch_key_n(
        &password_hash(TEST_PASSWORD),
        &support::SALT,
        TEST_ITERATIONS,
    );
    target
        .verify(&[*key])
        .expect("both CCM checks and healthy copy required");
}

#[test]
fn same_salt_is_stretched_once_but_each_metadata_copy_is_authenticated() {
    let healthy = password_identity();
    let mut damaged = healthy.clone();
    damaged
        .metadata
        .entries
        .iter_mut()
        .find(|entry| entry.entry_type == crate::metadata::ENTRY_TYPE_FVEK)
        .expect("FVEK")
        .data[12] ^= 1;
    let hash = password_hash(TEST_PASSWORD);
    let mut calls = 0;
    let verified = unlock_identities_with_stretch(
        &[damaged, healthy],
        ProtectorKind::Password,
        PROTECTION_PASSWORD,
        |salt| {
            calls += 1;
            crate::kdf::stretch_key_n(&hash, salt, TEST_ITERATIONS)
        },
    )
    .expect("healthy copy must authenticate after damaged FVEK");
    assert_eq!(calls, 1, "redundant salt must reuse the candidate KDF");
    assert_eq!(verified.identity().metadata.encryption_method_code, 0x8000);
}

#[test]
fn different_salts_use_independent_stretches() {
    let healthy = password_identity();
    let mut wrong_salt = healthy.clone();
    let vmk = wrong_salt
        .metadata
        .entries
        .iter_mut()
        .find(|entry| entry.is_vmk())
        .expect("VMK");
    // 28-byte VMK prefix + 8-byte nested stretch header + 4-byte method.
    vmk.data[40] ^= 1;
    let hash = password_hash(TEST_PASSWORD);
    let mut salts = Vec::new();
    unlock_identities_with_stretch(
        &[wrong_salt, healthy],
        ProtectorKind::Password,
        PROTECTION_PASSWORD,
        |salt| {
            salts.push(*salt);
            crate::kdf::stretch_key_n(&hash, salt, TEST_ITERATIONS)
        },
    )
    .expect("healthy salt must remain usable after a different salt fails");
    assert_eq!(salts.len(), 2);
    assert_ne!(salts[0], salts[1]);
}

#[test]
fn stretch_cache_does_not_cross_candidate_boundaries() {
    let identity = password_identity();
    for password in [TEST_PASSWORD, "wrong"] {
        let hash = password_hash(password);
        let mut calls = 0;
        let result = unlock_identities_with_stretch(
            std::slice::from_ref(&identity),
            ProtectorKind::Password,
            PROTECTION_PASSWORD,
            |salt| {
                calls += 1;
                crate::kdf::stretch_key_n(&hash, salt, TEST_ITERATIONS)
            },
        );
        assert_eq!(calls, 1);
        if password == TEST_PASSWORD {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(BitLockerError::CredentialRejected)));
        }
    }
}
