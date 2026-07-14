mod block;
mod block_handle;
mod census;
mod combined;
mod data;
mod entry;
mod footer;
mod index;
mod internal_key;
mod inventory;
mod layout;
mod metaindex;
mod model;
mod properties;
mod restart;
mod stream_totals;
mod visitor;

pub use block_handle::BlockHandle;
pub use census::{KeySpaceCensusContext, KeySpacePrefixRule};
pub use combined::inspect_sst_with_visitor;
pub use entry::{
    SstDataEntry, SstEntryKind, SstEntryStreamSummary, SstEntryVisitor, SstInspectionStream,
    SstRangeDeletionEntry, SstVisitError, SstVisitOptions,
};
pub use footer::{Footer, BLOCK_BASED_TABLE_MAGIC, FOOTER_LENGTH};
pub use inventory::inspect_sst;
pub use model::{
    BlockCompression, ChecksumType, DataBlockStats, EntryTypeCounts, IndexKeyKind,
    IndexKeyMetadata, KeySpaceBucket, KeySpaceCensus, SstInspection, SstReadOptions,
    TableProperties, BLOCK_TRAILER_LENGTH, KEY_SPACE_SUMMARY_VERSION,
};
pub use visitor::visit_sst_entries;

pub trait RangeReader {
    type Error: std::error::Error + Send + Sync + 'static;

    fn is_cancelled(&self) -> bool {
        false
    }

    fn read_range(
        &mut self,
        offset: u64,
        length: usize,
    ) -> std::result::Result<Vec<u8>, Self::Error>;
}
