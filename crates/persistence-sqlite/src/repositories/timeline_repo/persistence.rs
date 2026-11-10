use super::TimelineRepo;
use crate::DbResult;
use domain::TimelineEvent;

impl TimelineRepo<'_> {
    pub fn insert_batch(&self, events: &[TimelineEvent]) -> DbResult<()> {
        self.insert_batch_with_case(events, "")
    }

    pub fn insert_batch_with_case(&self, events: &[TimelineEvent], _case: &str) -> DbResult<()> {
        if let Some(source) = crate::source_fjall::source_id(self.conn)? {
            if crate::source_fjall::upsert_timeline(self.conn, events, &source)? {
                return Ok(());
            }
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn insert_batch_with_case_in_transaction(
        &self,
        events: &[TimelineEvent],
        _case: &str,
    ) -> DbResult<()> {
        if let Some(source) = crate::source_fjall::source_id(self.conn)? {
            if crate::source_fjall::upsert_timeline(self.conn, events, &source)? {
                return Ok(());
            }
        }
        Err(crate::DbError::System(
            "source detail storage is missing".into(),
        ))
    }
}
