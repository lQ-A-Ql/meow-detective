use super::super::ExtractionOutcome;
use crate::analysis_service::{candidates::EvidenceCandidate, error::AnalysisServiceError};
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

pub(super) fn extract_firefox_history(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let mut output = ExtractionOutcome::default();
    super::history::visit_history(
        connection,
        candidate,
        browser,
        profile,
        &AtomicBool::new(false),
        &mut |mut next| {
            output.artifacts.append(&mut next.artifacts);
            output.timeline_events.append(&mut next.timeline_events);
            Ok(())
        },
    )?;
    Ok(output)
}
