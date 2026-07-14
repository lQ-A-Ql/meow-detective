#[allow(dead_code)]
mod sst_support;

use std::convert::Infallible;
use std::fmt::{Display, Formatter};

use rocksdb_wire::{
    visit_sst_entries, BlockHandle, RocksDbWireError, SstDataEntry, SstEntryKind, SstEntryVisitor,
    SstRangeDeletionEntry, SstVisitError, SstVisitOptions,
};
use sst_support::{
    build_sst, internal_key, rewrite_checksum, DataCompression, FixtureOptions, MemoryRangeReader,
};

#[derive(Default)]
struct RecordingVisitor {
    data: Vec<RecordedDataEntry>,
    ranges: Vec<RecordedRangeEntry>,
}

#[derive(Debug, PartialEq, Eq)]
struct RecordedDataEntry {
    column_family_id: u32,
    block_handle: BlockHandle,
    block_ordinal: u64,
    entry_ordinal: u64,
    internal_key: Vec<u8>,
    user_key: Vec<u8>,
    sequence: u64,
    kind: SstEntryKind,
    value: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
struct RecordedRangeEntry {
    column_family_id: u32,
    block_handle: BlockHandle,
    entry_ordinal: u64,
    internal_key: Vec<u8>,
    start_user_key: Vec<u8>,
    end_user_key: Vec<u8>,
    sequence: u64,
}

impl SstEntryVisitor for RecordingVisitor {
    type Error = Infallible;

    fn visit_data(&mut self, entry: SstDataEntry<'_>) -> Result<(), Self::Error> {
        self.data.push(RecordedDataEntry {
            column_family_id: entry.column_family_id,
            block_handle: entry.block_handle,
            block_ordinal: entry.block_ordinal,
            entry_ordinal: entry.entry_ordinal,
            internal_key: entry.internal_key.to_vec(),
            user_key: entry.user_key.to_vec(),
            sequence: entry.sequence,
            kind: entry.kind,
            value: entry.value.to_vec(),
        });
        Ok(())
    }

    fn visit_range_deletion(
        &mut self,
        entry: SstRangeDeletionEntry<'_>,
    ) -> Result<(), Self::Error> {
        self.ranges.push(RecordedRangeEntry {
            column_family_id: entry.column_family_id,
            block_handle: entry.block_handle,
            entry_ordinal: entry.entry_ordinal,
            internal_key: entry.internal_key.to_vec(),
            start_user_key: entry.start_user_key.to_vec(),
            end_user_key: entry.end_user_key.to_vec(),
            sequence: entry.sequence,
        });
        Ok(())
    }
}

#[test]
fn streams_data_and_range_entries_without_reading_the_whole_file() {
    let fixture = build_sst(FixtureOptions::default());
    let data_handles = fixture.data_handles.clone();
    let range_handle = fixture.range_handle;
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    let summary = visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect("stream SST entries");

    assert_eq!(summary.properties.column_family_id, 1);
    assert_eq!(summary.properties.column_family_name, "m-0");
    assert_eq!(summary.data_block_count, 3);
    assert!(summary.scanned_decompressed_bytes > 0);
    assert_eq!(summary.counts.entries, 4);
    assert_eq!(summary.counts.deletions, 2);
    assert_eq!(summary.counts.merges, 1);
    assert_eq!(summary.counts.range_deletions, 1);
    assert_eq!(summary.smallest_sequence, 5);
    assert_eq!(summary.largest_sequence, 30);
    assert_eq!(
        visitor.data,
        vec![
            RecordedDataEntry {
                column_family_id: 1,
                block_handle: data_handles[0],
                block_ordinal: 0,
                entry_ordinal: 0,
                internal_key: internal_key(b"m-key-a", 30, 1),
                user_key: b"m-key-a".to_vec(),
                sequence: 30,
                kind: SstEntryKind::Value,
                value: b"value-a".to_vec(),
            },
            RecordedDataEntry {
                column_family_id: 1,
                block_handle: data_handles[1],
                block_ordinal: 1,
                entry_ordinal: 0,
                internal_key: internal_key(b"m-key-b", 20, 0),
                user_key: b"m-key-b".to_vec(),
                sequence: 20,
                kind: SstEntryKind::Deletion,
                value: Vec::new(),
            },
            RecordedDataEntry {
                column_family_id: 1,
                block_handle: data_handles[2],
                block_ordinal: 2,
                entry_ordinal: 0,
                internal_key: internal_key(b"m-key-c", 10, 2),
                user_key: b"m-key-c".to_vec(),
                sequence: 10,
                kind: SstEntryKind::Merge,
                value: b"merge".to_vec(),
            },
        ]
    );
    assert_eq!(
        visitor.ranges,
        vec![RecordedRangeEntry {
            column_family_id: 1,
            block_handle: range_handle,
            entry_ordinal: 0,
            internal_key: internal_key(b"m-key-a", 5, 0x0f),
            start_user_key: b"m-key-a".to_vec(),
            end_user_key: b"m-key-z".to_vec(),
            sequence: 5,
        }]
    );
    assert!(reader
        .reads
        .iter()
        .all(|(_, length)| *length < file_size as usize));
}

#[derive(Debug, PartialEq, Eq)]
struct VisitorStop;

impl Display for VisitorStop {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("stop")
    }
}

