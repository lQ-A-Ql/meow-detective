use super::*;

#[test]
fn empty_registry_reports_locked_without_leaking_scope_data() {
    let registry = BitLockerUnlockRegistry::default();
    let result = registry.resolve_for_identities("case-a", "source-a", 3, &[]);

    assert!(matches!(result, Err(BitLockerRuntimeError::Locked)));
    assert!(registry.is_empty());
}

#[test]
fn invalidating_unknown_scope_is_a_noop() {
    let registry = BitLockerUnlockRegistry::default();

    assert_eq!(registry.invalidate_source("case-a", "source-a").unwrap(), 0);
    assert_eq!(
        registry
            .invalidate_partition("case-a", "source-a", 3)
            .unwrap(),
        0
    );
    assert_eq!(registry.invalidate_case("case-a").unwrap(), 0);
    assert_eq!(registry.len(), 0);
}
