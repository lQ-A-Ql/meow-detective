use std::path::{Path, PathBuf};
use std::sync::mpsc;

use app_services::hash_service::evidence_jobs::update_hash_progress;
use domain::{DataSourceId, JobId};
use tauri::AppHandle;

use crate::events::event_bridge;

type ProgressUpdate = (u64, u64);

pub(super) struct ProgressReporter {
    sender: mpsc::SyncSender<ProgressUpdate>,
    worker: std::thread::JoinHandle<()>,
}

impl ProgressReporter {
    pub(super) fn sender(&self) -> mpsc::SyncSender<ProgressUpdate> {
        self.sender.clone()
    }
}

pub(super) fn spawn_progress_reporter(
    db_path: &Path,
    data_source_id: &DataSourceId,
    job_id: &JobId,
    app: Option<&AppHandle>,
) -> Option<ProgressReporter> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let path = db_path.to_path_buf();
    let source_id = data_source_id.clone();
    let job = job_id.clone();
    let app_handle = app.cloned();
    match std::thread::Builder::new()
        .name("meow-evidence-hash-progress".to_string())
        .spawn(move || run_progress_reporter(path, source_id, job, app_handle.as_ref(), receiver))
    {
        Ok(worker) => Some(ProgressReporter { sender, worker }),
        Err(error) => {
            tracing::warn!(%error, "Failed to start evidence hash progress reporter");
            None
        }
    }
}

fn run_progress_reporter(
    db_path: PathBuf,
    data_source_id: DataSourceId,
    job_id: JobId,
    app: Option<&AppHandle>,
    receiver: mpsc::Receiver<ProgressUpdate>,
) {
    let Ok(connection) = app_services::connection::open_case_db(&db_path) else {
        return;
    };
    let mut last_percent = 2;
    while let Ok((completed, total)) = receiver.recv() {
        let percent = hash_progress_percent(completed, total);
        if percent <= last_percent {
            continue;
        }
        last_percent = percent;
        if update_hash_progress(&connection, &data_source_id, &job_id, percent).is_err() {
            return;
        }
        if let Some(app) = app {
            event_bridge::emit_job_progress(app, &job_id.0, percent, "Hashing evidence");
        }
    }
}

pub(super) fn finish_progress_reporter(reporter: Option<ProgressReporter>) {
    let Some(reporter) = reporter else {
        return;
    };
    drop(reporter.sender);
    if reporter.worker.join().is_err() {
        tracing::warn!("Evidence hash progress reporter panicked");
    }
}

pub(super) fn hash_progress_percent(completed: u64, total: u64) -> u32 {
    if total == 0 {
        return 98;
    }
    let scaled = (u128::from(completed.min(total)) * 96) / u128::from(total);
    2 + u32::try_from(scaled).unwrap_or(96).min(96)
}
