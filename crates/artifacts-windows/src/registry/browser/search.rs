use super::{BrowserError, BrowserSearchPage, HiveSnapshot};
use std::sync::atomic::AtomicBool;
impl HiveSnapshot {
    pub fn search(&self, query: &str) -> Result<BrowserSearchPage, BrowserError> {
        self.search_with_cancel(query, &AtomicBool::new(false), &|_, _| {})
    }
    pub fn search_with_cancel(
        &self,
        query: &str,
        cancel: &AtomicBool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<BrowserSearchPage, BrowserError> {
        super::search_walk::SearchWalk::run(self, query, cancel, progress)
    }
}
