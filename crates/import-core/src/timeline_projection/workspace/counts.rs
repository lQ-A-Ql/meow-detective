use super::{add, overflow, TimelineProjectionError};
use chrono::{Datelike, NaiveDate};
use persistence_source::fjall::TimelineRecord;
use timeline::TimelineEventKind;

#[derive(Debug, Clone, Default)]
pub(super) struct CountWorkspace {
    ranges: [Option<PeriodRange>; 4],
    other_keys: u64,
    max_key_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
struct PeriodRange {
    first: NaiveDate,
    last: NaiveDate,
    events: u64,
}

impl CountWorkspace {
    pub fn observe(&mut self, record: &TimelineRecord) -> Result<(), TimelineProjectionError> {
        self.max_key_bytes = self
            .max_key_bytes
            .max(record.data_source_id.len() as u64 + record.event_type.len() as u64 + 160);
        let slot = match TimelineEventKind::parse(&record.event_type) {
            Some(TimelineEventKind::FileCreated) => 0,
            Some(TimelineEventKind::FileModified) => 1,
            Some(TimelineEventKind::FileAccessed) => 2,
            Some(TimelineEventKind::FileDeleted) => 3,
            _ => {
                add(&mut self.other_keys, 8)?;
                return Ok(());
            }
        };
        let date = record.timestamp.date_naive();
        if let Some(range) = &mut self.ranges[slot] {
            range.first = range.first.min(date);
            range.last = range.last.max(date);
            add(&mut range.events, 1)?;
        } else {
            self.ranges[slot] = Some(PeriodRange {
                first: date,
                last: date,
                events: 1,
            });
        }
        Ok(())
    }

    pub fn required(&self) -> Result<(u64, u64), TimelineProjectionError> {
        let mut keys = self.other_keys;
        for range in self.ranges.iter().flatten() {
            let years = (range.last.year() - range.first.year()) as u64 + 1;
            let months = ((range.last.year() - range.first.year()) as i64 * 12
                + i64::from(range.last.month())
                - i64::from(range.first.month())) as u64
                + 1;
            let days = (range.last - range.first).num_days() as u64 + 1;
            let periods = years
                .checked_add(months)
                .and_then(|n| n.checked_add(days))
                .and_then(|n| n.checked_add(1))
                .ok_or_else(overflow)?;
            let distinct_bound = periods
                .min(range.events.saturating_mul(4))
                .saturating_mul(2);
            add(&mut keys, distinct_bound)?;
        }
        // Account for key/value storage, the delta map and committed action metadata.
        let bytes = keys
            .checked_mul(self.max_key_bytes + 128)
            .and_then(|value| value.checked_mul(2))
            .ok_or_else(overflow)?;
        Ok((keys, bytes))
    }
}
