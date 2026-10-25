use super::{
    super::{
        browser_preload::BrowserPreloadContext,
        candidate_persistence::{
            delete_previous_outputs, persist_checkpoint, CandidateOutputBatch,
        },
        output_persistence::resolve_output_data_source_id,
        state::PersistedExtractionOutcome,
        ExtractionOutcome,
    },
    profile::browser_profile_from_path,
};
use crate::analysis_service::{
    cancellation::ensure_not_cancelled,
    candidates::{normalize_evidence_path, EvidenceCandidate},
    capability::AnalysisCapability,
    error::AnalysisServiceError,
};
use artifacts_browser::{sqlite::DatabaseSnapshot, BrowserSecretDecryptor};
use artifacts_core::ArtifactCompanion;
use rusqlite::Connection;
use std::{path::Path, sync::atomic::AtomicBool};

pub(in crate::analysis_service::extraction) struct PersistenceContext<'a> {
    pub connection: &'a Connection,
    pub case_id: &'a str,
    pub capability: AnalysisCapability,
    pub preload: &'a BrowserPreloadContext,
    pub cancel: &'a AtomicBool,
}

enum DatabaseView {
    Files(DatabaseSnapshot),
    Memory(Connection),
}
impl DatabaseView {
    fn connection(&self) -> &Connection {
        match self {
            Self::Files(snapshot) => snapshot.connection(),
            Self::Memory(connection) => connection,
        }
    }
    fn mode(&self) -> &'static str {
        match self {
            Self::Files(snapshot) => snapshot.input_mode(),
            Self::Memory(_) => "databaseOnly",
        }
    }
}

pub(in crate::analysis_service::extraction) fn persist(
    context: PersistenceContext<'_>,
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    companions: &[ArtifactCompanion],
) -> Result<PersistedExtractionOutcome, AnalysisServiceError> {
    let PersistenceContext {
        connection,
        case_id,
        capability,
        preload,
        cancel,
    } = context;
    ensure_not_cancelled(cancel)?;
    let source_id = resolve_output_data_source_id(connection)?;
    if source_id != candidate.data_source_id {
        return Err(AnalysisServiceError::InvalidInput(
            "browser source does not own the candidate".into(),
        ));
    }
    let view = open_view(connection, bytes, companions)?;
    let mode = view.mode();
    let transaction = connection.unchecked_transaction()?;
    delete_previous_outputs(&transaction, candidate, capability)?;
    let mut batch = CandidateOutputBatch::default();
    let mut sink = |mut outcome: ExtractionOutcome| {
        ensure_not_cancelled(cancel)?;
        for artifact in &mut outcome.artifacts {
            artifact.attrs.insert("sqliteInputMode".into(), mode.into());
        }
        batch.push(outcome);
        if batch.pending_artifact_count() >= 256 {
            batch.flush(&transaction, case_id, &source_id)?;
        }
        Ok(())
    };
    visit(view.connection(), candidate, preload, cancel, &mut sink)?;
    ensure_not_cancelled(cancel)?;
    batch.flush(&transaction, case_id, &source_id)?;
    let commit = batch.finish(Vec::new());
    persist_checkpoint(&transaction, candidate, capability, &commit)?;
    ensure_not_cancelled(cancel)?;
    transaction.commit()?;
    Ok(commit.observation)
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/browser_persistence.rs"]
mod tests;

fn open_view(
    connection: &Connection,
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
    let parent = connection
        .path()
        .map(Path::new)
        .and_then(Path::parent)
        .filter(|path| path.is_absolute());
    match parent {
        Some(parent) => Ok(DatabaseView::Files(DatabaseSnapshot::prepare(
            &parent.join("parser-inputs/browser"),
            bytes,
            wal,
            shm,
        )?)),
        None if wal.is_some() || shm.is_some() => Err(AnalysisServiceError::Unsupported(
            "browser WAL reading needs an explicit workspace".into(),
        )),
        None => Ok(DatabaseView::Memory(
            artifacts_browser::sqlite::open_sqlite_from_bytes(bytes)?,
        )),
    }
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
