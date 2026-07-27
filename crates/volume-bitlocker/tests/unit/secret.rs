use super::*;

#[test]
fn passphrase_exposes_exactly_what_was_supplied() {
    let phrase = Passphrase::new("bde-TEST".to_string());
    assert_eq!(phrase.expose_for_derivation(), "bde-TEST");
    assert!(!phrase.is_empty());
}

#[test]
fn passphrase_reports_empty_input() {
    // An empty credential is a caller bug worth rejecting before the KDF runs.
    assert!(Passphrase::new(String::new()).is_empty());
}

#[test]
fn passphrase_preserves_recovery_password_form_verbatim() {
    // Group separators and digits must survive untouched: the mod-11 validation
    // in Stage 1 operates on the exact entered form.
    let recovery = "068002-479633-277629-623568-540826-435039-327756-375705";
    let phrase = Passphrase::new(recovery.to_string());
    assert_eq!(phrase.expose_for_derivation(), recovery);
}

#[test]
fn passphrase_preserves_non_ascii_credentials() {
    // BitLocker passwords are UTF-16LE at derivation time, but this type holds
    // UTF-8 as entered. Non-ASCII must round-trip without normalization.
    let phrase = Passphrase::new("密码-Ünïcode".to_string());
    assert_eq!(phrase.expose_for_derivation(), "密码-Ünïcode");
}

#[test]
fn key_package_exposes_fvek_and_optional_tweak() {
    let cbc = VolumeKeyPackage::new(vec![0xAA; 32], None);
    assert_eq!(cbc.expose_fvek(), [0xAA; 32]);
    assert!(
        cbc.expose_tweak().is_none(),
        "CBC methods carry no tweak key"
    );

    let xts = VolumeKeyPackage::new(vec![0xBB; 32], Some(vec![0xCC; 32]));
    assert_eq!(xts.expose_fvek(), [0xBB; 32]);
    assert_eq!(xts.expose_tweak(), Some([0xCC; 32].as_slice()));
}

#[test]
fn key_package_keeps_fvek_and_tweak_distinct() {
    // Swapping these silently produces wrong plaintext rather than an error, so
    // the accessors must not be interchangeable.
    let package = VolumeKeyPackage::new(vec![1, 2, 3, 4], Some(vec![9, 9, 9, 9]));
    assert_ne!(package.expose_fvek(), package.expose_tweak().unwrap());
}
