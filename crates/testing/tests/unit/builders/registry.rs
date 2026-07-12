use super::*;

#[test]
fn system_hive_has_registry_header() {
    let hive = synthetic_system_hive();
    assert_eq!(&hive[0..4], b"regf");
    assert_eq!(&hive[0x1000..0x1004], b"hbin");
}

#[test]
fn software_hive_has_registry_header() {
    let hive = synthetic_software_hive();
    assert_eq!(&hive[0..4], b"regf");
    assert_eq!(&hive[0x1000..0x1004], b"hbin");
}
