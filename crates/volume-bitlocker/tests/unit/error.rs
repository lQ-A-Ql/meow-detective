use super::*;

fn sample_errors() -> Vec<BitLockerError> {
    vec![
        BitLockerError::Locked,
        BitLockerError::CredentialRejected,
        BitLockerError::UnsupportedEncryptionMethod {
            code: 0x8001,
            label: "AES-256-CBC + Elephant Diffuser",
        },
        BitLockerError::UnsupportedProtector {
            found: "TPM, startup key".to_string(),
        },
        BitLockerError::MetadataUnreadable {
            reason: "all three metadata copies failed signature validation".to_string(),
        },
        BitLockerError::PersistedKeyInvalid {
            reason: "truncated envelope",
        },
        BitLockerError::PersistedKeyMismatch,
        BitLockerError::EvidenceRead {
            offset: 65_536,
            source: std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
        },
        BitLockerError::OutOfBounds {
            offset: 1_024,
            length: 512,
            volume_length: 1_024,
        },
    ]
}

#[test]
fn every_variant_has_a_distinct_stable_code() {
    let codes: Vec<&str> = sample_errors().iter().map(BitLockerError::code).collect();
    let unique: std::collections::BTreeSet<&str> = codes.iter().copied().collect();
    assert_eq!(
        codes.len(),
        unique.len(),
        "error codes must be distinct: {codes:?}"
    );
}

#[test]
fn codes_use_the_bitlocker_prefix() {
    for error in sample_errors() {
        assert!(
            error.code().starts_with("BITLOCKER_"),
            "unexpected code: {}",
            error.code()
        );
    }
}

#[test]
fn locked_volume_uses_the_contract_code() {
    // The design pins this exact string: after lock, every new evidence read
    // must surface BITLOCKER_LOCKED.
    assert_eq!(BitLockerError::Locked.code(), "BITLOCKER_LOCKED");
}

#[test]
fn only_credential_failures_are_retryable() {
    assert!(BitLockerError::Locked.is_retryable_with_credential());
    assert!(BitLockerError::CredentialRejected.is_retryable_with_credential());

    for error in sample_errors()
        .into_iter()
        .filter(|e| e.code() != "BITLOCKER_LOCKED" && e.code() != "BITLOCKER_CREDENTIAL_REJECTED")
    {
        assert!(
            !error.is_retryable_with_credential(),
            "{} must not offer a credential retry",
            error.code()
        );
    }
}

#[test]
fn rejected_credential_message_reveals_nothing_about_the_attempt() {
    // The message crosses into logs, events, and reports. It must not name the
    // credential, its length, or which protector was tried, since each narrows a
    // later guess.
    let rendered = BitLockerError::CredentialRejected.to_string();
    assert_eq!(rendered, "credential did not unlock the volume");
}

#[test]
fn unsupported_method_message_names_the_cipher_not_a_credential() {
    let rendered = BitLockerError::UnsupportedEncryptionMethod {
        code: 0x8001,
        label: "AES-256-CBC + Elephant Diffuser",
    }
    .to_string();
    assert!(rendered.contains("0x8001"));
    assert!(rendered.contains("AES-256-CBC + Elephant Diffuser"));
}

#[test]
fn evidence_read_error_preserves_the_io_source() {
    let error = BitLockerError::EvidenceRead {
        offset: 4_096,
        source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    };
    let source = std::error::Error::source(&error).expect("io source must be preserved");
    assert!(!source.to_string().is_empty());
    assert!(error.to_string().contains("4096"));
}
