use super::ExtractionOutcome;
use crate::analysis_service::candidates::{
    is_browser_history_path, normalize_evidence_path, EvidenceCandidate,
};

mod chromium;
mod firefox;
mod profile;
mod records;
mod sqlite;

use chromium::extract_chromium_history;
use firefox::extract_firefox_history;
use profile::browser_profile_from_path;
use records::{extract_browser_cookies, extract_browser_passwords, extract_browser_sessions};
use sqlite::with_temp_sqlite;

pub(super) fn extract_browser_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let normalized = normalize_evidence_path(&candidate.path);
    if !is_browser_history_path(&normalized) {
        return ExtractionOutcome {
            warnings: vec![format!(
                "{} is not a recognized browser artifact",
                candidate.path
            )],
            ..ExtractionOutcome::default()
        };
    }

    let (browser, profile) = browser_profile_from_path(&normalized);
    let parse_result = if normalized.ends_with("/places.sqlite") {
        with_temp_sqlite(bytes, "browser-history", |db| {
            extract_firefox_history(db, candidate, &browser, &profile)
        })
    } else if normalized.ends_with("/history") || normalized.ends_with("/archived history") {
        with_temp_sqlite(bytes, "browser-history", |db| {
            extract_chromium_history(db, candidate, &browser, &profile)
        })
    } else if normalized.ends_with("/cookies") || normalized.ends_with("/cookies.sqlite") {
        extract_browser_cookies(candidate, bytes, &browser, &profile)
    } else if normalized.ends_with("/login data") || normalized.ends_with("/logins.json") {
        extract_browser_passwords(candidate, bytes, &browser, &profile)
    } else {
        extract_browser_sessions(candidate, bytes, &browser, &profile)
    };

    match parse_result {
        Ok(outcome) => outcome,
        Err(err) => ExtractionOutcome {
            warnings: vec![format!("{} browser parse failed: {}", candidate.path, err)],
            ..ExtractionOutcome::default()
        },
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/browser.rs"]
mod tests;
