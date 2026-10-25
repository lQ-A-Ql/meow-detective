use super::browser_preload::BrowserPreloadContext;
use super::ExtractionOutcome;
use crate::analysis_service::candidates::{
    is_browser_history_path, normalize_evidence_path, EvidenceCandidate,
};

mod chromium;
mod extensions;
mod firefox;
mod history;
pub(super) mod persistence;
mod profile;
mod records;
mod sqlite;

use chromium::extract_chromium_history;
use extensions::extract_firefox_extensions;
use firefox::extract_firefox_history;
use profile::browser_profile_from_path;
use records::{extract_browser_cookies, extract_browser_passwords, extract_browser_sessions};
use sqlite::with_database;

pub(super) fn extract_browser_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    browser_preload: &BrowserPreloadContext,
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
    if normalized.ends_with("/extensions.json") {
        return match extract_firefox_extensions(candidate, bytes, &profile) {
            Ok(outcome) => outcome,
            Err(err) => ExtractionOutcome {
                warnings: vec![format!(
                    "{} browser extension parse failed: {}",
                    candidate.path, err
                )],
                ..ExtractionOutcome::default()
            },
        };
    }
    let decryptor = browser_preload.decryptor_for(candidate);
    let parse_result = if normalized.ends_with("/places.sqlite") {
        with_database(bytes, "browser-history", |db| {
            extract_firefox_history(db, candidate, &browser, &profile)
        })
    } else if normalized.ends_with("/history") || normalized.ends_with("/archived history") {
        with_database(bytes, "browser-history", |db| {
            extract_chromium_history(db, candidate, &browser, &profile)
        })
    } else if normalized.ends_with("/cookies") || normalized.ends_with("/cookies.sqlite") {
        extract_browser_cookies(candidate, bytes, &browser, &profile, decryptor)
    } else if normalized.ends_with("/login data") || normalized.ends_with("/logins.json") {
        extract_browser_passwords(candidate, bytes, &browser, &profile, decryptor)
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

pub(super) fn is_sqlite_path(path: &str) -> bool {
    let path = normalize_evidence_path(path);
    path.ends_with("/history")
        || path.ends_with("/archived history")
        || path.ends_with("/places.sqlite")
        || path.ends_with("/cookies")
        || path.ends_with("/cookies.sqlite")
        || path.ends_with("/login data")
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/browser.rs"]
mod tests;
