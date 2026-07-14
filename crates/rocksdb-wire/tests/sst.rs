mod sst_support;

use rocksdb_wire::{
    inspect_sst, BlockCompression, IndexKeyKind, KeySpaceCensusContext, KeySpacePrefixRule,
    RocksDbWireError, SstReadOptions, FOOTER_LENGTH, KEY_SPACE_SUMMARY_VERSION,
};
use sst_support::{
    build_sst, decode_plain_block, restart_block, rewrite_checksum, DataCompression,
    FixtureOptions, MemoryRangeReader,
};

fn fixture_census_context() -> KeySpaceCensusContext {
    KeySpaceCensusContext::prefix_buckets(
        "m-0",
        "fixture.unknown",
        vec![
            KeySpacePrefixRule::new("fixture.primary", b"m-key".to_vec())
                .expect("valid sanitized rule"),
        ],
    )
    .expect("valid sanitized context")
}

#[test]
fn inspects_v5_xxh3_lz4_sst_with_delta_index_and_sanitized_census() {
    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let inspected = inspect_sst(
        &mut reader,
        file_size,
        SstReadOptions::default(),
        &fixture_census_context(),
    )
    .expect("inspect SST");

    assert_eq!(inspected.footer.format_version, 5);
    assert_eq!(inspected.properties.column_family_name, "m-0");
    assert_eq!(inspected.properties.original_file_number, 146);
    assert_eq!(inspected.properties.properties_format_version, 0);
    assert!(inspected.properties.index_key_is_user_key);
    assert_eq!(inspected.first_index_key.kind, IndexKeyKind::User);
    assert_eq!(inspected.first_index_key.sequence, None);
    assert_eq!(inspected.first_index_key.value_type, None);
    assert_eq!(inspected.data_blocks.len(), 3);
    assert!(inspected
        .data_blocks
        .iter()
        .all(|block| block.compression == BlockCompression::Lz4));
    assert_eq!(inspected.counts.entries, 4);
    assert_eq!(inspected.counts.deletions, 2);
    assert_eq!(inspected.counts.merges, 1);
    assert_eq!(inspected.counts.range_deletions, 1);
    assert_eq!(inspected.smallest_sequence, 5);
    assert_eq!(inspected.largest_sequence, 30);
    assert_eq!(inspected.census.version, KEY_SPACE_SUMMARY_VERSION);
    assert!(inspected.census.complete);
    assert_eq!(inspected.census.scanned_entries, 3);
    assert_eq!(inspected.census.buckets.len(), 1);
    assert_eq!(inspected.census.buckets[0].name, "fixture.primary");
    assert_eq!(inspected.census.buckets[0].entries, 3);
    assert_eq!(inspected.unknown_meta_block_count, 1);
    let debug = format!("{inspected:?}");
    assert!(!debug.contains("m-key"));
    assert!(!debug.contains("value-a"));
    assert!(!debug.contains("merge-value"));
    assert!(!debug.contains("private.raw"));
    assert!(reader
        .reads
        .iter()
        .all(|(_, length)| *length < file_size as usize));
    assert!(reader
        .reads
        .contains(&(file_size - FOOTER_LENGTH as u64, FOOTER_LENGTH)));
}

#[test]
fn supports_none_lz4hc_and_lz4_dictionary_decompression() {
    for options in [
        FixtureOptions {
            compression: DataCompression::None,
            with_dictionary: false,
            ..FixtureOptions::default()
        },
        FixtureOptions {
            compression: DataCompression::Lz4Hc,
            with_dictionary: false,
            ..FixtureOptions::default()
        },
        FixtureOptions {
            compression: DataCompression::Lz4,
            with_dictionary: true,
            ..FixtureOptions::default()
        },
    ] {
        let fixture = build_sst(options);
        let file_size = fixture.bytes.len() as u64;
        let mut reader = MemoryRangeReader::new(fixture.bytes);
        let inspected = inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context(),
        )
        .expect("inspect SST");
        assert_eq!(inspected.counts.entries, 4);
        assert_eq!(
            inspected.compression_dictionary_present,
            options.with_dictionary
        );
    }

    let fixture = build_sst(FixtureOptions {
        index_keys_are_user: false,
        ..FixtureOptions::default()
    });
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let inspected = inspect_sst(
        &mut reader,
        file_size,
        SstReadOptions::default(),
        &fixture_census_context(),
    )
    .expect("inspect internal-key index SST");
    assert_eq!(inspected.first_index_key.kind, IndexKeyKind::Internal);
    assert_eq!(inspected.first_index_key.sequence, Some(30));
    assert_eq!(inspected.first_index_key.value_type, Some(1));
}

