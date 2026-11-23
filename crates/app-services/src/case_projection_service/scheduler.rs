use import_core::post_import_service::{publish_application_event, ApplicationEventSink};
use persistence_sqlite::repositories::job_repo::JobRepo;
use source_kernel::task_runtime::{TaskManager, TaskRegistrationError, TaskScope};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex, OnceLock, Weak},
};
use transport::events::EventTopic;

const JOB_KIND: &str = "Case projections";

#[derive(Clone)]
pub struct CaseProjectionRuntime {
    pub tasks: Arc<TaskManager>,
    pub events: Option<Arc<dyn ApplicationEventSink>>,
}

#[derive(Debug, thiserror::Error)]
pub enum CaseProjectionScheduleError {
    #[error(transparent)]
    Database(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Runtime(#[from] std::io::Error),
    #[error(transparent)]
    Registration(#[from] TaskRegistrationError),
    #[error(transparent)]
    Job(#[from] crate::job_service::JobServiceError),
}

impl transport::ServiceErrorCategory for CaseProjectionScheduleError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Database(_) | Self::Runtime(_) => transport::ErrorCategory::Io,
            Self::Registration(_) => transport::ErrorCategory::Internal,
            Self::Job(error) => error.category(),
        }
    }
}

#[derive(Default)]
pub(super) struct Requests {
    pub(super) epoch: u64,
    pub(super) active_job: Option<String>,
}

type RequestMap = Mutex<HashMap<(String, PathBuf), Weak<Mutex<Requests>>>>;

pub fn schedule_case_projection(
    runtime: &CaseProjectionRuntime,
    case_id: &str,
    root: &Path,
) -> Result<Option<String>, CaseProjectionScheduleError> {
    let requests = requests_for(case_id, root)?;
    let conn = source_kernel::connection::open_existing_case_db(&root.join("app.db"))?;
    let repo = JobRepo::new(&conn);
    let job = {
        let mut state = requests
            .lock()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        state.epoch = state
            .epoch
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("case projection epoch overflow"))?;
        if state.active_job.is_some() {
            return Ok(None);
        }
        let job = repo.create(case_id, JOB_KIND)?;
        repo.update_progress(&job, 1, "Queued case projection build")?;
        state.active_job = Some(job.0.clone());
        job
    };
    let worker_runtime = runtime.clone();
    let worker_requests = Arc::clone(&requests);
    let worker_case = case_id.to_owned();
    let worker_root = root.to_owned();
    let worker_job = job.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let registration = runtime.tasks.spawn_scoped_heavy(
        job.0.clone(),
        TaskScope::case(
            case_id,
            format!("{}{case_id}", super::CASE_PROJECTION_TASK_GROUP_PREFIX),
        ),
        cancel,
        move || {
            let result = super::worker::run(
                &worker_runtime,
                &worker_case,
                &worker_root,
                &worker_job,
                &worker_requests,
                &worker_cancel,
            );
            let release = release_request(&worker_requests, &worker_job.0);
            result
                .and(release.map_err(Into::into))
                .map_err(|error| error.to_string())
        },
    );
    if let Err(error) = registration {
        release_request(&requests, &job.0)?;
        repo.fail(&job, "Case projection task could not start")?;
        return Err(error.into());
    }
    emit(
        runtime,
        case_id,
        &job.0,
        EventTopic::JobCreated,
        serde_json::json!({"name": JOB_KIND}),
    )?;
    Ok(Some(job.0))
}

fn requests_for(case_id: &str, root: &Path) -> std::io::Result<Arc<Mutex<Requests>>> {
    static REQUESTS: OnceLock<RequestMap> = OnceLock::new();
    let key = (case_id.to_owned(), std::fs::canonicalize(root)?);
    let mut states = REQUESTS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    states.retain(|_, state| state.strong_count() > 0);
    if let Some(state) = states.get(&key).and_then(Weak::upgrade) {
        return Ok(state);
    }
    let state = Arc::new(Mutex::new(Requests::default()));
    states.insert(key, Arc::downgrade(&state));
    Ok(state)
}

fn release_request(requests: &Mutex<Requests>, job_id: &str) -> std::io::Result<()> {
    let mut state = requests
        .lock()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    if state.active_job.as_deref() == Some(job_id) {
        state.active_job = None;
    }
    Ok(())
}

pub(super) fn emit(
    runtime: &CaseProjectionRuntime,
    case_id: &str,
    job_id: &str,
    topic: EventTopic,
    mut payload: serde_json::Value,
) -> std::io::Result<()> {
    payload["caseId"] = case_id.into();
    payload["jobId"] = job_id.into();
    publish_application_event(runtime.events.as_deref(), topic, payload)
}
