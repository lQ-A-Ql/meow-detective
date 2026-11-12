use super::validate_partition;

#[test]
fn partition_validation_accepts_ready_filesystems_and_rejects_locked_volumes() {
    assert!(validate_partition("supported", Some("NTFS")).is_ok());
    assert!(validate_partition("ready", Some("XFS")).is_ok());
    assert!(validate_partition("locked", Some("BitLocker")).is_err());
    assert!(validate_partition("supported", None).is_err());
}
