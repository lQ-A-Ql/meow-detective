use std::path::Path;

use domain::DataSourceId;

use super::EmulationRegistryError;

/// Resolved VM identity and storage-controller policy for one source.
pub(super) struct GuestProfile {
    pub(super) is_linux: bool,
    pub(super) guest_os: String,
    pub(super) disk_adapter: evidence_emulation::VmdkAdapter,
    pub(super) disk_adapter_reason: String,
}

/// Linux derives both the guestid and storage controller from read-only
/// evidence probes. Windows keeps the inbox IDE path and windows9-64 profile.
pub(super) fn guest_profile_for_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    data_source_id: &DataSourceId,
) -> Result<GuestProfile, EmulationRegistryError> {
    let is_linux =
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(case_conn)
            .find_storage(data_source_id)
            .map_err(app_services::mount_service::MountServiceError::from)?
            .map(|storage| storage.platform == "linux")
            .unwrap_or(false);
    if is_linux {
        let profile = app_services::mount_service::linux_guest_profile(
            case_conn,
            case_root,
            case_id,
            data_source_id,
        )?;
        Ok(GuestProfile {
            is_linux: true,
            guest_os: profile.guest_os,
            disk_adapter: profile.disk_adapter,
            disk_adapter_reason: profile.disk_adapter_reason,
        })
    } else {
        Ok(GuestProfile {
            is_linux: false,
            guest_os: "windows9-64".to_string(),
            disk_adapter: evidence_emulation::VmdkAdapter::Ide,
            disk_adapter_reason: "Windows guest uses the inbox IDE controller".to_string(),
        })
    }
}
