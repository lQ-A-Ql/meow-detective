#[allow(dead_code)]
mod sst_support;

use std::convert::Infallible;

use rocksdb_wire::{
    inspect_sst, inspect_sst_with_visitor, visit_sst_entries, BlockHandle, SstDataEntry,
    SstEntryKind, SstEntryVisitor, SstRangeDeletionEntry, SstReadOptions, SstVisitOptions,
};
use sst_support::{build_sst, FixtureOptions, MemoryRangeReader};

#[derive(Default, Debug, PartialEq, Eq)]
struct RecordingVisitor {
    data: Vec<RecordedData>,
    ranges: Vec<RecordedRange>,
}

#[derive(Debug, PartialEq, Eq)]
struct RecordedData {
    block_handle: BlockHandle,
    block_ordinal: u64,
    entry_ordinal: u64,
    user_key: Vec<u8>,
    sequence: u64,
    kind: SstEntryKind,
    value: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
struct RecordedRange {
    block_handle: BlockHandle,
    entry_ordinal: u64,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
    sequence: u64,
}

impl SstEntryVisitor for RecordingVisitor {
    type Error = Infallible;

    fn visit_data(&mut self, entry: SstDataEntry<'_>) -> Result<(), Self::Error> {
        self.data.push(RecordedData {
            block_handle: entry.block_handle,
            block_ordinal: entry.block_ordinal,
            entry_ordinal: entry.entry_ordinal,
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
        self.ranges.push(RecordedRange {
            block_handle: entry.block_handle,
            entry_ordinal: entry.entry_ordinal,
            start_key: entry.start_user_key.to_vec(),
            end_key: entry.end_user_key.to_vec(),
            sequence: entry.sequence,
        });
        Ok(())
    }
}

#[test]
fn combined_scan_matches_existing_apis_and_reads_payload_blocks_once() {
    let fixture = build_sst(FixtureOptions::default());
    let file_size = fixture.bytes.len() as u64;
    let census_context = fixture_census_context();

    let mut inspection_reader = MemoryRangeReader::new(fixture.bytes.clone());
    let expected_inspection = inspect_sst(
        &mut inspection_reader,
        file_size,
        SstReadOptions::default(),
        &census_context,
    )
    .expect("inspect SST");

    let mut stream_reader = MemoryRangeReader::new(fixture.bytes.clone());
    let mut expected_visitor = RecordingVisitor::default();
    let expected_stream = visit_sst_entries(
        &mut stream_reader,
        file_size,
        SstVisitOptions::default(),
        &mut expected_visitor,
    )
    .expect("stream SST");

    let mut combined_reader = MemoryRangeReader::new(fixture.bytes);
    let mut combined_visitor = RecordingVisitor::default();
    let combined = inspect_sst_with_visitor(
        &mut combined_reader,
        file_size,
        SstVisitOptions::default(),
        &census_context,
        &mut combined_visitor,
    )
    .expect("combined SST scan");

    assert_eq!(combined.inspection, expected_inspection);
    assert_eq!(combined.stream, expected_stream);
    assert_eq!(combined_visitor, expected_visitor);
    for handle in fixture
        .data_handles
        .iter()
        .chain(std::iter::once(&fixture.range_handle))
    {
        let expected_read = (handle.offset, handle.size as usize + 5);
        assert_eq!(
            combined_reader
                .reads
                .iter()
                .filter(|read| **read == expected_read)
                .count(),
            1,
            "payload block at offset {} must be read once",
            handle.offset
        );
    }
}

fn fixture_census_context() -> rocksdb_wire::KeySpaceCensusContext {
    rocksdb_wire::KeySpaceCensusContext::prefix_buckets(
        "m-0",
        "fixture.unknown",
        vec![
            rocksdb_wire::KeySpacePrefixRule::new("fixture.primary", b"m-key".to_vec())
                .expect("valid prefix"),
        ],
    )
    .expect("valid census context")
}
