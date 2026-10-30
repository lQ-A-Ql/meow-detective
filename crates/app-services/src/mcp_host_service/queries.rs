use super::{arguments::validate_arguments, McpHostServiceError};
use domain::CaseMeta;
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

pub struct McpHostQueryContext<'a> {
    pub connection: &'a Connection,
    pub case_root: &'a Path,
    pub case_meta: &'a CaseMeta,
}

pub fn execute_tool(
    context: McpHostQueryContext<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    validate_arguments(name, arguments)?;
    match name {
        "forensics.get_current_case" => Ok(super::current_case::get_current_case(&context)),
        "forensics.list_jobs" => {
            let jobs = crate::job_service::get_jobs_from_db(context.connection)
                .map_err(|error| McpHostServiceError::Invalid(error.to_string()))?;
            serde_json::to_value(jobs)
                .map(|value| {
                    value
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .cloned()
                                .map(super::sanitization::job_snapshot)
                                .collect()
                        })
                        .unwrap_or(value)
                })
                .map_err(McpHostServiceError::from)
        }
        "forensics.get_job" => {
            let request: transport::dto::mcp_host::McpHostJobRequestDto =
                serde_json::from_value(arguments.clone())?;
            let job = crate::job_service::get_job_from_db(
                context.connection,
                &domain::JobId(request.job_id),
            )
            .map_err(|error| match error {
                crate::job_service::JobServiceError::NotFound(_) => McpHostServiceError::NotFound,
                other => McpHostServiceError::Invalid(other.to_string()),
            })?;
            serde_json::to_value(job)
                .map(super::sanitization::job_snapshot)
                .map_err(McpHostServiceError::from)
        }
        "forensics.list_data_sources" => super::data_sources::list_data_sources(&context),
        "forensics.get_data_source" => super::data_sources::get_data_source(&context, arguments),
        "forensics.list_plugin_modules" => {
            super::plugin_modules::list_plugin_modules(&context, arguments)
        }
        "forensics.get_plugin_module" => {
            super::plugin_modules::get_plugin_module(&context, arguments)
        }
        "forensics.get_timeline_events"
        | "forensics.get_timeline_event"
        | "forensics.get_timeline_facets" => {
            super::timeline::execute_timeline_tool(&context, name, arguments)
        }
        "forensics.search_files" => super::search::execute_search_tool(&context, arguments),
        "forensics.get_content_index_coverage" => {
            super::content_index::get_content_index_coverage(&context, arguments)
        }
        _ => Err(McpHostServiceError::InvalidInput),
    }
}
