use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};

use app_services::{job_service, processing_phase_service};

use crate::{
    commands::import::background_job::{
        continue_cluster_rbd_processing, BackgroundDerivedSourceProcessingJob,
    },
    state::{AppState, TaskScope},
};

pub(super) fn recover_interrupted_jobs(state: &AppState) {
    // Recovery is best-effort so a stale job cannot prevent the case from opening.
    match state.get_connection() {
        Ok(conn) => {
            recover_jobs(&conn);
            recover_processing_phases(&conn);
            resume_derived_source_processing(state, &conn);
        }
        Err(error) => {
            tracing::warn!("Failed to get connection for job recovery on case open: {error}");
        }
    }
}

fn resume_derived_source_processing(state: &AppState, conn: &rusqlite::Connection) {
    let active = match state.active_case.lock() {
        Ok(guard) => guard.as_ref().map(|active| {
            (
                active.meta.id.clone(),
                active.case_root.clone(),
                active.db_path(),
            )
        }),
        Err(error) => {
            tracing::warn!("Failed to inspect active case for processing recovery: {error}");
            return;
        }
    };
    let Some((case_id, case_root, db_path)) = active else {
        return;
    };
    let source_ids = match processing_phase_service::retryable_derived_sources(conn, &case_id) {
        Ok(source_ids) => source_ids,
        Err(error) => {
            tracing::warn!("Failed to discover retryable derived sources: {error}");
            return;
        }
    };
    for data_source_id in source_ids {
        schedule_derived_source_recovery(state, &db_path, &case_id, &case_root, data_source_id);
    }
}

fn schedule_derived_source_recovery(
    state: &AppState,
    db_path: &Path,
    case_id: &domain::CaseId,
    case_root: &Path,
    data_source_id: domain::DataSourceId,
) {
    let task_id = format!("case-open-recovery:derived-processing:{}", data_source_id.0);
    if state.task_manager.is_running(&task_id) {
        return;
    }
    let cancel_token = Arc::new(AtomicBool::new(false));
    let worker_cancel_token = cancel_token.clone();
    let job = BackgroundDerivedSourceProcessingJob {
        db_path: PathBuf::from(db_path),
        case_id: case_id.clone(),
        case_root: PathBuf::from(case_root),
        cluster_id: "case-open-recovery".to_string(),
        source_ids: vec![data_source_id.clone()],
    };
    let registration = state.task_manager.spawn_scoped_heavy(
        task_id.clone(),
        TaskScope::data_source(case_id.0.clone(), data_source_id.0, "case-open-recovery"),
        cancel_token,
        move || {
            continue_cluster_rbd_processing(&job, &worker_cancel_token)
                .map_err(|error| error.message)
        },
    );
    match registration {
        Ok(()) => tracing::info!(task_id, "Resumed derived-source background processing"),
        Err(error) => {
            tracing::warn!(task_id, %error, "Derived-source recovery task was not admitted")
        }
    }
}

fn recover_jobs(conn: &rusqlite::Connection) {
    match job_service::recover_interrupted_jobs(conn) {
        Ok(recovery) => {
            if !recovery.recovered_job_ids.is_empty() {
                tracing::info!(
                    "Recovered {} interrupted job(s): {:?}",
                    recovery.recovered_job_ids.len(),
                    recovery.recovered_job_ids
                );
            }
        }
        Err(error) => {
            tracing::warn!("Failed to recover interrupted jobs on case open: {error}");
        }
    }
}

fn recover_processing_phases(conn: &rusqlite::Connection) {
    match processing_phase_service::recover_interrupted_processing_phases(conn) {
        Ok(recovered) if recovered > 0 => {
            tracing::info!("Recovered {recovered} interrupted processing phase(s)");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!("Failed to recover interrupted processing phases on case open: {error}");
        }
    }
}
