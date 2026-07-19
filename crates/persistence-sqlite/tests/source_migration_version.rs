use persistence_sqlite::runner;

#[test]
fn source_version_order_accepts_equal_and_newer_versions() {
    assert!(runner::source_version_is_at_least(
        "source_015_ceph_bluestore_rbd_header_context",
        "source_015_ceph_bluestore_rbd_header_context"
    ));
    assert!(runner::source_version_is_at_least(
        "source_016_file_partition_index",
        "source_015_ceph_bluestore_rbd_header_context"
    ));
    assert!(runner::source_version_is_at_least(
        "source_017_timeline_projection_identity",
        "source_016_file_partition_index"
    ));
    assert!(runner::source_version_is_at_least(
        "source_018_cephfs_metadata_inventory",
        "source_017_timeline_projection_identity"
    ));
    assert!(runner::source_version_is_at_least(
        "source_019_cephfs_journal_replay",
        "source_018_cephfs_metadata_inventory"
    ));
}

#[test]
fn source_version_order_rejects_older_and_unknown_versions() {
    assert!(!runner::source_version_is_at_least(
        "source_014_ceph_osd_device_bindings",
        "source_015_ceph_bluestore_rbd_header_context"
    ));
    assert!(!runner::source_version_is_at_least(
        "source_999_unknown",
        "source_015_ceph_bluestore_rbd_header_context"
    ));
    assert!(!runner::source_version_is_at_least(
        "source_017_timeline_projection_identity",
        "source_999_unknown"
    ));
}
