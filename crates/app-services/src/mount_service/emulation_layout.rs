//! Source-byte layout preflight, shared by UI inspection and session admission.
use super::MountServiceError;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use transport::dto::DirectNtfsVolumeDto;

pub(crate) fn direct_volume(
    conn: &Connection,
    case: &CaseId,
    source: &DataSourceId,
) -> Result<Option<DirectNtfsVolumeDto>, MountServiceError> {
    if !DataSourceRepo::new(conn)
        .find_by_case(case)?
        .iter()
        .any(|item| item.id == *source)
    {
        return Err(MountServiceError::NotFound(
            "data source in active case".into(),
        ));
    }
    let prepared = super::prepare_physical_mount_source(conn, source)?;
    let kind = DataSourceRepo::new(conn).source_kind(source)?.to_string();
    let mut reader =
        crate::file_service::open_host_evidence_reader(&kind, &prepared.source_path, &case.0)
            .map_err(|error| MountServiceError::Reader(error.to_string()))?;
    crate::datasource_service::direct_ntfs_volume::probe(&mut reader)
        .map_err(MountServiceError::SourceMetadata)
}

/// Fail before the full container hash or workspace creation if no boot disk exists.
pub fn validate_emulation_disk_layout(
    conn: &Connection,
    case: &CaseId,
    source: &DataSourceId,
) -> Result<(), MountServiceError> {
    if direct_volume(conn, case, source)?.is_some() {
        return Err(MountServiceError::DirectVolumeBootLayoutRequired);
    }
    Ok(())
}
