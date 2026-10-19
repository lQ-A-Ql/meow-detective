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

/// Select a derived volume only when validated recovery media is available.
pub fn prepare_emulation_disk_layout(
    conn: &Connection,
    case: &CaseId,
    source: &DataSourceId,
    recovery_media_available: bool,
) -> Result<Option<DirectNtfsVolumeDto>, MountServiceError> {
    let geometry = direct_volume(conn, case, source)?;
    if let Some(volume) = &geometry {
        if !recovery_media_available {
            return Err(MountServiceError::DirectVolumeRecoveryRequired);
        }
        if volume.bytes_per_sector != 512
            || volume
                .verified_extent_bytes
                .is_none_or(|length| length == 0 || length / 512 > u64::from(u32::MAX) - 2048)
        {
            return Err(MountServiceError::Unsupported(
                "derived NTFS boot disks require a verified 512-byte MBR-sized volume".into(),
            ));
        }
    }
    Ok(geometry)
}
