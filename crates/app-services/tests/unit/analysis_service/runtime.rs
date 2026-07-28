use super::*;

#[test]
fn default_runtime_does_not_invent_bitlocker_capabilities() {
    let runtime = AnalysisSourceReadRuntime::default();
    assert!(runtime.bitlocker_runtime.is_none());
}

#[test]
fn configured_runtime_retains_the_shared_unlock_registry() {
    let registry = Arc::new(BitLockerUnlockRegistry::default());
    let runtime = AnalysisSourceReadRuntime::with_bitlocker_runtime(Arc::clone(&registry));

    assert!(runtime
        .bitlocker_runtime
        .as_ref()
        .is_some_and(|configured| Arc::ptr_eq(configured, &registry)));
}
