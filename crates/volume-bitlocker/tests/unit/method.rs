use super::*;

#[test]
fn classifies_every_known_method_code() {
    assert_eq!(
        EncryptionMethod::from_code(0x8000),
        EncryptionMethod::Aes128CbcDiffuser
    );
    assert_eq!(
        EncryptionMethod::from_code(0x8001),
        EncryptionMethod::Aes256CbcDiffuser
    );
    assert_eq!(
        EncryptionMethod::from_code(0x8002),
        EncryptionMethod::Aes128Cbc
    );
    assert_eq!(
        EncryptionMethod::from_code(0x8003),
        EncryptionMethod::Aes256Cbc
    );
    assert_eq!(
        EncryptionMethod::from_code(0x8004),
        EncryptionMethod::XtsAes128
    );
    assert_eq!(
        EncryptionMethod::from_code(0x8005),
        EncryptionMethod::XtsAes256
    );
}

#[test]
fn unknown_codes_round_trip_without_loss() {
    // An unrecognized cipher must still be reportable, so the code survives
    // classification rather than collapsing to a placeholder.
    for code in [0x0000u16, 0x7FFF, 0x8006, 0x8FFF, 0xFFFF] {
        let method = EncryptionMethod::from_code(code);
        assert_eq!(method, EncryptionMethod::Unknown(code));
        assert_eq!(method.code(), code);
    }
}

#[test]
fn code_round_trips_for_all_known_methods() {
    for code in [0x8000u16, 0x8001, 0x8002, 0x8003, 0x8004, 0x8005] {
        assert_eq!(EncryptionMethod::from_code(code).code(), code);
    }
}

#[test]
fn only_the_five_validated_methods_are_decryptable() {
    // 0x8001 is recognized but must stay non-decryptable: there is no oracle for
    // AES-256-CBC + diffuser, and an unvalidated cipher path would emit
    // plausible-looking wrong plaintext on an evidence reader.
    let decryptable: Vec<u16> = (0x8000u16..=0x8005)
        .filter(|c| EncryptionMethod::from_code(*c).is_decryptable())
        .collect();
    assert_eq!(decryptable, vec![0x8000, 0x8002, 0x8003, 0x8004, 0x8005]);
}

#[test]
fn unknown_methods_are_never_decryptable() {
    for code in [0x0000u16, 0x8006, 0xFFFF] {
        assert!(!EncryptionMethod::from_code(code).is_decryptable());
    }
}

#[test]
fn every_method_has_a_nonempty_label() {
    for code in [0x8000u16, 0x8001, 0x8002, 0x8003, 0x8004, 0x8005, 0xFFFF] {
        assert!(!EncryptionMethod::from_code(code).label().is_empty());
    }
}
