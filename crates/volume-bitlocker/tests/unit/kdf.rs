use super::*;

/// Wraps a plaintext into the on-disk AES-CCM layout `nonce(12) | tag(16) | ct`,
/// the inverse of [`aes_ccm_unwrap`].
///
/// Local to this module: production never writes to a volume, and the unlock
/// tests build whole volumes with their own copy. Sharing one helper across test
/// modules is not available here, because the test-layout guard requires the
/// bridge out of `src` to be exactly a private `mod tests;`.
fn wrap_key(key: &[u8; 32], nonce: &[u8; 12], plaintext: &[u8]) -> Vec<u8> {
    let cipher = <BitLockerCcm as KeyInit>::new(GenericArray::from_slice(key));
    let mut buffer = plaintext.to_vec();
    let tag = cipher
        .encrypt_in_place_detached(GenericArray::from_slice(nonce), &[], &mut buffer)
        .expect("in-memory encryption cannot fail");
    let mut out = Vec::with_capacity(28 + buffer.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&tag);
    out.extend_from_slice(&buffer);
    out
}

/// A published recovery password from the BelkaCTF #6 `vault.raw` sample. It is
/// public test data, not a live credential.
const PUBLIC_RECOVERY: &str = "590238-514580-359986-088242-029766-319495-410509-636911";

#[test]
fn password_hash_encodes_utf16le_without_bom_or_terminator() {
    // "A" is 0x41 0x00 in UTF-16LE, so the digest must equal the double SHA-256
    // of exactly those two bytes — no BOM, no NUL.
    let expected: [u8; 32] = Sha256::digest(Sha256::digest([0x41u8, 0x00])).into();
    assert_eq!(*password_hash("A"), expected);
}

#[test]
fn password_hash_handles_non_ascii() {
    // Non-BMP characters become surrogate pairs, four bytes in UTF-16LE.
    let hash = password_hash("密码🔒");
    assert_ne!(*hash, [0u8; 32]);
    assert_ne!(*hash, *password_hash("密码"));
}

#[test]
fn password_hash_is_deterministic_and_input_sensitive() {
    assert_eq!(*password_hash("bde-TEST"), *password_hash("bde-TEST"));
    assert_ne!(*password_hash("bde-TEST"), *password_hash("bde-test"));
}

#[test]
fn stretch_iteration_count_matches_the_format() {
    // The count is not tunable; a smaller one silently weakens every derived key.
    assert_eq!(STRETCH_ITERATIONS, 1_048_576);
}

#[test]
fn stretch_key_depends_on_the_salt() {
    let hash = [0x11u8; 32];
    let a = stretch_key_n(&hash, &[0x01; 16], 4);
    let b = stretch_key_n(&hash, &[0x02; 16], 4);
    assert_ne!(*a, *b, "the salt must reach the derived key");
}

#[test]
fn stretch_key_depends_on_the_iteration_count() {
    let hash = [0x11u8; 32];
    let salt = [0x33u8; 16];
    assert_ne!(
        *stretch_key_n(&hash, &salt, 1),
        *stretch_key_n(&hash, &salt, 2)
    );
}

#[test]
fn zero_iterations_returns_the_untouched_accumulator() {
    // Documents the loop's boundary: with no rounds the `last` field is still
    // zero, so a caller that accidentally passed 0 gets an obviously wrong key
    // rather than the credential hash itself.
    assert_eq!(*stretch_key_n(&[0x11; 32], &[0x33; 16], 0), [0u8; 32]);
}

#[test]
fn recovery_key_hash_accepts_a_valid_public_recovery_password() {
    let hash = recovery_key_hash(PUBLIC_RECOVERY).expect("published recovery password is valid");
    assert_ne!(*hash, [0u8; 32]);
}

#[test]
fn recovery_key_hash_derives_from_the_divided_words() {
    // Each group divided by 11 becomes a little-endian u16. All-zero groups give
    // an all-zero 16-byte key, so the result must be SHA-256 of sixteen zeros —
    // this pins the division and the packing, not just that something was hashed.
    let all_zero = "000000-000000-000000-000000-000000-000000-000000-000000";
    let expected: [u8; 32] = Sha256::digest([0u8; 16]).into();
    assert_eq!(*recovery_key_hash(all_zero).expect("valid"), expected);
}

