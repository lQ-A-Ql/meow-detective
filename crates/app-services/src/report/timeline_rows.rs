use super::ReportError;
use crate::timeline_service::{query_timeline_filtered_for_case, TimelineQuery};
use domain::CaseId;
use rusqlite::Connection;
use std::path::Path;
use transport::dto::TimelineEventDto;

const REPORT_TIMELINE_PAGE_SIZE: u32 = 500;

pub(crate) fn load_full_timeline_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<Vec<TimelineEventDto>, ReportError> {
    let mut events = Vec::new();
    let mut cursor = None::<String>;
    loop {
        let page = query_timeline_filtered_for_case(
            case_conn,
            case_root,
            case_id,
            TimelineQuery {
                offset: 0,
                limit: REPORT_TIMELINE_PAGE_SIZE,
                time_start: None,
                time_end: None,
                event_type: None,
                cursor: cursor.as_deref(),
            },
        )?;
        events.extend(page.items);
        let Some(next_cursor) = page.next_cursor else {
            break;
        };
        if cursor.as_deref() == Some(next_cursor.as_str()) {
            return Err(ReportError::Other(
                "timeline report cursor did not advance".to_string(),
            ));
        }
        cursor = Some(next_cursor);
    }
    Ok(events)
}
