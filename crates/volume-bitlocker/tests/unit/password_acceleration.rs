use super::*;

#[test]
fn batch_initial_hashes_preserve_utf16_unicode_and_candidate_order() {
    let passwords = ["", "ASCII", "密码🔒"];
    let secrets = passwords
        .iter()
        .map(|value| Passphrase::new(value.to_string()))
        .collect::<Vec<_>>();
    let hashes = PasswordAccelerationTarget::initial_hashes(&secrets);
    assert_eq!(hashes.len(), passwords.len());
    for (index, password) in passwords.iter().enumerate() {
        let expected = password_hash(password);
        for (i, word) in hashes[index].iter().enumerate() {
            assert_eq!(word.to_be_bytes(), expected[i * 4..i * 4 + 4]);
        }
    }
}

#[test]
fn empty_metadata_is_not_an_acceleration_target() {
    assert!(matches!(
        PasswordAccelerationTarget::new(&[]),
        Err(BitLockerError::CredentialRejected)
    ));
}
