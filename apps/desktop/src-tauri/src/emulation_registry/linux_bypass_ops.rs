//! Client-side Linux bypass operations bound to the prepared session.

use domain::DataSourceId;
use transport::dto::{EmulationLinuxBypassInspectionDto, EmulationLinuxBypassResultDto};

use super::{BypassCaseRef, EmulationRegistry, EmulationRegistryError, EmulationState};

impl EmulationRegistry {
    pub fn inspect_linux_bypass(
        &self,
        case: &BypassCaseRef<'_>,
        session_id: &str,
        partition_index: u32,
    ) -> Result<EmulationLinuxBypassInspectionDto, EmulationRegistryError> {
        let session = self.edit_session_entry(
            session_id,
            "bypass inspection is only allowed before the guest is launched",
        )?;
        let _op_guard = session.op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let ctx = app_services::emulation_bypass::BypassCaseContext {
            case_conn: case.case_conn,
            case_root: case.case_root,
            case_id: case.case_id,
            data_source_id: &DataSourceId(session.data_source_id),
        };
        let mut inspection =
            app_services::emulation_linux_bypass::inspect_linux_bypass(&ctx, partition_index)?;
        inspection.session_id = session_id.to_string();
        Ok(inspection)
    }

    pub fn apply_linux_bypass(
        &self,
        case: &BypassCaseRef<'_>,
        session_id: &str,
        partition_index: u32,
        username: &str,
    ) -> Result<EmulationLinuxBypassResultDto, EmulationRegistryError> {
        let session = self.edit_session_entry(
            session_id,
            "bypass edits are only allowed before the guest is launched",
        )?;
        let _op_guard = session.op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let ctx = app_services::emulation_bypass::BypassCaseContext {
            case_conn: case.case_conn,
            case_root: case.case_root,
            case_id: case.case_id,
            data_source_id: &DataSourceId(session.data_source_id),
        };
        let mut result = app_services::emulation_linux_bypass::apply_linux_bypass(
            &session.disk,
            &ctx,
            partition_index,
            username,
        )?;
        result.session_id = session_id.to_string();
        Ok(result)
    }
}
