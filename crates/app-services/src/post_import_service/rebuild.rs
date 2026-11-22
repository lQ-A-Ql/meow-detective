use super::{ExecutableCapability, PostImportError, PostImportRuntime};
use domain::CaseId;
use std::path::Path;
use transport::dto::{TimelineProjectionStateDto, TimelineProjectionStatusDto};

pub fn rebuild_timeline(
    runtime: &PostImportRuntime,
    case: &CaseId,
    root: &Path,
    target: Option<&str>,
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
