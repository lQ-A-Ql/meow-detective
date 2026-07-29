use crate::RecoveredAesKey;

#[test]
fn recovered_aes_key_accepts_only_aes_key_lengths() {
    assert_eq!(
        RecoveredAesKey::new(vec![0x11; 16]).expect("AES-128").len(),
        16
    );
    assert_eq!(
        RecoveredAesKey::new(vec![0x22; 32]).expect("AES-256").len(),
        32
    );
    assert!(RecoveredAesKey::new(vec![0x33; 15]).is_err());
    assert!(RecoveredAesKey::new(vec![0x44; 64]).is_err());
}
