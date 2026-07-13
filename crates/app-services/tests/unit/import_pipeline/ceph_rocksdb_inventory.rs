#[test]
fn classifies_unknown_mandatory_manifest_features_as_unsupported() {
    for error in [
        rocksdb_wire::RocksDbWireError::UnknownMandatoryTag { tag: 400 },
        rocksdb_wire::RocksDbWireError::UnknownMandatoryCustomTag { tag: 66 },
        rocksdb_wire::RocksDbWireError::UnsupportedWalCompressionRecord { offset: 0 },
    ] {
        let mapped = super::map_manifest_error(error);
        assert_eq!(mapped.category, "unsupported");
    }
}

#[test]
fn classifies_corrupt_manifest_records_as_parser_errors() {
    let mapped = super::map_manifest_error(rocksdb_wire::RocksDbWireError::LogCrcMismatch {
        offset: 0,
        expected: 1,
        actual: 2,
    });

    assert_eq!(mapped.category, "parser");
}
