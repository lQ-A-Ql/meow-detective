//! Capability execution uses the shared processing-phase attempt ledger.

use domain::DataSourceId;
use rusqlite::Connection;

use super::processing_phase_repo::{
    DataSourceProcessingPhaseRecord, DataSourceProcessingPhaseRepo, ProcessingPhase,
    ProcessingPhaseClaim, ProcessingPhaseCompletion, ProcessingPhaseState,
    ProcessingPhaseTransition,
};
use super::source_capability_repo::{SourceCapabilityRepo, SourceCapabilityRow};
use crate::{DbError, DbResult};

#[derive(Debug, Clone, Copy)]
pub struct CapabilityCompletion<'a> {
    pub state: &'a str,
    pub detail: Option<&'a str>,
    pub error_code: Option<&'a str>,
}

pub const CAPABILITY_EXECUTION_VERSION: u32 = 2;
const MAX_QUEUED_USER_RETRIES: usize = 4;

pub struct CapabilityClaimRequest<'a> {
    pub case_id: &'a str,
    pub source_id: &'a str,
    pub capability: &'a str,
    pub revision: i64,
    pub input_fingerprint: &'a str,
    pub owner: &'a str,
}

#[derive(Debug)]
pub struct CapabilityExecutionAttempt {
    pub row: SourceCapabilityRow,
    pub phase: DataSourceProcessingPhaseRecord,
}

pub enum CapabilityClaimOutcome {
    Acquired(CapabilityExecutionAttempt),
    Ready(SourceCapabilityRow),
    Busy,
    Superseded,
}

pub enum CapabilityQueueOutcome {
    Queued(SourceCapabilityRow),
    AlreadyActive,
    Full,
}

pub struct CapabilityExecutionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CapabilityExecutionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn queue_retry(
        &self,
        case: &str,
        source: &str,
        capability: &str,
    ) -> DbResult<CapabilityQueueOutcome> {
        phase_kind(capability)?;
        let transaction = self.conn.unchecked_transaction()?;
        ensure_source(&transaction, case, source)?;
        let repo = SourceCapabilityRepo::new(&transaction);
        let current = repo.get(case, source, capability)?;
        if current
            .as_ref()
            .is_some_and(|row| matches!(row.state.as_str(), "queued" | "running"))
        {
            return Ok(CapabilityQueueOutcome::AlreadyActive);
        }
        if repo
            .list_for_case(case)?
            .iter()
            .filter(|row| row.state == "queued" && row.phase.as_deref() == Some("userRetry"))
            .count()
            >= MAX_QUEUED_USER_RETRIES
        {
            return Ok(CapabilityQueueOutcome::Full);
        }
        let mut row = current.unwrap_or_else(|| new_row(case, source, capability));
        row.revision = row
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("capability revision overflow"))?;
        row.state = "queued".into();
        row.phase = Some("userRetry".into());
        row.detail = None;
        row.error_code = None;
        row.progress_done = 0;
        row.progress_total = None;
        row.derived = 0;
        row.updated_at = chrono::Utc::now().to_rfc3339();
        repo.upsert(&row)?;
        transaction.commit()?;
        Ok(CapabilityQueueOutcome::Queued(row))
    }

    pub fn claim(&self, request: CapabilityClaimRequest<'_>) -> DbResult<CapabilityClaimOutcome> {
        let phase = phase_kind(request.capability)?;
        let transaction = self.conn.unchecked_transaction()?;
        ensure_source(&transaction, request.case_id, request.source_id)?;
        let repo = SourceCapabilityRepo::new(&transaction);
        let Some(mut row) = repo.get(request.case_id, request.source_id, request.capability)?
        else {
            return Ok(CapabilityClaimOutcome::Superseded);
        };
        if row.revision != request.revision || !matches!(row.state.as_str(), "pending" | "queued") {
            return Ok(CapabilityClaimOutcome::Superseded);
        }
        let phase_repo = DataSourceProcessingPhaseRepo::new(&transaction);
        if phase_repo
            .find(&DataSourceId(request.source_id.to_owned()), phase)?
            .is_some_and(|attempt| attempt.state == ProcessingPhaseState::Running)
        {
            return Ok(CapabilityClaimOutcome::Busy);
        }
        let claim = phase_repo.claim_in_transaction(
            &DataSourceId(request.source_id.to_owned()),
            phase,
            CAPABILITY_EXECUTION_VERSION,
            request.input_fingerprint,
            request.owner,
        )?;
        let outcome = match claim {
            ProcessingPhaseClaim::Busy(_) => CapabilityClaimOutcome::Busy,
            ProcessingPhaseClaim::Ready(_) => {
                row.state = "ready".into();
                row.phase = Some("done".into());
                row.derived = 0;
                row.updated_at = chrono::Utc::now().to_rfc3339();
                repo.upsert(&row)?;
                CapabilityClaimOutcome::Ready(row)
            }
            ProcessingPhaseClaim::Acquired(attempt) => {
                row.state = "running".into();
                row.phase = Some("running".into());
                row.derived = 0;
                row.updated_at = chrono::Utc::now().to_rfc3339();
                repo.upsert(&row)?;
                CapabilityClaimOutcome::Acquired(CapabilityExecutionAttempt {
                    row,
                    phase: attempt,
                })
            }
        };
        transaction.commit()?;
        Ok(outcome)
    }

    pub fn complete(
        &self,
        attempt: &CapabilityExecutionAttempt,
        update: CapabilityCompletion<'_>,
    ) -> DbResult<Option<SourceCapabilityRow>> {
        let transaction = self.conn.unchecked_transaction()?;
        let repo = SourceCapabilityRepo::new(&transaction);
        let Some(mut row) = repo.get(
            &attempt.row.case_id,
            &attempt.row.data_source_id,
            &attempt.row.capability,
        )?
        else {
            return Ok(None);
        };
        if row.revision != attempt.row.revision || row.state != "running" {
            return Ok(None);
        }
        let phase_repo = DataSourceProcessingPhaseRepo::new(&transaction);
        let Some(current) = phase_repo.find(&attempt.phase.data_source_id, attempt.phase.phase)?
        else {
            return Ok(None);
        };
        if current.attempt_id != attempt.phase.attempt_id
            || current.owner_id != attempt.phase.owner_id
            || current.state != ProcessingPhaseState::Running
        {
            return Ok(None);
        }
        finish_attempt(&phase_repo, &attempt.phase, update)?;
        row.state = update.state.to_owned();
        row.detail = update.detail.map(str::to_owned);
        row.error_code = update.error_code.map(str::to_owned);
        row.phase = Some("done".into());
        row.derived = 0;
        row.updated_at = chrono::Utc::now().to_rfc3339();
        if update.state == "ready" {
            row.revision = row
                .revision
                .checked_add(1)
                .ok_or_else(|| invalid("capability revision overflow"))?;
        }
        repo.upsert(&row)?;
        transaction.commit()?;
        Ok(Some(row))
    }

    pub fn cancel(&self, case: &str, source: &str, capability: &str) -> DbResult<bool> {
        let phase = phase_kind(capability)?;
        let transaction = self.conn.unchecked_transaction()?;
        ensure_source(&transaction, case, source)?;
        let repo = SourceCapabilityRepo::new(&transaction);
        let Some(mut row) = repo.get(case, source, capability)? else {
            return Ok(false);
        };
        if !matches!(row.state.as_str(), "pending" | "queued" | "running") {
            return Ok(false);
        }
        let phase_repo = DataSourceProcessingPhaseRepo::new(&transaction);
        if let Some(current) = phase_repo.find(&DataSourceId(source.to_owned()), phase)? {
            if current.state == ProcessingPhaseState::Running {
                finish_attempt(
                    &phase_repo,
                    &current,
                    CapabilityCompletion {
                        state: "cancelled",
                        detail: Some("cancelled by user"),
                        error_code: None,
                    },
                )?;
            }
        }
        row.state = "cancelled".into();
        row.revision = row
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("capability revision overflow"))?;
        row.phase = None;
        row.detail = Some("cancelled by user".into());
        row.error_code = None;
        row.derived = 0;
        row.updated_at = chrono::Utc::now().to_rfc3339();
        repo.upsert(&row)?;
        transaction.commit()?;
        Ok(true)
    }
}

