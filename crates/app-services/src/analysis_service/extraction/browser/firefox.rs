use super::profile::unix_microseconds_to_dt;
use super::sqlite::table_exists;
use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{
    browser_attrs, make_artifact, make_timeline_event, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::error::AnalysisServiceError;
use rusqlite::Connection;
use serde_json::Value;

pub(super) fn extract_firefox_history(
    db: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let mut outcome = ExtractionOutcome::default();
    if !table_exists(db, "moz_places")? {
        outcome
            .warnings
            .push(format!("{} has no moz_places table", candidate.path));
        return Ok(outcome);
    }
    let mut stmt = db.prepare(
        "SELECT p.url, COALESCE(p.title, ''), COALESCE(p.visit_count, 0),
                    COALESCE(v.visit_date, p.last_visit_date)
             FROM moz_places p
             LEFT JOIN moz_historyvisits v ON v.place_id = p.id
             ORDER BY COALESCE(v.visit_date, p.last_visit_date) DESC
             LIMIT 500",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<i64>>(3)?,
        ))
    })?;
    for row in rows {
        let (url, title, visit_count, raw_time) = row?;
        if url.trim().is_empty() {
            continue;
        }
        let visited_at = raw_time.and_then(unix_microseconds_to_dt);
        let mut attrs = browser_attrs(candidate, browser, profile);
        attrs.insert("url".to_string(), Value::String(url.clone()));
        attrs.insert("title".to_string(), Value::String(title.clone()));
        attrs.insert(
            "visitCount".to_string(),
            Value::Number(serde_json::Number::from(visit_count.max(0) as u64)),
        );
        if let Some(dt) = visited_at {
            attrs.insert("visitTime".to_string(), Value::String(dt.to_rfc3339()));
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "BROWSER_VISIT",
                dt,
                format!("{} visit: {}", browser, title_or_url(&title, &url)),
                url.clone(),
                attrs.clone(),
                "browser.history",
            ));
        }
        outcome.artifacts.push(make_artifact(
            "BrowserHistory",
            format!("{} visit: {}", browser, title_or_url(&title, &url)),
            url,
            candidate,
            "browser.history",
            attrs,
        ));
    }
    Ok(outcome)
}
