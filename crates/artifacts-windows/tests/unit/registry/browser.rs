use crate::browse_registry_hive;

#[test]
fn browser_rejects_non_hive_bytes() {
    let error = browse_registry_hive(b"not-a-hive", "").expect_err("invalid hive");
    assert!(error.contains("registry hive"));
}
