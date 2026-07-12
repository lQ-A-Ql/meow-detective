use super::*;

#[test]
fn test_sign_and_verify_roundtrip() {
    let (sk, pk) = SigningEngine::generate_keypair();
    let data = b"forensic case export data";
    let sig = SigningEngine::sign_data(&sk, data).expect("signing should succeed");
    assert!(SigningEngine::verify_signature(&pk, data, &sig));
}

#[test]
fn test_invalid_signature_detected() {
    let (sk, pk) = SigningEngine::generate_keypair();
    let data = b"original data";
    let sig = SigningEngine::sign_data(&sk, data).expect("signing should succeed");

    // Tampered data should not verify.
    assert!(!SigningEngine::verify_signature(
        &pk,
        b"tampered data",
        &sig
    ));

    // Tampered signature (flip last byte) should not verify.
    let mut bad_sig = sig.clone();
    if let Some(last) = bad_sig.last_mut() {
        *last ^= 0xFF;
    }
    assert!(!SigningEngine::verify_signature(&pk, data, &bad_sig));

    // Wrong public key should not verify.
    let (_, other_pk) = SigningEngine::generate_keypair();
    assert!(!SigningEngine::verify_signature(&other_pk, data, &sig));
}

#[test]
fn test_sign_case_export_produces_valid_structure() {
    let (sk, _pk) = SigningEngine::generate_keypair();
    let export_data = br#"{"case":"test","evidence":[]}"#;
    let signed =
        SigningEngine::sign_case_export("case-001", export_data, &sk).expect("should sign");

    assert_eq!(signed.signature.len(), 64);
    assert_eq!(signed.public_key.len(), 32);
    assert_eq!(signed.case_id, "case-001");
    assert_eq!(signed.algorithm, "Ed25519");
    assert!(!signed.timestamp.is_empty());

    // case_hash should be SHA-256 of export_data.
    let mut expected_hasher = Sha256::new();
    expected_hasher.update(export_data);
    assert_eq!(signed.case_hash, expected_hasher.finalize().to_vec());

    // Reconstruct the payload and verify the signature.
    let mut payload = Vec::new();
    payload.extend_from_slice(b"case-001");
    payload.extend_from_slice(signed.timestamp.as_bytes());
    payload.extend_from_slice(&signed.case_hash);
    assert!(SigningEngine::verify_signature(
        &signed.public_key,
        &payload,
        &signed.signature
    ));
}

#[test]
fn test_deterministic_signing() {
    let (sk, _pk) = SigningEngine::generate_keypair();
    let data = b"deterministic test";
    let sig1 = SigningEngine::sign_data(&sk, data).expect("first sign");
    let sig2 = SigningEngine::sign_data(&sk, data).expect("second sign");
    // Ed25519 is deterministic — same key + data = same signature.
    assert_eq!(sig1, sig2);
}