#[test]
fn census_budget_fails_closed_before_unbounded_scanning() {
    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let options = SstReadOptions {
        max_census_entries: 1,
        ..SstReadOptions::default()
    };
    assert!(matches!(
        inspect_sst(&mut reader, file_size, options, &fixture_census_context()),
        Err(RocksDbWireError::SstCensusEntryLimit { limit: 1 })
    ));
    assert!(reader.reads.iter().all(|(offset, _)| !fixture
        .data_handles
        .iter()
        .any(|handle| handle.offset == *offset)));

    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let options = SstReadOptions {
        max_census_decompressed_bytes: 1,
        ..SstReadOptions::default()
    };
    assert!(matches!(
        inspect_sst(&mut reader, file_size, options, &fixture_census_context()),
        Err(RocksDbWireError::SstCensusDecompressedLimit { limit: 1 })
    ));
    assert!(reader.reads.len() < fixture.data_handles.len() + 8);
}

#[test]
fn bounds_restart_arrays_and_resident_compression_dictionaries() {
    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let options = SstReadOptions {
        max_metaindex_entries: 2,
        ..SstReadOptions::default()
    };
    assert!(matches!(
        inspect_sst(&mut reader, file_size, options, &fixture_census_context()),
        Err(RocksDbWireError::SstEntryLimit { limit: 2 })
    ));

    let fixture = build_sst(FixtureOptions {
        with_dictionary: true,
        ..FixtureOptions::default()
    });
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let options = SstReadOptions {
        max_compression_dictionary_bytes: 1,
        ..SstReadOptions::default()
    };
    assert!(matches!(
        inspect_sst(&mut reader, file_size, options, &fixture_census_context()),
        Err(RocksDbWireError::SstDecompressedBlockLimit { limit: 1, .. })
    ));
}

#[test]
fn rejects_range_tombstone_only_tables_as_unsupported() {
    let fixture = build_sst(FixtureOptions {
        range_tombstone_only: true,
        ..FixtureOptions::default()
    });
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);

    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::UnsupportedSstFeature {
            feature: "table without data blocks",
            value: 1
        })
    ));
}

#[test]
fn auxiliary_metadata_budget_is_cumulative_and_checked_before_reads() {
    let fixture = build_sst(FixtureOptions {
        additional_unknown_meta_blocks: 1,
        ..FixtureOptions::default()
    });
    let first_serialized_size =
        fixture.unknown_meta_handles[0].size as usize + rocksdb_wire::BLOCK_TRAILER_LENGTH;
    let file_size = fixture.bytes.len() as u64;
    let auxiliary_offsets = fixture
        .unknown_meta_handles
        .iter()
        .map(|handle| handle.offset)
        .collect::<Vec<_>>();
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let options = SstReadOptions {
        max_auxiliary_metadata_bytes: first_serialized_size,
        ..SstReadOptions::default()
    };

    assert!(matches!(
        inspect_sst(&mut reader, file_size, options, &fixture_census_context()),
        Err(RocksDbWireError::SstAuxiliaryMetadataLimit { .. })
    ));
    assert!(reader
        .reads
        .iter()
        .all(|(offset, _)| !auxiliary_offsets.contains(offset)));
}

#[test]
fn rejects_footer_magic_version_checksum_padding_and_noncanonical_handle() {
    let cases = [
        (45usize, 0xffu8, "magic"),
        (41usize, 0x06u8, "version"),
        (0usize, 0x01u8, "checksum"),
    ];
    for (relative, replacement, expected) in cases {
        let mut fixture = build_sst(FixtureOptions::default());
        fixture.bytes[fixture.footer_offset + relative] = replacement;
        let file_size = fixture.bytes.len() as u64;
        let mut reader = MemoryRangeReader::new(fixture.bytes);
        let error = inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context(),
        )
        .expect_err("footer mutation must fail");
        assert!(error.to_string().contains(expected));
    }

    let mut padding = build_sst(FixtureOptions::default());
    padding.bytes[padding.footer_offset + 40] = 1;
    let file_size = padding.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(padding.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::NonZeroSstFooterPadding { .. })
    ));

    let mut canonical = build_sst(FixtureOptions::default());
    canonical.bytes[canonical.footer_offset + 1] = 0x80;
    canonical.bytes[canonical.footer_offset + 2] = 0x00;
    let file_size = canonical.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(canonical.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::NonCanonicalVarint { .. })
    ));
}

#[test]
fn rejects_corrupt_checksum_unknown_compression_and_lz4_bomb() {
    let mut corrupt = build_sst(FixtureOptions::default());
    corrupt.bytes[corrupt.data_handles[0].offset as usize] ^= 0x40;
    let file_size = corrupt.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(corrupt.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::SstChecksumMismatch { .. })
    ));

    let mut corrupt_meta = build_sst(FixtureOptions::default());
    corrupt_meta.bytes[corrupt_meta.unknown_meta_handle.offset as usize] ^= 0x20;
    let file_size = corrupt_meta.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(corrupt_meta.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::SstChecksumMismatch { .. })
    ));

    let mut opaque_meta = build_sst(FixtureOptions::default());
    let handle = opaque_meta.unknown_meta_handle;
    opaque_meta.bytes[(handle.offset + handle.size) as usize] = 0x7f;
    rewrite_checksum(&mut opaque_meta.bytes, handle);
    let file_size = opaque_meta.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(opaque_meta.bytes);
    inspect_sst(
        &mut reader,
        file_size,
        SstReadOptions::default(),
        &fixture_census_context(),
    )
    .expect("opaque auxiliary metadata only requires a valid checksum");

    let mut unknown = build_sst(FixtureOptions::default());
    let handle = unknown.data_handles[0];
    unknown.bytes[(handle.offset + handle.size) as usize] = 0x07;
    rewrite_checksum(&mut unknown.bytes, handle);
    let file_size = unknown.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(unknown.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::UnsupportedSstCompression { .. })
    ));

    let mut bomb = build_sst(FixtureOptions::default());
    let handle = bomb.data_handles[0];
    let offset = handle.offset as usize;
    bomb.bytes[offset] = 0xff;
    bomb.bytes[offset + 1] = 0xff;
    bomb.bytes[offset + 2] = 0xff;
    bomb.bytes[offset + 3] = 0xff;
    bomb.bytes[offset + 4] = 0x07;
    rewrite_checksum(&mut bomb.bytes, handle);
    let file_size = bomb.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(bomb.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::SstDecompressedBlockLimit { .. })
    ));
}

