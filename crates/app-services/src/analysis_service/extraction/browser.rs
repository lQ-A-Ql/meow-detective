use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{
    browser_attrs, make_artifact, make_timeline_event, title_or_url,
};
use crate::analysis_service::candidates::{
    is_browser_history_path, normalize_evidence_path, EvidenceCandidate,
};
use crate::analysis_service::error::AnalysisServiceError;
use chrono::{DateTime, TimeZone, Utc};
use domain::{Artifact, TimelineEvent};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

fn with_temp_sqlite(
    bytes: &[u8],
    prefix: &str,
    parse: impl FnOnce(&Connection) -> Result<ExtractionOutcome, AnalysisServiceError>,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let path = temp_sqlite_path(prefix);
    std::fs::write(&path, bytes)?;
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let result = parse(&conn);
    let _ = std::fs::remove_file(path);
    result
}

fn temp_sqlite_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("forensics-{prefix}-{}.sqlite", Uuid::new_v4()))
}

fn table_exists(db: &Connection, table: &str) -> Result<bool, AnalysisServiceError> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn table_columns(db: &Connection, table: &str) -> Result<Vec<String>, AnalysisServiceError> {
    let mut stmt = db.prepare(&format!("PRAGMA table_info({})", table))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = Vec::new();
    for row in rows {
        columns.push(row?);
    }
    Ok(columns)
}

fn browser_profile_from_path(normalized: &str) -> (String, String) {
    let browser = if normalized.contains("/microsoft/edge/user data/") {
        "Edge"
    } else if normalized.contains("/mozilla/firefox/profiles/") {
        "Firefox"
    } else {
        "Chrome"
    };
    let marker = if browser == "Firefox" {
        "/mozilla/firefox/profiles/"
    } else if browser == "Edge" {
        "/microsoft/edge/user data/"
    } else {
        "/google/chrome/user data/"
    };
    let profile = normalized
        .split_once(marker)
        .map(|(_, rest)| rest.split('/').next().unwrap_or("default"))
        .filter(|value| !value.is_empty())
        .unwrap_or("default");
    (browser.to_string(), profile.to_string())
}

fn chromium_time_to_dt(value: i64) -> Option<DateTime<Utc>> {
    if value <= 0 {
        return None;
    }
    let seconds = value / 1_000_000 - 11_644_473_600;
    let nanos = ((value % 1_000_000) * 1_000) as u32;
    Utc.timestamp_opt(seconds, nanos).single()
}

fn unix_microseconds_to_dt(value: i64) -> Option<DateTime<Utc>> {
    if value <= 0 {
        return None;
    }
    Utc.timestamp_opt(value / 1_000_000, ((value % 1_000_000) * 1_000) as u32)
        .single()
}

pub(super) fn extract_browser_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let normalized = normalize_evidence_path(&candidate.path);
    if !is_browser_history_path(&normalized) {
        return ExtractionOutcome {
            warnings: vec![format!(
                "{} is not a browser history database",
                candidate.path
            )],
            ..ExtractionOutcome::default()
        };
    }
    let (browser, profile) = browser_profile_from_path(&normalized);
    let parse_result = with_temp_sqlite(bytes, "browser-history", |db| {
        if normalized.ends_with("/places.sqlite") {
            extract_firefox_history(db, candidate, &browser, &profile)
        } else {
            extract_chromium_history(db, candidate, &browser, &profile)
        }
    });
    match parse_result {
        Ok(outcome) => outcome,
        Err(err) => ExtractionOutcome {
            warnings: vec![format!("{} browser parse failed: {}", candidate.path, err)],
            ..ExtractionOutcome::default()
        },
    }
}

fn extract_chromium_history(
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

fn extract_firefox_history(
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
