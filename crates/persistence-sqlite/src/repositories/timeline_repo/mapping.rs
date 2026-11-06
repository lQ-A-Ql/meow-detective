use super::{TimelineCursorRow, TimelineSortKey};
use domain::{TimelineEvent, TimelineEventId};

pub(super) fn row_to_timeline_cursor_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TimelineCursorRow> {
    let timestamp = row.get::<_, Option<String>>(3)?;
    let event = row_to_timeline_event(row)?;
    let event_id = event.id.0.clone();
    Ok(TimelineCursorRow {
        event,
        sort_key: TimelineSortKey {
            timestamp,
            event_id,
        },
    })
}

pub(super) fn row_to_timeline_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<TimelineEvent> {
    let attrs: String = row.get(10)?;
    let attrs = serde_json::from_str(&attrs).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let timestamp = row
        .get::<_, Option<String>>(3)?
        .map(|value| crate::util::parse_datetime(&value))
        .unwrap_or_default();
    Ok(TimelineEvent {
        id: TimelineEventId(row.get(0)?),
        source_object_id: row.get(1)?,
        event_type: row.get(2)?,
        timestamp,
        title: row.get(4)?,
        description: row.get(5)?,
        parser_id: row.get(6)?,
        parser_version: row.get(7)?,
        confidence: row.get(8)?,
        source_attribution: row.get(9)?,
        attrs,
    })
}
