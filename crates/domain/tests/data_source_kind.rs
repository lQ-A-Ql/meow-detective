use domain::DataSourceKind;

#[test]
fn ceph_rbd_storage_value_is_stable() {
    assert_eq!(DataSourceKind::CephRbd.to_string(), "ceph_rbd");
}

#[test]
fn ceph_fs_storage_value_is_stable() {
    assert_eq!(DataSourceKind::CephFs.to_string(), "ceph_fs");
}

#[test]
fn local_disk_storage_value_is_stable() {
    assert_eq!(DataSourceKind::LocalDisk.to_string(), "local_disk");
}
