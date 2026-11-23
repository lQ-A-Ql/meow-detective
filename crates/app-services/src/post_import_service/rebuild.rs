use super::{ExecutableCapability, PostImportError, PostImportRuntime};
use domain::CaseId;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;
use transport::dto::{TimelineProjectionStateDto, TimelineProjectionStatusDto};

pub fn rebuild_timeline(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
    target: Option<&str>,
) -> Result<TimelineProjectionStatusDto, PostImportError> {
    let worker_runtime = runtime.clone();
    let worker_case = case.clone();
    let worker_root = root.to_owned();
    let worker_target = target.map(str::to_owned);
    let task_id = format!("timeline-batch:{}:{}", case.0, uuid::Uuid::new_v4());
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let (sender, receiver) = mpsc::sync_channel(1);
    runtime.tasks.spawn_scoped(
        task_id.clone(),
        source_kernel::task_runtime::TaskScope::case(
            &case.0,
            format!(
                "{}{}|manual",
                import_core::post_import_service::CAPABILITY_TASK_GROUP_PREFIX,
                case.0
            ),
        ),
        Arc::clone(&cancel),
        move || {
            let result = rebuild_sources(
                &worker_runtime,
                &worker_case,
                &worker_root,
                worker_target.as_deref(),
                &worker_cancel,
            );
            let completed = result
                .as_ref()
                .map(|_| ())
                .map_err(|error| error.to_string());
            sender.send(result).map_err(|error| error.to_string())?;
            completed
        },
    )?;
    let result = receiver.recv().map_err(|_| PostImportError::Interrupted)?;
    while runtime.tasks.is_running(&task_id) {
        runtime
            .tasks
            .wait_task(&task_id, Duration::from_millis(250));
    }
    runtime.tasks.wait_for_group_prefix(
        &case.0,
        crate::case_projection_service::CASE_PROJECTION_TASK_GROUP_PREFIX,
        &cancel,
    )?;
    result
}

fn rebuild_sources(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
    target: Option<&str>,
    cancel: &AtomicBool,
) -> Result<TimelineProjectionStatusDto, PostImportError> {
    let conn = source_kernel::connection::open_existing_case_db(&root.join("app.db"))?;
    let status = projection_status(&conn, root, case)?;
    let sources = match target {
        Some(source) => vec![source.to_owned()],
        None => status
            .sources
            .into_iter()
            .filter(|source| source.state != TimelineProjectionStateDto::Complete)
            .map(|source| source.data_source_id)
            .collect(),
    };
    for source in sources {
        if cancel.load(Ordering::Acquire) {
            return Err(PostImportError::Cancelled);
        }
        import_core::post_import_service::request_and_wait(
            &runtime.core(),
            case,
            root,
            &source,
            ExecutableCapability::Timeline,
        )?;
    }
    projection_status(&conn, root, case)
}

fn projection_status(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
) -> Result<TimelineProjectionStatusDto, PostImportError> {
    crate::timeline_service::get_timeline_projection_status(conn, root, case)
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()).into())
}
