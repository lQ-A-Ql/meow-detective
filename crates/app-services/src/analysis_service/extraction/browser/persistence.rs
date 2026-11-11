use super::{
    super::{
        browser_preload::BrowserPreloadContext,
        native_output::{ConcurrentOutputBatch, StreamingGeneration},
        state::PersistedExtractionOutcome,
        ExtractionOutcome,
    },
    profile::browser_profile_from_path,
};
use crate::analysis_service::{
    cancellation::ensure_not_cancelled,
    candidates::{normalize_evidence_path, EvidenceCandidate},
    error::AnalysisServiceError,
};
use artifacts_browser::{sqlite::DatabaseSnapshot, BrowserSecretDecryptor};
use artifacts_core::ArtifactCompanion;
use rusqlite::Connection;
use std::{path::Path, sync::atomic::AtomicBool};

enum DatabaseView {
    Files(DatabaseSnapshot),
}
impl DatabaseView {
    fn connection(&self) -> &Connection {
        match self {
            Self::Files(snapshot) => snapshot.connection(),
        }
    }
    fn mode(&self) -> &'static str {
        match self {
            Self::Files(snapshot) => snapshot.input_mode(),
        }
    }
}

pub(in crate::analysis_service::extraction) fn persist_worker(
    output: &StreamingGeneration,
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    companions: &[ArtifactCompanion],
    preload: &BrowserPreloadContext,
    cancel: &AtomicBool,
) -> Result<PersistedExtractionOutcome, AnalysisServiceError> {
    ensure_not_cancelled(cancel)?;
    let parser_workspace = output.parser_workspace()?;
    let view = open_view(&parser_workspace, bytes, companions)?;
    let mode = view.mode();
    let mut batch = ConcurrentOutputBatch::new(output);
    let mut sink = |mut outcome: ExtractionOutcome| {
        ensure_not_cancelled(cancel)?;
        for artifact in &mut outcome.artifacts {
            artifact.attrs.insert("sqliteInputMode".into(), mode.into());
        }
        batch.push(outcome);
        if batch.pending_artifact_count() >= 256 {
            batch.flush()?;
        }
        Ok(())
    };
    visit(view.connection(), candidate, preload, cancel, &mut sink)?;
    ensure_not_cancelled(cancel)?;
    batch.finish(Vec::new())
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/browser_persistence.rs"]
mod tests;

fn open_view(
    parser_workspace: &Path,
    bytes: &[u8],
    companions: &[ArtifactCompanion],
) -> Result<DatabaseView, AnalysisServiceError> {
    let wal = companions
        .iter()
        .find(|entry| entry.file_path.to_ascii_lowercase().ends_with("-wal"))
        .map(|entry| entry.data.as_slice());
    let shm = companions
        .iter()
        .find(|entry| entry.file_path.to_ascii_lowercase().ends_with("-shm"))
        .map(|entry| entry.data.as_slice());
    Ok(DatabaseView::Files(DatabaseSnapshot::prepare(
        parser_workspace,
        bytes,
        wal,
        shm,
    )?))
}

fn visit(
    database: &Connection,
    candidate: &EvidenceCandidate,
    preload: &BrowserPreloadContext,
    cancel: &AtomicBool,
    sink: &mut impl FnMut(ExtractionOutcome) -> Result<(), AnalysisServiceError>,
) -> Result<(), AnalysisServiceError> {
    let path = normalize_evidence_path(&candidate.path);
    let (browser, profile) = browser_profile_from_path(&path);
    let decryptor = preload
        .decryptor_for(candidate)
        .map(|value| value as &dyn BrowserSecretDecryptor);
    if path.ends_with("/places.sqlite") {
        super::history::visit_history(database, candidate, &browser, &profile, cancel, sink)
    } else if path.ends_with("/history") || path.ends_with("/archived history") {
        super::chromium::visit(database, candidate, &browser, &profile, cancel, sink)
    } else if path.ends_with("/cookies") || path.ends_with("/cookies.sqlite") {
        let cookies = if browser == "Firefox" {
            artifacts_browser::firefox::parse_firefox_cookies_from_connection(database)?
        } else {
            artifacts_browser::chromium::cookies::parse_chrome_cookies_from_connection(
                database, decryptor,
            )?
        };
        for cookie in cookies {
            sink(super::records::project_cookie(
                candidate, &browser, &profile, cookie,
            ))?;
        }
        Ok(())
    } else {
        for password in artifacts_browser::chromium::parse_chrome_passwords_from_connection(
            database,
            &browser,
            Some(&profile),
            decryptor,
        )? {
            sink(super::records::project_password(
                candidate, &browser, &profile, password,
            ))?;
        }
        Ok(())
    }
}
