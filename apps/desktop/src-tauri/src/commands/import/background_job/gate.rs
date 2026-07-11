use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::Duration;

use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use super::status::cancel_job;
use crate::events::event_bridge;

static IMPORT_JOB_GATE: OnceLock<Mutex<()>> = OnceLock::new();

pub(super) fn acquire_import_slot(
    job_repo: &JobRepo<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
    cancel_token: &Arc<AtomicBool>,
) -> Result<MutexGuard<'static, ()>, CommandError> {
    let gate = IMPORT_JOB_GATE.get_or_init(|| Mutex::new(()));
    let mut emitted_waiting = false;
    loop {
        if cancel_token.load(Ordering::Relaxed) {
            let message = "Import cancelled while waiting for import slot";
            cancel_job(job_repo, job_id, app, message);
            return Err(CommandError::internal(message));
        }

        match gate.try_lock() {
            Ok(guard) => {
                if emitted_waiting {
                    job_repo
                        .update_progress(job_id, 5, "Import slot acquired")
                        .map_err(CommandError::from_typed_service_error)?;
                    if let Some(app) = app {
                        event_bridge::emit_job_progress(app, &job_id.0, 5, "Import slot acquired");
                    }
                }
                return Ok(guard);
            }
            Err(TryLockError::WouldBlock) => {
                if !emitted_waiting {
                    job_repo
                        .update_progress(job_id, 2, "Waiting for import slot")
                        .map_err(CommandError::from_typed_service_error)?;
                    if let Some(app) = app {
                        event_bridge::emit_job_progress(
                            app,
                            &job_id.0,
                            2,
                            "Waiting for import slot",
                        );
                    }
                    emitted_waiting = true;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(TryLockError::Poisoned(poisoned)) => {
                tracing::warn!("Import job gate was poisoned; continuing with recovered guard");
                return Ok(poisoned.into_inner());
            }
        }
    }
}
