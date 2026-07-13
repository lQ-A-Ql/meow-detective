mod crc32c;
mod cursor;
mod error;
mod limits;
mod log;
mod replay;
mod version_edit;

pub use crc32c::{crc32c, extend_crc32c, mask_crc32c, unmask_crc32c};
pub use error::{Result, RocksDbWireError};
pub use limits::{LogDecodeLimits, ManifestDecodeLimits, ReplayLimits, VersionEditLimits};
pub use log::{
    decode_log, LogDecodeOptions, LogicalLogRecord, ROCKSDB_LOG_BLOCK_SIZE,
    ROCKSDB_LOG_HEADER_SIZE, ROCKSDB_RECYCLABLE_LOG_HEADER_SIZE,
};
pub use replay::{replay_version_edits, ColumnFamilyState, LiveFile, ManifestSnapshot};
pub use version_edit::{
    parse_version_edit, ColumnFamilyAction, CompactCursor, DeletedFile, IgnoredField,
    InternalKeyMetadata, NewFile, NewFileFormat, NewFileMetadata, VersionEdit,
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
