use crate::state::{AppState, TaskScope};
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
use transport::{
    commands::{ExportReportRequest, ReportScopeDto},
    dto::ReportFormatDto,
    CommandError,
};

pub(crate) fn start(
    state: &AppState,
    case: &domain::CaseMeta,
    case_id: &str,
    case_root: &Path,
    template_id: &str,
    format: &str,
) -> Result<String, CommandError> {
    if template_id != "report-summary" {
        return Err(CommandError::invalid_input("报告模板无效"));
    }
    let format: ReportFormatDto = serde_json::from_value(serde_json::json!(format))
        .map_err(|_| CommandError::invalid_input("报告格式无效"))?;
    let path = case_root.join("app.db");
    let connection = app_services::connection::open_existing_case_db(&path)
        .map_err(app_services::connection::command_error_from_db)?;
    let repo = JobRepo::new(&connection);
    let job = repo
        .create(case_id, "Report export")
        .map_err(app_services::connection::command_error_from_db)?;
    let case = case.clone();
    let case_root = case_root.to_owned();
    let request = ExportReportRequest {
        case_id: case_id.to_owned(),
        format,
        scope: ReportScopeDto::default(),
        data_source_id: None,
    };
    let token = Arc::new(AtomicBool::new(false));
    let worker_token = token.clone();
    let worker_id = job.clone();
    let registration = state.task_manager.spawn_scoped(
        job.0.clone(),
        TaskScope::case(case_id, job.0.clone()),
        token,
        move || {
            let connection =
                app_services::connection::open_existing_case_db(&case_root.join("app.db"))
                    .map_err(|error| error.to_string())?;
            let repo = JobRepo::new(&connection);
            if !repo.start(&worker_id).map_err(|error| error.to_string())? {
                return Err("report job was cancelled before execution".into());
            }
            match app_services::report::export_report_for_case(
                app_services::report::ReportExportContext {
                    connection: &connection,
                    case: &case,
                    case_root: &case_root,
                    request: &request,
                    cancel: &worker_token,
                },
            ) {
                Ok(report) => {
                    repo.complete_committed(&worker_id, &report.file_name)
                        .map_err(|error| error.to_string())?;
                    Ok(())
                }
                Err(error) => {
                    let error = CommandError::from_typed_service_error(error);
                    if error.category == "cancelled" {
                        repo.cancel(&worker_id, &error.message).map(|_| ())
                    } else {
                        repo.fail(&worker_id, &error.message)
                    }
                    .map_err(|error| error.to_string())?;
                    Err(error.to_string())
                }
            }
        },
    );
    if let Err(error) = registration {
        repo.fail(&job, &error.to_string())
            .map_err(app_services::connection::command_error_from_db)?;
        return Err(CommandError::internal(error.to_string()));
    }
    Ok(job.0)
}
