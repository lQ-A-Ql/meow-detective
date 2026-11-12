//! Validated bounds for one Timeline projection, independent of IPC or parsers.

/// SQLite negative cache_size uses KiB, independent of database page_size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineProjectionPolicy {
    pub graph_batch_rows: u32,
    pub cached_graph_statements: bool,
    pub wal_autocheckpoint_pages: Option<u32>,
    pub cache_kib: u32,
}

impl Default for TimelineProjectionPolicy {
    fn default() -> Self {
        Self {
            graph_batch_rows: 20_000,
            cached_graph_statements: false,
            wal_autocheckpoint_pages: None,
            cache_kib: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("timeline projection policy exceeds its bounded batch or WAL range")]
pub struct TimelineProjectionPolicyError;

impl TimelineProjectionPolicy {
    pub fn validate(self) -> Result<(), TimelineProjectionPolicyError> {
        if self.graph_batch_rows == 0
            || !(128 * 1024..=1024 * 1024).contains(&self.cache_kib)
            || self.graph_batch_rows > 100_000
            || self
                .wal_autocheckpoint_pages
                .is_some_and(|pages| pages == 0 || pages > 65_536)
        {
            return Err(TimelineProjectionPolicyError);
        }
        Ok(())
    }
}
