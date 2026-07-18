use std::path::{Path, PathBuf};
use std::time::Duration;

use app_services::active_case::ActiveCase;
use transport::CommandError;

use super::close::drain_active_case_jobs;
use crate::state::AppState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActiveCaseIdentity {
    pub(super) case_id: String,
    pub(super) case_root: PathBuf,
}

pub(super) struct ActiveCaseTransition {
    previous: Option<ActiveCase>,
    previous_identity: Option<ActiveCaseIdentity>,
    next_identity: ActiveCaseIdentity,
}

pub(super) fn active_case_identity(
    state: &AppState,
) -> Result<Option<ActiveCaseIdentity>, CommandError> {
    let guard = state
        .active_case
        .lock()
        .map_err(|error| CommandError::from_lock_error("Case", error))?;
    Ok(guard.as_ref().map(identity_for))
}

pub(super) fn begin_active_case_transition(
    state: &AppState,
    next: ActiveCase,
    timeout: Duration,
) -> Result<ActiveCaseTransition, CommandError> {
    let next_identity = identity_for(&next);
    let previous_identity = active_case_identity(state)?;
    if let Some(previous) = &previous_identity {
        retire_case_runtime(state, previous, timeout)?;
    }

    state.task_manager.reactivate_case(&next_identity.case_id);
    if let Err(error) = state.reactivate_preview_case(&next_identity.case_id) {
        reactivate_previous_case(state, previous_identity.as_ref());
        return Err(CommandError::from_service_error(error));
    }

    let previous = match state.active_case.lock() {
        Ok(mut guard) => {
            if !matches_identity(guard.as_ref(), previous_identity.as_ref()) {
                reactivate_previous_case(state, previous_identity.as_ref());
                return Err(CommandError::conflict(
                    "Active case changed during a serialized case transition",
                ));
            }
            guard.replace(next)
        }
        Err(error) => {
            reactivate_previous_case(state, previous_identity.as_ref());
            return Err(CommandError::from_lock_error("Case", error));
        }
    };

    Ok(ActiveCaseTransition {
        previous,
        previous_identity,
        next_identity,
    })
}

impl ActiveCaseTransition {
    pub(super) fn commit(self, state: &AppState) {
        if let Some(previous) = self.previous_identity {
            let _ = state.clear_runtime_cache_for_case(&previous.case_id);
            app_services::file_service::clear_e01_reader_cache_for_case(&previous.case_id);
        }
        drop(self.previous);
    }

    pub(super) fn rollback(self, state: &AppState, timeout: Duration) {
        let _ = state
            .task_manager
            .retire_case_and_drain(&self.next_identity.case_id, timeout);
        if let Err(error) = state.retire_preview_case(&self.next_identity.case_id, timeout) {
            tracing::error!(
                case_id = self.next_identity.case_id,
                %error,
                "Failed to retire the replacement case preview runtime during rollback"
            );
        }

        match state.active_case.lock() {
            Ok(mut guard) => {
                if matches_identity(guard.as_ref(), Some(&self.next_identity)) {
                    *guard = self.previous;
                } else {
                    tracing::error!(
                        expected_case_id = self.next_identity.case_id,
                        "Refused to overwrite an unexpected active case during rollback"
                    );
                }
            }
            Err(error) => {
                tracing::error!("Failed to restore the previous active case: {error}");
            }
        }

        let _ = state.clear_runtime_cache_for_case(&self.next_identity.case_id);
        app_services::file_service::clear_e01_reader_cache_for_case(&self.next_identity.case_id);
        reactivate_previous_case(state, self.previous_identity.as_ref());
    }
}

pub(super) fn clear_active_case_if_matches(
    state: &AppState,
    expected: &ActiveCaseIdentity,
) -> Result<bool, CommandError> {
    let mut guard = state
        .active_case
        .lock()
        .map_err(|error| CommandError::from_lock_error("Case", error))?;
    if matches_identity(guard.as_ref(), Some(expected)) {
        *guard = None;
        return Ok(true);
    }
    Ok(false)
}

fn retire_case_runtime(
    state: &AppState,
    identity: &ActiveCaseIdentity,
    timeout: Duration,
) -> Result<(), CommandError> {
    drain_active_case_jobs(state, &identity.case_id, timeout)?;
    let drained = state
        .retire_preview_case(&identity.case_id, timeout)
        .map_err(CommandError::from_service_error)?;
    if drained {
        return Ok(());
    }

    state.task_manager.reactivate_case(&identity.case_id);
    let _ = state.reactivate_preview_case(&identity.case_id);
    Err(CommandError::timeout(
        "Timed out waiting for active preview reads to finish",
    ))
}

fn reactivate_previous_case(state: &AppState, identity: Option<&ActiveCaseIdentity>) {
    if let Some(identity) = identity {
        state.task_manager.reactivate_case(&identity.case_id);
        if let Err(error) = state.reactivate_preview_case(&identity.case_id) {
            tracing::error!(
                case_id = identity.case_id,
                %error,
                "Failed to reactivate the previous case preview runtime"
            );
        }
    }
}

fn identity_for(active: &ActiveCase) -> ActiveCaseIdentity {
    ActiveCaseIdentity {
        case_id: active.meta.id.0.clone(),
        case_root: active.case_root.clone(),
    }
}

fn matches_identity(active: Option<&ActiveCase>, expected: Option<&ActiveCaseIdentity>) -> bool {
    match (active, expected) {
        (None, None) => true,
        (Some(active), Some(expected)) => {
            active.meta.id.0 == expected.case_id
                && same_path(&active.case_root, &expected.case_root)
        }
        _ => false,
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
