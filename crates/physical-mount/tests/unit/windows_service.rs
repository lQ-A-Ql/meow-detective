use super::{ServiceCoordinatorState, SERVICE_DISABLED};

#[test]
fn nested_leases_restore_only_after_the_last_release() {
    let mut state = ServiceCoordinatorState::default();
    state.register_lease(Some(SERVICE_DISABLED)).unwrap();
    state.register_lease(None).unwrap();

    assert_eq!(state.release_lease().unwrap(), None);
    assert_eq!(state.release_lease().unwrap(), Some(SERVICE_DISABLED));
}

#[test]
fn pending_restore_survives_a_new_first_lease() {
    let mut state = ServiceCoordinatorState {
        active_leases: 0,
        restore_start_type: Some(SERVICE_DISABLED),
    };

    state.register_lease(None).unwrap();

    assert_eq!(state.release_lease().unwrap(), Some(SERVICE_DISABLED));
}

#[test]
fn duplicate_release_is_rejected() {
    let mut state = ServiceCoordinatorState::default();

    assert!(state.release_lease().is_err());
}
