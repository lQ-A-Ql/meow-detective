use domain::DataSourceKind;

#[test]
fn ceph_rbd_storage_value_is_stable() {
    assert_eq!(DataSourceKind::CephRbd.to_string(), "ceph_rbd");
}
