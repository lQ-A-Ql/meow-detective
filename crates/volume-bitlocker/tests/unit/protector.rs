use super::*;

#[test]
fn only_password_and_recovery_password_unlock() {
    assert!(ProtectorKind::Password.is_unlockable());
    assert!(ProtectorKind::RecoveryPassword.is_unlockable());

    // Clear key is decryptable in principle but deliberately not an unlock path
    // in v1 — see the ClearKey doc comment.
    assert!(!ProtectorKind::ClearKey.is_unlockable());
    assert!(!ProtectorKind::Tpm.is_unlockable());
    assert!(!ProtectorKind::StartupKey.is_unlockable());
    assert!(!ProtectorKind::Unknown(0x1234).is_unlockable());
}

#[test]
fn only_clear_key_needs_no_credential() {
    assert!(!ProtectorKind::ClearKey.requires_credential());
    for kind in [
        ProtectorKind::Password,
        ProtectorKind::RecoveryPassword,
        ProtectorKind::Tpm,
        ProtectorKind::StartupKey,
        ProtectorKind::Unknown(0),
    ] {
        assert!(
            kind.requires_credential(),
            "{kind:?} must need a credential"
        );
    }
}

#[test]
fn inventory_preserves_on_disk_order() {
    // Order is evidence: it reflects the metadata entry sequence, which an
    // examiner may need to correlate with a raw hex view.
    let kinds = vec![
        ProtectorKind::Tpm,
        ProtectorKind::RecoveryPassword,
        ProtectorKind::Password,
    ];
    let inventory = ProtectorInventory::new(kinds.clone());
    assert_eq!(inventory.protectors(), kinds.as_slice());
}

#[test]
fn inventory_reports_unlockability() {
    let locked_only = ProtectorInventory::new(vec![ProtectorKind::Tpm, ProtectorKind::StartupKey]);
    assert!(!locked_only.has_unlockable_protector());

    let with_recovery =
        ProtectorInventory::new(vec![ProtectorKind::Tpm, ProtectorKind::RecoveryPassword]);
    assert!(with_recovery.has_unlockable_protector());
}

#[test]
fn clear_key_alone_is_not_an_unlockable_inventory() {
    let clear = ProtectorInventory::new(vec![ProtectorKind::ClearKey]);
    assert!(
        !clear.has_unlockable_protector(),
        "a clear-key volume must not report itself as unlockable in v1"
    );
    assert!(!clear.is_empty());
}

#[test]
fn empty_inventory_is_distinguishable() {
    // An empty inventory means metadata parsed but carried no protector entries:
    // a malformed-volume signal, not an unprotected volume.
    let empty = ProtectorInventory::default();
    assert!(empty.is_empty());
    assert!(!empty.has_unlockable_protector());
    assert!(empty.protectors().is_empty());
}

#[test]
fn every_protector_has_a_nonempty_label() {
    for kind in [
        ProtectorKind::ClearKey,
        ProtectorKind::RecoveryPassword,
        ProtectorKind::Password,
        ProtectorKind::Tpm,
        ProtectorKind::StartupKey,
        ProtectorKind::Unknown(0xFFFF),
    ] {
        assert!(!kind.label().is_empty());
    }
}
