use super::parse_source_sha256;

#[test]
fn emulation_parent_binding_requires_an_exact_sha256() {
    assert_eq!(parse_source_sha256(&"ab".repeat(32)).unwrap(), [0xab; 32]);
    assert!(parse_source_sha256("data-source-id:source-1").is_err());
    assert!(parse_source_sha256(&"z".repeat(64)).is_err());
    assert!(parse_source_sha256(&"a".repeat(62)).is_err());
}
