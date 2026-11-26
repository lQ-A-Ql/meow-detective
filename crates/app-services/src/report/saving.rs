use super::ReportError;
use domain::CaseId;
use persistence_sqlite::repositories::report_repo::ReportRepo;
use rusqlite::Connection;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path},
};
use transport::commands::SaveReportRequest;

pub fn save_report_for_case(
    connection: &Connection,
    case_root: &Path,
    request: &SaveReportRequest,
) -> Result<(), ReportError> {
    let record = ReportRepo::new(connection)
        .find_by_id(&request.case_id, &request.report_id)?
        .ok_or_else(|| ReportError::NotFound("report was not found in this case".into()))?;
    if record.status != "completed" {
        return Err(ReportError::NotFound("report has not completed".into()));
    }
    let name = Path::new(&record.file_name);
    if name.components().count() != 1
        || !matches!(name.components().next(), Some(Component::Normal(_)))
    {
        return Err(ReportError::Other("invalid report file name".into()));
    }
    let root = case_root.join("reports").canonicalize()?;
    let source = root.join(name).canonicalize()?;
    if source.parent() != Some(root.as_path()) {
        return Err(ReportError::Other(
            "report file is outside its managed directory".into(),
        ));
    }
    let destination = evidence_access::file_service::prepare_case_export_destination(
        connection,
        case_root,
        &CaseId(request.case_id.clone()),
        Path::new(&request.destination),
        request.overwrite,
    )
    .map_err(ReportError::File)?;
    let parent = destination
        .parent()
        .ok_or_else(|| ReportError::Other("report destination has no parent".into()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let input = File::open(source)?;
    if record.file_size != Some(input.metadata()?.len()) {
        return Err(ReportError::Other("report file size check failed".into()));
    }
    let limit = input.metadata()?.len().saturating_add(1);
    std::io::copy(&mut input.take(limit), temporary.as_file_mut())?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    temporary.as_file_mut().seek(SeekFrom::Start(0))?;
    let (size, hash) = super::export::hash_reader(temporary.as_file_mut())?;
    if record.file_size != Some(size) || record.sha256.as_deref() != Some(&hash) {
        return Err(ReportError::Other(
            "report file integrity check failed".into(),
        ));
    }
    let published = if request.overwrite {
        temporary.persist(&destination)
    } else {
        temporary.persist_noclobber(&destination)
    };
    published.map_err(|error| ReportError::Io(error.error))?;
    Ok(())
}
