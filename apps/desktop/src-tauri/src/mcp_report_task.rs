use std::{sync::atomic::AtomicBool, sync::Arc};

use persistence_sqlite::repositories::job_repo::JobRepo;
use transport::{commands::ExportScopeDto, CommandError};

use crate::state::{AppState, TaskScope};

pub(crate) fn start(
    state: &AppState,
    case: &domain::CaseMeta,
    case_id: &str,
    case_root: &std::path::Path,
    template_id: &str,
    format: &str,
) -> Result<String, CommandError> {
    if template_id != "report-summary" && template_id != "report-files" {
        return Err(CommandError::invalid_input("报告模板无效"));
    }
    if !matches!(format, "html" | "csv" | "json") {
        return Err(CommandError::invalid_input("报告格式无效"));
    }
    let db_path = case_root.join("app.db");
    let connection = app_services::connection::open_existing_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_id = JobRepo::new(&connection)
        .create(case_id, "Report export")
        .map_err(CommandError::from_typed_service_error)?;
    JobRepo::new(&connection)
        .update_progress(&job_id, 1, "Queued for report export")
        .map_err(CommandError::from_typed_service_error)?;
    drop(connection);

    let task_id = job_id.0.clone();
    let worker_db_path = db_path.clone();
    let worker_case = case.clone();
    let worker_case_root = case_root.to_path_buf();
    let worker_template = template_id.to_owned();
    let worker_format = format.to_owned();
    let worker_state = state.clone();
    let worker_task_id = task_id.clone();
    let token = Arc::new(AtomicBool::new(false));
    let worker_token = Arc::clone(&token);
    let scope = TaskScope::case(case_id, task_id.clone());
    let registration = state
        .task_manager
        .spawn_scoped(task_id.clone(), scope, token, move || {
            let connection = match app_services::connection::open_existing_case_db(&worker_db_path)
            {
                Ok(connection) => connection,
                Err(error) => return Err(error.to_string()),
            };
            let repo = JobRepo::new(&connection);
            let job = domain::JobId(worker_task_id.clone());
            if !repo.start(&job).map_err(|error| error.to_string())? {
                return Err("report job was cancelled before execution".into());
            }
            let result = generate(
                &worker_state,
                &connection,
                &worker_case,
                &worker_case_root,
                &worker_template,
                &worker_format,
                &worker_token,
            );
            match result {
                Ok(file_name) => {
                    repo.complete_committed(&job, &format!("Report completed: {file_name}"))
                        .map_err(|error| error.to_string())?;
                    Ok(())
                }
                Err(error) if error.category == "cancelled" => {
                    repo.cancel(&job, &error.message)
                        .map_err(|value| value.to_string())?;
                    Err(error.to_string())
                }
                Err(error) => {
                    repo.fail(&job, &error.message)
                        .map_err(|value| value.to_string())?;
                    Err(error.to_string())
                }
            }
        });
    if let Err(error) = registration {
        let connection = app_services::connection::open_existing_case_db(&db_path)
            .map_err(CommandError::from_typed_service_error)?;
        JobRepo::new(&connection)
            .fail(
                &job_id,
                &format!("Report task registration failed: {error}"),
            )
            .map_err(CommandError::from_typed_service_error)?;
        return Err(CommandError::internal(format!(
            "Report task registration failed: {error}"
        )));
    }
    Ok(task_id)
}

fn generate(
    state: &AppState,
    connection: &rusqlite::Connection,
    case: &domain::CaseMeta,
    case_root: &std::path::Path,
    template_id: &str,
    format: &str,
    cancel_token: &AtomicBool,
) -> Result<String, CommandError> {
    if cancel_token.load(std::sync::atomic::Ordering::Acquire) {
        return Err(CommandError::cancelled("报告任务已取消"));
    }
    let output_dir = case_root.join("reports");
    let scope = ExportScopeDto::default();
    match (template_id, format) {
        ("report-summary", "html") => {
            let bitlocker = app_services::report::BitLockerReportContext::new(
                app_services::bitlocker_service::BitLockerRuntimeContext::new(
                    &state.preview_runtime,
                    &state.bitlocker_runtime,
                    state.bitlocker_key_store.as_ref(),
                ),
            );
            app_services::report::generate_html_report_for_case_with_bitlocker(
                connection,
                case,
                case_root,
                &output_dir,
                &scope,
                bitlocker,
            )
            .map_err(CommandError::from_typed_service_error)
        }
        ("report-files", "csv") => app_services::report::generate_csv_artifacts_for_case(
            connection,
            case,
            case_root,
            &output_dir,
            &scope,
        )
        .map_err(CommandError::from_typed_service_error),
        (_, "json") => {
            let bitlocker = app_services::report::BitLockerReportContext::new(
                app_services::bitlocker_service::BitLockerRuntimeContext::new(
                    &state.preview_runtime,
                    &state.bitlocker_runtime,
                    state.bitlocker_key_store.as_ref(),
                ),
            );
            app_services::report::generate_json_export_for_case_with_bitlocker(
                connection,
                case,
                case_root,
                &output_dir,
                &scope,
                bitlocker,
            )
            .map_err(CommandError::from_typed_service_error)
        }
        _ => Err(CommandError::invalid_input("报告模板与格式组合无效")),
    }
}
