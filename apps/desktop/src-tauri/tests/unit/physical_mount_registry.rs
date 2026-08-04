use super::PhysicalMountRegistryError;
use transport::ServiceErrorCategory;

#[test]
fn duplicate_physical_mounts_are_validation_errors() {
    assert!(matches!(
        PhysicalMountRegistryError::AlreadyMounted.category(),
        transport::ErrorCategory::Validation
    ));
}

#[test]
fn unsupported_physical_backend_is_typed_as_unsupported() {
    let error = PhysicalMountRegistryError::Backend(
        physical_mount::PhysicalMountError::UnsupportedPlatform,
    );
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Unsupported
    ));
}

#[test]
fn physical_mount_elevation_failures_are_typed_as_security_errors() {
    for backend_error in [
        physical_mount::PhysicalMountError::IscsiServiceRequiresElevation,
        physical_mount::PhysicalMountError::IscsiLoginRequiresElevation,
    ] {
        let error = PhysicalMountRegistryError::Backend(backend_error);
        assert!(matches!(
            error.category(),
            transport::ErrorCategory::Security
        ));
    }
}
