mod crc32c;
mod cursor;
mod error;
mod limits;
mod log;
mod recovery;
mod replay;
mod sst;
mod version_edit;
mod write_batch;

pub use crc32c::{crc32c, extend_crc32c, mask_crc32c, unmask_crc32c};
pub use error::{Result, RocksDbWireError};
pub use limits::{
    LogDecodeLimits, ManifestDecodeLimits, ReplayLimits, VersionEditLimits, WriteBatchLimits,
};
pub use log::{decode_log, LogDecodeOptions, LogicalLogRecord, ROCKSDB_LOG_BLOCK_SIZE};
pub use recovery::{
    reduce_latest_state, reduce_latest_state_ref, KeyVersion, KeyVersionKind, LatestState,
    LatestStateError, LatestStateLimits, LatestStateRef, MergeOperator,
};
pub use replay::{replay_version_edits, ColumnFamilyState, LiveFile, ManifestSnapshot};
pub use sst::{
    inspect_sst, inspect_sst_with_visitor, visit_sst_entries, BlockCompression, BlockHandle,
    ChecksumType, DataBlockStats, EntryTypeCounts, Footer, IndexKeyKind, IndexKeyMetadata,
    KeySpaceBucket, KeySpaceCensus, KeySpaceCensusContext, KeySpacePrefixRule, RangeReader,
    SstDataEntry, SstEntryKind, SstEntryStreamSummary, SstEntryVisitor, SstInspection,
    SstInspectionStream, SstRangeDeletionEntry, SstReadOptions, SstVisitError, SstVisitOptions,
    TableProperties, BLOCK_BASED_TABLE_MAGIC, BLOCK_TRAILER_LENGTH, FOOTER_LENGTH,
    KEY_SPACE_SUMMARY_VERSION,
};
pub use version_edit::{
    parse_version_edit, ColumnFamilyAction, CompactCursor, DeletedFile, IgnoredField,
    InternalKeyMetadata, NewFile, NewFileFormat, NewFileMetadata, VersionEdit,
};
pub use write_batch::{
    decode_write_batch, WriteBatch, WriteBatchAuxiliaryKind, WriteBatchAuxiliaryRecord,
    WriteBatchMutation, WriteBatchMutationKind, ROCKSDB_MAX_SEQUENCE_NUMBER,
    WRITE_BATCH_HEADER_SIZE,
};

pub fn decode_manifest(input: &[u8], limits: ManifestDecodeLimits) -> Result<ManifestSnapshot> {
    let records = decode_log(
        input,
        LogDecodeOptions {
            expected_recyclable_log_number: None,
            limits: limits.log,
        },
    )?;
    let mut edits = Vec::with_capacity(records.len());
    for record in records {
        edits.push(parse_version_edit(&record.data, limits.version_edit)?);
    }
    replay_version_edits(&edits, limits.replay)
}
