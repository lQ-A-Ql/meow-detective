use super::TimelineRepo;
use domain::TimelineEvent;
use persistence_sqlite::DbResult;

impl TimelineRepo<'_> {
    pub fn insert_batch(&self, events: &[TimelineEvent]) -> DbResult<()> {
        self.insert_batch_with_case(events, "")
    }

    pub fn insert_batch_with_case(&self, events: &[TimelineEvent], _case: &str) -> DbResult<()> {
        if let Some(source) = crate::detail::source_id(self.conn)? {
            if crate::detail::upsert_timeline(self.conn, events, &source)? {
                return Ok(());
            }
        }
        Err(persistence_sqlite::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn insert_batch_with_case_in_transaction(
        &self,
        events: &[TimelineEvent],
        _case: &str,
    ) -> DbResult<()> {
        if let Some(source) = crate::detail::source_id(self.conn)? {
            if crate::detail::upsert_timeline(self.conn, events, &source)? {
                return Ok(());
            }
        }
        Err(persistence_sqlite::DbError::System(
            "source detail storage is missing".into(),
        ))
    }
}