impl std::error::Error for VisitorStop {}

struct FailingVisitor;

impl SstEntryVisitor for FailingVisitor {
    type Error = VisitorStop;

    fn visit_data(&mut self, _entry: SstDataEntry<'_>) -> Result<(), Self::Error> {
        Err(VisitorStop)
    }

    fn visit_range_deletion(
        &mut self,
        _entry: SstRangeDeletionEntry<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn preserves_typed_visitor_failures_without_scanning_later_blocks() {
    let fixture = build_sst(FixtureOptions::default());
    let first_data_offset = fixture.data_handles[0].offset;
    let later_data_offsets = fixture.data_handles[1..]
        .iter()
        .map(|handle| handle.offset)
        .collect::<Vec<_>>();
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);

    let error = visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut FailingVisitor,
    )
    .expect_err("visitor must stop the stream");

    assert!(matches!(error, SstVisitError::Visitor(VisitorStop)));
    assert!(reader
        .reads
        .iter()
        .any(|(offset, _)| *offset == first_data_offset));
    assert!(reader
        .reads
        .iter()
        .all(|(offset, _)| !later_data_offsets.contains(offset)));
}

#[test]
fn rejects_internal_keys_that_regress_across_data_blocks() {
    let fixture_options = FixtureOptions {
        compression: DataCompression::None,
        ..FixtureOptions::default()
    };
    let mut fixture = build_sst(fixture_options);
    replace_same_length(
        &mut fixture.bytes,
        fixture.data_handles[1],
        b"m-key-b",
        b"m-key-0",
    );
    rewrite_checksum(&mut fixture.bytes, fixture.data_handles[1]);
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    let error = visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect_err("regressing internal keys must fail");

    assert!(matches!(
        error,
        SstVisitError::Wire(RocksDbWireError::InvalidSstProperty {
            context: "SST entry stream",
            reason: "internal keys are not strictly ordered",
        })
    ));
}

#[test]
fn accepts_same_user_key_only_when_internal_trailers_descend() {
    let fixture_options = FixtureOptions {
        compression: DataCompression::None,
        ..FixtureOptions::default()
    };
    let mut fixture = build_sst(fixture_options);
    for (handle, from) in [
        (fixture.data_handles[1], b"m-key-b".as_slice()),
        (fixture.data_handles[2], b"m-key-c".as_slice()),
    ] {
        replace_same_length(&mut fixture.bytes, handle, from, b"m-key-a");
        rewrite_checksum(&mut fixture.bytes, handle);
    }
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect("same user key with descending sequence/type trailers is ordered");

    assert_eq!(
        visitor
            .data
            .iter()
            .map(|entry| entry.sequence)
            .collect::<Vec<_>>(),
        vec![30, 20, 10]
    );
}

#[test]
fn rejects_duplicate_internal_keys_across_data_blocks() {
    let fixture_options = FixtureOptions {
        compression: DataCompression::None,
        ..FixtureOptions::default()
    };
    let mut fixture = build_sst(fixture_options);
    replace_same_length(
        &mut fixture.bytes,
        fixture.data_handles[1],
        &internal_key(b"m-key-b", 20, 0),
        &internal_key(b"m-key-a", 30, 1),
    );
    rewrite_checksum(&mut fixture.bytes, fixture.data_handles[1]);
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    let error = visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect_err("duplicate internal keys must fail");

    assert!(matches!(
        error,
        SstVisitError::Wire(RocksDbWireError::InvalidSstProperty {
            context: "SST entry stream",
            reason: "internal keys are not strictly ordered",
        })
    ));
}

#[test]
fn rejects_data_block_hash_index_before_decoding_restart_offsets() {
    let fixture_options = FixtureOptions {
        compression: DataCompression::None,
        ..FixtureOptions::default()
    };
    let mut fixture = build_sst(fixture_options);
    let handle = fixture.data_handles[0];
    let footer_offset = handle.offset as usize + handle.size as usize - 4;
    let footer = u32::from_le_bytes(
        fixture.bytes[footer_offset..footer_offset + 4]
            .try_into()
            .expect("fixture restart footer"),
    );
    fixture.bytes[footer_offset..footer_offset + 4]
        .copy_from_slice(&(footer | (1 << 31)).to_le_bytes());
    rewrite_checksum(&mut fixture.bytes, handle);
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    let error = visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect_err("data block hash index is unsupported");

    assert!(matches!(
        error,
        SstVisitError::Wire(RocksDbWireError::UnsupportedSstFeature {
            feature: "data block hash index",
            value: 1,
        })
    ));
    assert!(visitor.data.is_empty());
}

