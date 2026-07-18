use app_services::{job_service, processing_phase_service};

use crate::state::AppState;

pub(super) fn recover_interrupted_jobs(state: &AppState) {
    // Recovery is best-effort so a stale job cannot prevent the case from opening.
    match state.get_connection() {
        Ok(conn) => {
            recover_jobs(&conn);
            recover_processing_phases(&conn);
        }
        Err(error) => {
            tracing::warn!("Failed to get connection for job recovery on case open: {error}");
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