#[test]
fn rejects_restart_index_and_property_feature_corruption() {
    let mut restart = build_sst(FixtureOptions {
        compression: DataCompression::None,
        with_dictionary: false,
        ..FixtureOptions::default()
    });
    let handle = restart.data_handles[0];
    let footer = handle.offset as usize + handle.size as usize - 4;
    restart.bytes[footer..footer + 4].copy_from_slice(&0u32.to_le_bytes());
    rewrite_checksum(&mut restart.bytes, handle);
    let file_size = restart.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(restart.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::InvalidRestartBlock { .. })
    ));

    let mut index = build_sst(FixtureOptions {
        compression: DataCompression::None,
        with_dictionary: false,
        ..FixtureOptions::default()
    });
    let index_handle = index.index_handle;
    let index_offset = index_handle.offset as usize;
    let footer = index_offset + index_handle.size as usize - 4;
    let restarts = u32::from_le_bytes(
        index.bytes[footer..footer + 4]
            .try_into()
            .expect("restart footer"),
    );
    index.bytes[footer..footer + 4].copy_from_slice(&(restarts | (1 << 31)).to_le_bytes());
    rewrite_checksum(&mut index.bytes, index_handle);
    let file_size = index.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(index.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::UnsupportedSstFeature {
            feature: "index block hash index",
            ..
        })
    ));

    let unsupported = build_sst(FixtureOptions {
        properties_format_version: 1,
        ..FixtureOptions::default()
    });
    let file_size = unsupported.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(unsupported.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::UnsupportedSstFeature {
            feature: "table properties format version",
            value: 1
        })
    ));
}

#[test]
fn rejects_unknown_internal_type_and_short_range_read() {
    let mut fixture = build_sst(FixtureOptions {
        compression: DataCompression::None,
        with_dictionary: false,
        ..FixtureOptions::default()
    });
    let handle = fixture.data_handles[0];
    let mut block = decode_plain_block(&fixture.bytes, handle);
    let trailer_offset = 3 + b"m-key-a".len();
    block[trailer_offset] = 0x03;
    fixture.bytes[handle.offset as usize..handle.offset as usize + block.len()]
        .copy_from_slice(&block);
    rewrite_checksum(&mut fixture.bytes, handle);
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::UnsupportedSstEntryType { value_type: 3 })
    ));

    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    reader.short_read_once = true;
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::SstRangeRead { .. })
    ));

    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut bytes = fixture.bytes;
    bytes.pop();
    let mut reader = MemoryRangeReader::new(bytes);
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::SstSourceRead { .. })
    ));

    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    reader.cancelled = true;
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &fixture_census_context()
        ),
        Err(RocksDbWireError::SstInspectionCancelled)
    ));
}

#[test]
fn empty_restart_block_is_valid_for_generic_metadata_parser_shape() {
    let empty = restart_block(&[], 1);
    assert_eq!(empty, [0, 0, 0, 0, 1, 0, 0, 0]);
}

#[test]
fn census_context_is_caller_defined_validated_and_redacted() {
    let rule =
        KeySpacePrefixRule::new("fixture.redacted", b"secret-prefix".to_vec()).expect("valid rule");
    let context = KeySpaceCensusContext::prefix_buckets("m-0", "fixture.unknown", vec![rule])
        .expect("valid context");
    let debug = format!("{context:?}");
    assert!(debug.contains("prefix_length"));
    assert!(!debug.contains("secret-prefix"));
    assert!(!debug.contains("m-0"));

    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mismatched =
        KeySpaceCensusContext::unclassified("other-cf", "fixture.unknown").expect("valid context");
    assert!(matches!(
        inspect_sst(
            &mut reader,
            file_size,
            SstReadOptions::default(),
            &mismatched
        ),
        Err(RocksDbWireError::SstCensusColumnFamilyMismatch)
    ));

    assert!(KeySpacePrefixRule::new("raw name", vec![1]).is_err());
    assert!(KeySpacePrefixRule::new("fixture.valid", Vec::new()).is_err());
}
