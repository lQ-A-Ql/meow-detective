#[test]
fn classifies_unsupported_sst_features_separately_from_corruption() {
    let unsupported = super::map_sst_error(
        rocksdb_wire::RocksDbWireError::UnsupportedSstFormatVersion { version: 6 },
    );
    assert_eq!(unsupported.category, "unsupported");

    let corrupt = super::map_sst_error(rocksdb_wire::RocksDbWireError::SstChecksumMismatch {
        offset: 1,
        expected: 2,
        actual: 3,
    });
    assert_eq!(corrupt.category, "parser");

    let source_read = super::map_sst_error(rocksdb_wire::RocksDbWireError::SstSourceRead {
        offset: 1,
        length: 2,
    });
    assert_eq!(source_read.category, "io");

    let cancelled = super::map_sst_error(rocksdb_wire::RocksDbWireError::SstInspectionCancelled);
    assert_eq!(cancelled.code, "CONFLICT");
}

#[test]
fn classifies_all_sst_resource_limits_as_bounded_unsupported_capabilities() {
    let errors = [
        rocksdb_wire::RocksDbWireError::SstStoredBlockLimit { size: 2, limit: 1 },
        rocksdb_wire::RocksDbWireError::SstDecompressedBlockLimit { size: 2, limit: 1 },
        rocksdb_wire::RocksDbWireError::SstAuxiliaryMetadataLimit { total: 2, limit: 1 },
        rocksdb_wire::RocksDbWireError::SstEntryLimit { limit: 1 },
        rocksdb_wire::RocksDbWireError::SstCensusEntryLimit { limit: 1 },
        rocksdb_wire::RocksDbWireError::SstCensusDecompressedLimit { limit: 1 },
        rocksdb_wire::RocksDbWireError::SstKeyLengthLimit {
            length: 2,
            limit: 1,
        },
        rocksdb_wire::RocksDbWireError::SstValueLengthLimit {
            length: 2,
            limit: 1,
        },
    ];

    for error in errors {
        let mapped = super::map_sst_error(error);
        assert_eq!(mapped.category, "unsupported");
        assert!(mapped
            .message
            .contains("exceeds bounded inspection capability"));
    }
}
