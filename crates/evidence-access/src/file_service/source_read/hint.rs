//! hint support for source-bound reads.
use super::*;
impl SourceReadFileHint {
    pub fn new(
        file_id: FileEntryId,
        data_source_id: DataSourceId,
        partition_index: Option<usize>,
        path: String,
        size: u64,
        encrypted: bool,
    ) -> Self {
        Self {
            file_id,
            data_source_id,
            partition_index,
            path,
            size,
            encrypted,
        }
    }
}
