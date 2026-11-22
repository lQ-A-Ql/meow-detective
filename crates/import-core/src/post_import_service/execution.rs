use super::{events, CapabilityOutcome, CapabilityWork, ExecutableCapability, PostImportError};
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::capability_execution_repo::{
    CapabilityClaimOutcome, CapabilityClaimRequest, CapabilityCompletion, CapabilityExecutionRepo,
};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(super) fn run(work: CapabilityWork, cancel: Arc<AtomicBool>) -> Result<(), PostImportError> {
    wait_for_imports(&cancel)?;
    let conn = source_kernel::connection::open_existing_case_db(&work.case_root.join("app.db"))?;
    let input = input_fingerprint(&work)?;
    let owner = source_kernel::task_runtime::process_owner::current_owner()?;
    recover_abandoned_attempt(&conn, &work)?;
    let claimed = CapabilityExecutionRepo::new(&conn).claim(CapabilityClaimRequest {
        case_id: &work.row.case_id,
        source_id: &work.row.data_source_id,
        capability: work.kind.wire(),
        revision: work.row.revision,
        input_fingerprint: &input,
        owner: &owner,
    })?;
    let attempt = match claimed {
        CapabilityClaimOutcome::Acquired(attempt) => attempt,
        CapabilityClaimOutcome::Ready(row) => {
            return events::status(work.events.as_deref(), &row).map_err(Into::into)
        }
        CapabilityClaimOutcome::Busy | CapabilityClaimOutcome::Superseded => return Ok(()),
    };
    let mut outcome = if let Err(error) = events::status(work.events.as_deref(), &attempt.row) {
        tracing::error!(error = %error, "post-import status publication failed");
        CapabilityOutcome::failed("EVENT_PUBLICATION_FAILED")
    } else if cancel.load(Ordering::Acquire) {
        CapabilityOutcome::terminal("cancelled")
    } else {
        match work.kind {
            ExecutableCapability::Timeline => {
                work.timeline.project(super::TimelineProjectionRequest {
                    conn: &conn,
                    case_root: &work.case_root,
                    case_id: &CaseId(work.row.case_id.clone()),
                    source_id: &work.row.data_source_id,
                    cancel: &cancel,
                })
            }
            ExecutableCapability::Artifacts => super::artifacts::run(&work, &conn, &cancel),
        }
    };
    if outcome.state == "ready" {
        match input_fingerprint(&work) {
            Ok(current) if current == input => {}
            Ok(_) => outcome = CapabilityOutcome::failed("SOURCE_CHANGED"),
            Err(error) => {
                tracing::error!(error = %error, "post-import input verification failed");
                outcome = CapabilityOutcome::failed("SOURCE_UNAVAILABLE");
            }
        }
    }
    let update = CapabilityCompletion {
        state: outcome.state,
        detail: outcome.detail.as_deref(),
        error_code: outcome.error_code,
    };
    if let Some(row) = CapabilityExecutionRepo::new(&conn).complete(&attempt, update)? {
        events::status(work.events.as_deref(), &row)?;
        if row.state == "ready" && work.kind == ExecutableCapability::Timeline {
            events::timeline_updated(&work, outcome.timeline_events)?;
        }
    }
    Ok(())
}

fn input_fingerprint(work: &CapabilityWork) -> Result<String, PostImportError> {
    let source = source_kernel::source_db::open_registered_fjall_source_read_only(
        &work.case_root,
        &DataSourceId(work.row.data_source_id.clone()),
    )?;
    let manifest = persistence_source::fjall::read_source_generation_manifest(
        source.source_store(),
        &work.row.data_source_id,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(b"post-import-v2\0");
    digest.update(work.kind.wire().as_bytes());
    digest.update(manifest.immutable_data_digest.as_bytes());
    Ok(hex::encode(digest.finalize()))
}

fn recover_abandoned_attempt(
    conn: &rusqlite::Connection,
    work: &CapabilityWork,
) -> Result<(), PostImportError> {
    use persistence_sqlite::repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseCompletion,
        ProcessingPhaseState, ProcessingPhaseTransition,
    };
    let phase = match work.kind {
        ExecutableCapability::Timeline => ProcessingPhase::Timeline,
        ExecutableCapability::Artifacts => ProcessingPhase::Artifacts,
    };
    let repo = DataSourceProcessingPhaseRepo::new(conn);
    let Some(attempt) = repo.find(&DataSourceId(work.row.data_source_id.clone()), phase)? else {
        return Ok(());
    };
    if attempt.state != ProcessingPhaseState::Running || attempt.version != 2 {
        return Ok(());
    }
    let owner = attempt
        .owner_id
        .as_deref()
        .ok_or_else(|| std::io::Error::other("processing attempt owner is missing"))?;
    if !source_kernel::task_runtime::process_owner::owner_is_running(owner)? {
        repo.finish(
            &attempt.data_source_id,
            phase,
            ProcessingPhaseCompletion::new(
                attempt.version,
                &attempt.input_fingerprint,
                owner,
                attempt.attempt_id.as_deref().ok_or_else(|| {
                    std::io::Error::other("processing attempt identity is missing")
                })?,
                ProcessingPhaseTransition::failed("{}", "execution process exited"),
            ),
        )?;
    }
    Ok(())
}

fn wait_for_imports(cancel: &AtomicBool) -> Result<(), PostImportError> {
    let deadline = Instant::now() + Duration::from_secs(300);
    while crate::import_scheduler::global_import_admission().active_weight() > 0
        && Instant::now() < deadline
    {
        if cancel.load(Ordering::Acquire) {
            return Err(PostImportError::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if cancel.load(Ordering::Acquire) {
        return Err(PostImportError::Cancelled);
    }
    Ok(())
}
