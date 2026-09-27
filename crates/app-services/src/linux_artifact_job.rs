use domain::{CaseId, DataSourceId, JobId};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use persistence_sqlite::DbResult;
use rusqlite::Connection;

pub fn source_is_ready_linux(
    connection: &Connection,
    data_source_id: &DataSourceId,
) -> DbResult<bool> {
    let storage = DataSourceRepo::new(connection).find_storage(data_source_id)?;
    Ok(storage.is_some_and(|value| value.platform == "linux" && value.import_state == "ready"))
}

pub fn create_job(connection: &Connection, case_id: &CaseId) -> DbResult<JobId> {
    JobRepo::new(connection).create(&case_id.0, "Linux artifact analysis")
}

pub fn update_progress(
    connection: &Connection,
    job_id: &JobId,
    progress: u32,
    detail: &str,
) -> DbResult<()> {
    JobRepo::new(connection).update_progress(job_id, progress, detail)
}

pub fn complete(connection: &Connection, job_id: &JobId, detail: &str) -> DbResult<()> {
    JobRepo::new(connection).complete(job_id, detail)
}

pub fn fail(connection: &Connection, job_id: &JobId, detail: &str) -> DbResult<()> {
    JobRepo::new(connection).fail(job_id, detail)
}