#[test]
fn enforces_a_cumulative_decompressed_byte_budget() {
    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();
    let options = SstVisitOptions {
        max_total_decompressed_bytes: 1,
        ..SstVisitOptions::default()
    };

    let error = visit_sst_entries(&mut reader, file_size, options, &mut visitor)
        .expect_err("stream must enforce its cumulative decompressed byte budget");

    assert!(matches!(
        error,
        SstVisitError::Wire(RocksDbWireError::SstStreamDecompressedLimit { limit: 1 })
    ));
    assert!(visitor.data.is_empty());
    assert!(visitor.ranges.is_empty());
}

#[test]
fn rejects_unknown_data_entry_types() {
    let fixture_options = FixtureOptions {
        compression: DataCompression::None,
        ..FixtureOptions::default()
    };
    let mut fixture = build_sst(fixture_options);
    replace_same_length(
        &mut fixture.bytes,
        fixture.data_handles[0],
        &internal_key(b"m-key-a", 30, 1),
        &internal_key(b"m-key-a", 30, 3),
    );
    rewrite_checksum(&mut fixture.bytes, fixture.data_handles[0]);
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    let error = visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect_err("unknown data entry type must fail");

    assert!(matches!(
        error,
        SstVisitError::Wire(RocksDbWireError::UnsupportedSstEntryType { value_type: 3 })
    ));
    assert!(visitor.data.is_empty());
}

#[test]
fn rejects_reversed_range_tombstones_before_the_range_callback() {
    let mut fixture = build_sst(FixtureOptions::default());
    replace_same_length(
        &mut fixture.bytes,
        fixture.range_handle,
        b"m-key-z",
        b"m-key-0",
    );
    rewrite_checksum(&mut fixture.bytes, fixture.range_handle);
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    let error = visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect_err("reversed range tombstone must fail");

    assert!(matches!(
        error,
        SstVisitError::Wire(RocksDbWireError::InvalidSstProperty {
            context: "range deletion entry",
            reason: "start key is after end key",
        })
    ));
    assert!(visitor.ranges.is_empty());
}

#[test]
fn preserves_empty_range_tombstones_as_legal_no_effect_records() {
    let mut fixture = build_sst(FixtureOptions::default());
    replace_same_length(
        &mut fixture.bytes,
        fixture.range_handle,
        b"m-key-z",
        b"m-key-a",
    );
    rewrite_checksum(&mut fixture.bytes, fixture.range_handle);
    let range_handle = fixture.range_handle;
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect("empty range tombstone is a valid raw record");

    assert_eq!(
        visitor.ranges,
        vec![RecordedRangeEntry {
            column_family_id: 1,
            block_handle: range_handle,
            entry_ordinal: 0,
            internal_key: internal_key(b"m-key-a", 5, 0x0f),
            start_user_key: b"m-key-a".to_vec(),
            end_user_key: b"m-key-a".to_vec(),
            sequence: 5,
        }]
    );
}

#[test]
fn rejects_external_sst_global_sequence_properties() {
    let fixture = build_sst(FixtureOptions {
        external_sst_properties: true,
        ..FixtureOptions::default()
    });
    let file_size = fixture.bytes.len() as u64;
    let mut reader = MemoryRangeReader::new(fixture.bytes);
    let mut visitor = RecordingVisitor::default();

    let error = visit_sst_entries(
        &mut reader,
        file_size,
        SstVisitOptions::default(),
        &mut visitor,
    )
    .expect_err("external SST global sequence semantics must fail closed");

    assert!(matches!(
        error,
        SstVisitError::Wire(RocksDbWireError::UnsupportedSstFeature {
            feature: "external SST global sequence",
            value: 1,
        })
    ));
    assert!(visitor.data.is_empty());
    assert!(visitor.ranges.is_empty());
}

#[test]
fn enforces_stream_specific_block_entry_and_range_limits_before_callbacks() {
    for options in [
        SstVisitOptions {
            max_data_blocks: 2,
            ..SstVisitOptions::default()
        },
        SstVisitOptions {
            max_total_entries: 3,
            ..SstVisitOptions::default()
        },
        SstVisitOptions {
            max_range_deletions: 0,
            ..SstVisitOptions::default()
        },
    ] {
        let fixture = build_sst(FixtureOptions::default());
        let file_size = fixture.bytes.len() as u64;
        let mut reader = MemoryRangeReader::new(fixture.bytes);
        let mut visitor = RecordingVisitor::default();

        assert!(visit_sst_entries(&mut reader, file_size, options, &mut visitor).is_err());
        assert!(visitor.data.is_empty());
        assert!(visitor.ranges.is_empty());
    }
}

fn replace_same_length(bytes: &mut [u8], handle: BlockHandle, from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let start = handle.offset as usize;
    let end = start + handle.size as usize;
    let block = &mut bytes[start..end];
    let offset = block
        .windows(from.len())
        .position(|window| window == from)
        .expect("fixture block contains target bytes");
    block[offset..offset + to.len()].copy_from_slice(to);
}