#[test]
fn recovery_key_hash_rejects_a_failed_checksum() {
    // 000001 is not divisible by 11. Catching this before derivation matters: a
    // typo that slipped through would produce a wrong key that is
    // indistinguishable from a wrong password at the AES-CCM tag check.
    let typo = "000001-000000-000000-000000-000000-000000-000000-000000";
    assert_eq!(
        recovery_key_hash(typo).expect_err("must reject"),
        RecoveryPasswordError::Checksum
    );
}

#[test]
fn recovery_key_hash_rejects_wrong_group_counts() {
    let too_few = "000000-000000-000000";
    assert_eq!(
        recovery_key_hash(too_few).expect_err("must reject"),
        RecoveryPasswordError::GroupCount
    );
    let too_many = "000000-000000-000000-000000-000000-000000-000000-000000-000000";
    assert_eq!(
        recovery_key_hash(too_many).expect_err("must reject"),
        RecoveryPasswordError::GroupCount
    );
}

#[test]
fn recovery_key_hash_rejects_malformed_groups() {
    for malformed in [
        "00000-000000-000000-000000-000000-000000-000000-000000",
        "0000000-000000-000000-000000-000000-000000-000000-000000",
        "00000a-000000-000000-000000-000000-000000-000000-000000",
        "-00000-000000-000000-000000-000000-000000-000000-000000",
        "",
    ] {
        assert_eq!(
            recovery_key_hash(malformed).expect_err("must reject"),
            RecoveryPasswordError::GroupShape,
            "input: {malformed:?}"
        );
    }
}

#[test]
fn recovery_key_hash_rejects_an_out_of_range_group() {
    // 720896 / 11 = 65536, one past what a u16 word can hold.
    let out_of_range = "720896-000000-000000-000000-000000-000000-000000-000000";
    assert_eq!(
        recovery_key_hash(out_of_range).expect_err("must reject"),
        RecoveryPasswordError::OutOfRange
    );
}

#[test]
fn recovery_rejection_reasons_never_echo_the_input() {
    // These strings reach error details and therefore logs.
    for error in [
        RecoveryPasswordError::GroupCount,
        RecoveryPasswordError::GroupShape,
        RecoveryPasswordError::Checksum,
        RecoveryPasswordError::OutOfRange,
    ] {
        let reason = error.reason();
        assert!(!reason.is_empty());
        assert!(
            !reason.chars().any(|c| c.is_ascii_digit())
                || reason.contains("11")
                || reason.contains("8")
                || reason.contains("6")
                || reason.contains("16"),
            "reason must only contain structural numbers: {reason}"
        );
    }
}

#[test]
fn aes_ccm_unwrap_round_trips_a_wrapped_key() {
    let key = [0x42u8; 32];
    let plaintext = b"volume master key container".to_vec();
    let wrapped = wrap_key(&key, &[0x11; 12], &plaintext);
    let unwrapped = aes_ccm_unwrap(&key, &wrapped).expect("correct key must unwrap");
    assert_eq!(unwrapped.as_slice(), plaintext.as_slice());
}

#[test]
fn aes_ccm_unwrap_rejects_a_wrong_key() {
    let wrapped = wrap_key(&[0x42u8; 32], &[0x11; 12], b"secret");
    assert!(
        aes_ccm_unwrap(&[0x43u8; 32], &wrapped).is_none(),
        "the authentication tag must reject a wrong key"
    );
}

#[test]
fn aes_ccm_unwrap_rejects_a_tampered_ciphertext() {
    let key = [0x42u8; 32];
    let mut wrapped = wrap_key(&key, &[0x11; 12], b"secret payload");
    let last = wrapped.len() - 1;
    wrapped[last] ^= 0x01;
    assert!(aes_ccm_unwrap(&key, &wrapped).is_none());
}

#[test]
fn aes_ccm_unwrap_rejects_truncated_values_without_panicking() {
    let key = [0x42u8; 32];
    // Shorter than nonce + tag, so there is nothing to authenticate.
    for len in [0usize, 1, 11, 12, 27] {
        assert!(
            aes_ccm_unwrap(&key, &vec![0u8; len]).is_none(),
            "a {len}-byte value must be rejected"
        );
    }
}

#[test]
fn aes_ccm_unwrap_accepts_an_empty_payload() {
    // Exactly nonce + tag with no ciphertext is structurally valid; the caller's
    // key-length check is what rejects it, not the unwrap.
    let key = [0x42u8; 32];
    let wrapped = wrap_key(&key, &[0x11; 12], b"");
    let unwrapped = aes_ccm_unwrap(&key, &wrapped).expect("structurally valid");
    assert!(unwrapped.is_empty());
}