fn finish_attempt(
    repo: &DataSourceProcessingPhaseRepo<'_>,
    attempt: &DataSourceProcessingPhaseRecord,
    update: CapabilityCompletion<'_>,
) -> DbResult<()> {
    let state = match update.state {
        "ready" | "notApplicable" => ProcessingPhaseState::Ready,
        "deferred" => ProcessingPhaseState::Deferred,
        "partial" | "failed" | "cancelled" => ProcessingPhaseState::Failed,
        _ => return Err(invalid("capability completion requires a terminal state")),
    };
    let stats =
        serde_json::json!({ "capabilityState": update.state, "errorCode": update.error_code })
            .to_string();
    let reason = update.detail.or(update.error_code).unwrap_or(update.state);
    repo.finish(
        &attempt.data_source_id,
        attempt.phase,
        ProcessingPhaseCompletion::new(
            attempt.version,
            &attempt.input_fingerprint,
            attempt
                .owner_id
                .as_deref()
                .ok_or_else(|| invalid("capability attempt owner is missing"))?,
            attempt
                .attempt_id
                .as_deref()
                .ok_or_else(|| invalid("capability attempt identity is missing"))?,
            ProcessingPhaseTransition {
                state,
                stats_json: &stats,
                last_error: (state != ProcessingPhaseState::Ready).then_some(reason),
            },
        ),
    )?;
    Ok(())
}

fn phase_kind(capability: &str) -> DbResult<ProcessingPhase> {
    match capability {
        "timeline" => Ok(ProcessingPhase::Timeline),
        "artifacts" => Ok(ProcessingPhase::Artifacts),
        _ => Err(invalid("capability has no executable processing phase")),
    }
}

fn ensure_source(conn: &Connection, case: &str, source: &str) -> DbResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM data_sources WHERE id = ?1 AND case_id = ?2)",
        [source, case],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(invalid("capability source is not registered in the case"))
    }
}

fn new_row(case: &str, source: &str, capability: &str) -> SourceCapabilityRow {
    SourceCapabilityRow {
        case_id: case.into(),
        data_source_id: source.into(),
        capability: capability.into(),
        state: "pending".into(),
        revision: 0,
        progress_done: 0,
        progress_total: None,
        phase: None,
        detail: None,
        error_code: None,
        derived: 0,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn invalid(message: &str) -> DbError {
    DbError::System(message.to_owned())
}
