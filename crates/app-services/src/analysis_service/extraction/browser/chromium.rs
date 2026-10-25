use super::super::ExtractionOutcome;
use crate::analysis_service::{
    artifact_builders::{browser_attrs, make_artifact, make_timeline_event},
    candidates::EvidenceCandidate,
    error::AnalysisServiceError,
};
use artifacts_browser::{chromium::downloads::DownloadRow, history::VisitError};
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

pub(super) fn extract_chromium_history(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let mut output = ExtractionOutcome::default();
    let cancel = AtomicBool::new(false);
    visit(
        connection,
        candidate,
        browser,
        profile,
        &cancel,
        &mut |mut next| {
            output.artifacts.append(&mut next.artifacts);
            output.timeline_events.append(&mut next.timeline_events);
            Ok(())
        },
    )?;
    Ok(output)
}

pub(super) fn visit(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
    cancel: &AtomicBool,
    sink: &mut impl FnMut(ExtractionOutcome) -> Result<(), AnalysisServiceError>,
) -> Result<(), AnalysisServiceError> {
    super::history::visit_history(connection, candidate, browser, profile, cancel, sink)?;
    match artifacts_browser::chromium::downloads::visit_downloads(connection, cancel, |row| {
        sink(project_download(candidate, browser, profile, row))
    }) {
        Ok(()) => Ok(()),
        Err(VisitError::Parser(error)) => Err(error.into()),
        Err(VisitError::Sink(error)) => Err(error),
    }
}

fn project_download(
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
    row: DownloadRow,
) -> ExtractionOutcome {
    let url = row.url.unwrap_or_default();
    let target = row.target_path.unwrap_or_default();
    let mut attrs = browser_attrs(candidate, browser, profile);
    attrs.insert("rowId".into(), row.row_id.to_string().into());
    attrs.insert("url".into(), url.clone().into());
    attrs.insert("targetPath".into(), target.clone().into());
    attrs.insert("urlSourceColumn".into(), row.url_source.into());
    attrs.insert("rawTimeBasis".into(), "webkitMicroseconds1601".into());
    if let Some(time) = row.raw_start {
        attrs.insert("rawStartTimestamp".into(), time.to_string().into());
    }
    if let Some(time) = row.raw_end {
        attrs.insert("rawEndTimestamp".into(), time.to_string().into());
    }
    if let Some(bytes) = row.total_bytes {
        attrs.insert("totalBytes".into(), bytes.max(0).into());
    }
    let mut outcome = ExtractionOutcome::default();
    if let Some(time) = row
        .raw_start
        .and_then(artifacts_browser::chromium::time::webkit_time_to_dt)
    {
        attrs.insert("startTime".into(), time.to_rfc3339().into());
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "BROWSER_DOWNLOAD",
            time,
            format!("{browser} download: {target}"),
            url.clone(),
            attrs.clone(),
            "browser.history",
        ));
    }
    if let Some(time) = row
        .raw_end
        .and_then(artifacts_browser::chromium::time::webkit_time_to_dt)
    {
        attrs.insert("endTime".into(), time.to_rfc3339().into());
    }
    outcome.artifacts.push(make_artifact(
        "BrowserDownload",
        format!("{browser} download: {target}"),
        url,
        candidate,
        "browser.history",
        attrs,
    ));
    outcome
}
