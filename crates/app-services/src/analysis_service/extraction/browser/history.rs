use super::super::ExtractionOutcome;
use crate::analysis_service::{
    artifact_builders::{browser_attrs, make_artifact, make_timeline_event, title_or_url},
    candidates::EvidenceCandidate,
    error::AnalysisServiceError,
};
use artifacts_browser::history::{BrowserFamily, HistoryKind, HistoryRow, VisitError};
use rusqlite::Connection;
use serde_json::Value;
use std::sync::atomic::AtomicBool;

pub(super) fn visit_history(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
    cancel: &AtomicBool,
    sink: &mut impl FnMut(ExtractionOutcome) -> Result<(), AnalysisServiceError>,
) -> Result<(), AnalysisServiceError> {
    let family = if browser == "Firefox" {
        BrowserFamily::Firefox
    } else {
        BrowserFamily::Chromium
    };
    let result = artifacts_browser::history::visit_history(connection, family, cancel, |row| {
        sink(project(candidate, browser, profile, row))
    });
    match result {
        Ok(()) => Ok(()),
        Err(VisitError::Parser(error)) => Err(error.into()),
        Err(VisitError::Sink(error)) => Err(error),
    }
}

fn project(
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
    row: HistoryRow,
) -> ExtractionOutcome {
    let mut attrs = browser_attrs(candidate, browser, profile);
    let url = row.url.as_deref().unwrap_or_default();
    let title = row.title.as_deref().unwrap_or_default();
    let mut outcome = ExtractionOutcome::default();
    attrs.insert("url".into(), url.into());
    attrs.insert("title".into(), title.into());
    attrs.insert("rowId".into(), row.row_id.to_string().into());
    attrs.insert(
        "historyRecordKind".into(),
        match row.kind {
            HistoryKind::Visit => "visit",
            HistoryKind::UrlSummary => "urlSummary",
        }
        .into(),
    );
    attrs.insert("rawTimeBasis".into(), row.clock.name().into());
    if let Some(id) = row.visit_id {
        attrs.insert("visitId".into(), id.to_string().into());
    }
    if let Some(id) = row.url_id {
        attrs.insert("urlId".into(), id.to_string().into());
    }
    if let Some(count) = row.visit_count {
        attrs.insert("visitCount".into(), (count.max(0) as u64).into());
        attrs.insert("rawVisitCount".into(), count.into());
    }
    if let Some(time) = row.raw_time {
        attrs.insert("rawTimestamp".into(), time.to_string().into());
    }
    if let Some(time) = row.raw_time.and_then(|raw| row.clock.datetime(raw)) {
        let key = if row.kind == HistoryKind::Visit {
            "visitTime"
        } else {
            "lastVisitTime"
        };
        attrs.insert(key.into(), time.to_rfc3339().into());
        if row.kind == HistoryKind::Visit {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "BROWSER_VISIT",
                time,
                format!("{browser} visit: {}", title_or_url(title, url)),
                url.into(),
                attrs.clone(),
                "browser.history",
            ));
        }
    }
    if row.url.is_none() {
        attrs.insert("urlUnavailable".into(), Value::Bool(true));
    }
    let label = if row.kind == HistoryKind::Visit {
        "visit"
    } else {
        "URL summary"
    };
    outcome.artifacts.push(make_artifact(
        "BrowserHistory",
        format!("{browser} {label}: {}", title_or_url(title, url)),
        url.into(),
        candidate,
        "browser.history",
        attrs,
    ));
    outcome
}
