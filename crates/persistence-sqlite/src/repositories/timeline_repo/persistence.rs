use super::TimelineRepo;
use crate::{DbError, DbResult};
use domain::TimelineEvent;
use rusqlite::params;

impl TimelineRepo<'_> {
    pub fn insert_batch(&self, events: &[TimelineEvent]) -> DbResult<()> {
        self.insert_batch_with_case(events, "")
    }

    pub fn insert_batch_with_case(&self, events: &[TimelineEvent], case: &str) -> DbResult<()> {
        if let Some(source) = crate::source_fjall::source_id(self.conn)? {
            if crate::source_fjall::upsert_timeline(self.conn, events, &source)? {
                return Ok(());
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        TimelineRepo::new(&tx).insert_batch_with_case_in_transaction(events, case)?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_batch_with_case_in_transaction(
        &self,
        events: &[TimelineEvent],
        case: &str,
    ) -> DbResult<()> {
        if let Some(source) = crate::source_fjall::source_id(self.conn)? {
            if crate::source_fjall::upsert_timeline(self.conn, events, &source)? {
                return Ok(());
            }
        }
        let mut statement = self.conn.prepare_cached(
            "INSERT INTO timeline_events(id,case_id,source_object_id,event_type,ts,title,description,parser_id,parser_version,confidence,source_attribution,attrs)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
        )?;
        for event in events {
            let attrs = serde_json::to_string(&event.attrs)
                .map_err(|error| DbError::System(error.to_string()))?;
            statement.execute(params![
                event.id.0,
                case,
                event.source_object_id,
                event.event_type,
                event.timestamp.to_rfc3339(),
                event.title,
                event.description,
                event.parser_id,
                event.parser_version,
                event.confidence,
                event.source_attribution,
                attrs
            ])?;
        }
        Ok(())
    }
}
