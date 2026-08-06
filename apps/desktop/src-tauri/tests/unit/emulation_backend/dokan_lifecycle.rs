use dokan::MountFlags;

use super::{mount_flags, validate_mount_directory};

#[test]
fn extent_mount_is_writable_only_through_the_cow_handler() {
    let flags = mount_flags();

    assert!(!flags.contains(MountFlags::MOUNT_MANAGER));
    assert!(flags.contains(MountFlags::CURRENT_SESSION));
    assert!(!flags.contains(MountFlags::WRITE_PROTECT));
}

#[test]
fn extent_mount_rejects_nonempty_session_directory() {
    let session = tempfile::tempdir().unwrap();
    let mount = session.path().join("mount");
    std::fs::create_dir(&mount).unwrap();
    std::fs::write(mount.join("unexpected.bin"), b"derived").unwrap();

    let error = validate_mount_directory(session.path(), &mount).unwrap_err();
    assert!(error.to_string().contains("must be empty"));
}
