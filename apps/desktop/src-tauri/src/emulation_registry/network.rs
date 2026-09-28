use std::path::Path;
use std::sync::Arc;

use app_services::emulation_bypass::BypassCaseContext;
use domain::{CaseId, DataSourceId};
use evidence_emulation::{CowDisk, VmNetworkMode, VmOptions};

use super::EmulationRegistryError;

pub(super) fn cleanup_prepare_failure(
    workspace: &super::workspace::SessionWorkspace,
    backend: &crate::emulation_backend::EmulationBackendHandle,
    error: EmulationRegistryError,
) -> EmulationRegistryError {
    let _ = backend.stop();
    workspace.remove_best_effort();
    error
}

pub(super) fn build_maintenance_for_guest(
    requested: bool,
    linux: bool,
    conn: &rusqlite::Connection,
    root: &Path,
    case_id: &CaseId,
    source_id: &DataSourceId,
) -> Result<Option<super::materials::MaintenancePayload>, EmulationRegistryError> {
    if !requested {
        return Ok(None);
    }
    if linux {
        super::materials::build_linux_rescue_payload(conn, root, case_id, source_id)
    } else {
        super::materials::build_maintenance_payload(conn, root, case_id, source_id)
    }
}

pub(super) fn prepare_linux_network_if_needed(
    is_linux: bool,
    options: VmOptions,
    disk: &Arc<CowDisk>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<(), EmulationRegistryError> {
    if is_linux && options.network_mode != VmNetworkMode::Off {
        app_services::emulation_linux_bypass::prepare_network_for_emulation(
            disk,
            &BypassCaseContext {
                case_conn,
                case_root,
                case_id,
                data_source_id,
            },
        )?;
    }
    Ok(())
}
