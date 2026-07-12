use super::profile::chromium_time_to_dt;
use super::sqlite::{table_columns, table_exists};
use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{
    browser_attrs, make_artifact, make_timeline_event, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::error::AnalysisServiceError;
use domain::{Artifact, TimelineEvent};
use rusqlite::Connection;
use serde_json::Value;

pub(super) fn extract_chromium_history(
    db: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let mut outcome = ExtractionOutcome::default();
    if table_exists(db, "urls")? {
        let mut stmt = db.prepare(
            "SELECT urls.url, COALESCE(urls.title, ''), COALESCE(urls.visit_count, 0),
                        COALESCE(visits.visit_time, urls.last_visit_time)
                 FROM urls
                 LEFT JOIN visits ON visits.url = urls.id
                 ORDER BY COALESCE(visits.visit_time, urls.last_visit_time) DESC
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
            let visited_at = raw_time.and_then(chromium_time_to_dt);
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
    }

    if table_exists(db, "downloads")? {
        outcome.artifacts.extend(extract_chromium_downloads(
            db,
            candidate,
            browser,
            profile,
            &mut outcome.timeline_events,
        )?);
    }
    Ok(outcome)
}

fn extract_chromium_downloads(
    db: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
    events: &mut Vec<TimelineEvent>,
) -> Result<Vec<Artifact>, AnalysisServiceError> {
    let columns = table_columns(db, "downloads")?;
    let url_expr = if columns.iter().any(|column| column == "tab_url") {
        "COALESCE(tab_url, '')"
    } else if columns.iter().any(|column| column == "url") {
        "COALESCE(url, '')"
    } else {
        "''"
    };
    let target_expr = if columns.iter().any(|column| column == "target_path") {
        "COALESCE(target_path, '')"
    } else if columns.iter().any(|column| column == "current_path") {
        "COALESCE(current_path, '')"
    } else {
        "''"
    };
    let start_expr = if columns.iter().any(|column| column == "start_time") {
        "COALESCE(start_time, 0)"
    } else {
        "0"
    };
    let bytes_expr = if columns.iter().any(|column| column == "total_bytes") {
        "COALESCE(total_bytes, 0)"
    } else if columns.iter().any(|column| column == "received_bytes") {
        "COALESCE(received_bytes, 0)"
    } else {
        "0"
    };
    let sql = format!(
        "SELECT {url_expr}, {target_expr}, {start_expr}, {bytes_expr} FROM downloads ORDER BY {start_expr} DESC LIMIT 500"
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, Option<i64>>(3)?,
        ))
    })?;
    let mut artifacts = Vec::new();
    for row in rows {
        let (url, target_path, raw_start, total_bytes) = row?;
        if url.trim().is_empty() && target_path.trim().is_empty() {
            continue;
        }
        let started_at = raw_start.and_then(chromium_time_to_dt);
        let mut attrs = browser_attrs(candidate, browser, profile);
        attrs.insert("url".to_string(), Value::String(url.clone()));
        attrs.insert("targetPath".to_string(), Value::String(target_path.clone()));
        attrs.insert(
            "totalBytes".to_string(),
            Value::Number(serde_json::Number::from(
                total_bytes.unwrap_or(0).max(0) as u64
            )),
        );
        if let Some(dt) = started_at {
            attrs.insert("startTime".to_string(), Value::String(dt.to_rfc3339()));
            events.push(make_timeline_event(
                &candidate.file_id,
                "BROWSER_DOWNLOAD",
                dt,
                format!("{} download: {}", browser, target_path),
                url.clone(),
                attrs.clone(),
                "browser.history",
            ));
        }
        artifacts.push(make_artifact(
            "BrowserDownload",
            format!("{} download: {}", browser, target_path),
            url,
            candidate,
            "browser.history",
            attrs,
        ));
    }
    Ok(artifacts)
}
