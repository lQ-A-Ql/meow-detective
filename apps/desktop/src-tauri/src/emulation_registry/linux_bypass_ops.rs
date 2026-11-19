//! Linux bypass inspection and appliance-verdict operations.
//!
//! Split from `session_ops.rs` to keep that module under the 500-line limit.
//! The Appliance verdict stages the self-authored payload CD (never the guest
//! OS itself) or reports a structured `Unavailable` result; this path must
//! never call `disk.invalidate()`.

use std::path::Path;

use app_services::emulation_linux_bypass::LinuxBypassPlan;
use domain::DataSourceId;
use transport::dto::{
    EmulationLinuxBypassAlternativeDto, EmulationLinuxBypassBlockerDto,
    EmulationLinuxBypassInspectionDto, EmulationLinuxBypassMethodDto,
    EmulationLinuxBypassStatusDto,
};

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
        inspection.appliance_available = self.rescue_iso_available(session_id);
        Ok(inspection)
    }

    pub fn apply_linux_bypass(
        &self,
        case: &BypassCaseRef<'_>,
        session_id: &str,
        partition_index: u32,
        username: &str,
    ) -> Result<transport::dto::EmulationLinuxBypassResultDto, EmulationRegistryError> {
        let session = self.edit_session_entry(
            session_id,
            "bypass edits are only allowed before the guest is launched",
        )?;
        let _op_guard = session.op_lock.lock().map_err(|_| Self::lock_error())?;
        self.require_state(session_id, EmulationState::DescriptorReady)?;
        let data_source_id = session.data_source_id.clone();
        let ctx = app_services::emulation_bypass::BypassCaseContext {
            case_conn: case.case_conn,
            case_root: case.case_root,
            case_id: case.case_id,
            data_source_id: &DataSourceId(data_source_id.clone()),
        };
        // Plan first: read-only, never invalidates the overlay. Only the
        // HostInPlace verdict may write; Appliance/Unavailable verdicts return
        // a structured `Unavailable` result instead of raising.
        let plan = app_services::emulation_linux_bypass::plan_linux_bypass(
            &session.disk,
            &ctx,
            partition_index,
            username,
        )?;
        match plan {
            LinuxBypassPlan::Appliance {
                edited, blocker, ..
            } => self.appliance_verdict(
                session_id,
                &data_source_id,
                partition_index,
                username,
                &edited,
                blocker,
            ),
            _ => {
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
    }

    /// Read-only probe: the session has a configured, readable rescue ISO.
    fn rescue_iso_available(&self, session_id: &str) -> bool {
        let Ok(entries) = self.entries.lock() else {
            return false;
        };
        entries
            .get(session_id)
            .and_then(|entry| entry.vmx_blueprint.as_ref())
            .and_then(|blueprint| blueprint.recovery_iso())
            .and_then(|iso| super::linux_rescue::rescue_iso_for_session(Some(Path::new(iso))))
            .is_some()
    }

    /// Appliance verdict: stage the self-authored payload CD when a rescue ISO
    /// is available, otherwise report the blocker without touching the disk.
    /// No `disk.invalidate()` may occur on this path.
    fn appliance_verdict(
        &self,
        session_id: &str,
        data_source_id: &str,
        partition_index: u32,
        username: &str,
        edited: &str,
        blocker: EmulationLinuxBypassBlockerDto,
    ) -> Result<transport::dto::EmulationLinuxBypassResultDto, EmulationRegistryError> {
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get_mut(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        let blueprint = entry.vmx_blueprint.clone();
        let rescue_iso = blueprint
            .as_ref()
            .and_then(|blueprint| blueprint.recovery_iso())
            .and_then(|iso| super::linux_rescue::rescue_iso_for_session(Some(Path::new(iso))));

        let (payload_staged, alternatives) = match (blueprint.as_ref(), rescue_iso) {
            (Some(blueprint), Some(_rescue)) => {
                // Appliance verdicts originate from the XFS assessment only;
                // ext4 always returns a HostInPlace plan.
                let input = super::linux_rescue::RescuePayloadInput {
                    partition_index,
                    filesystem: "xfs",
                    username,
                    blocker,
                    edited,
                };
                super::linux_rescue::stage_rescue_payload(&entry.workspace, blueprint, &input)?;
                (
                    true,
                    vec![EmulationLinuxBypassAlternativeDto::ExternalRescueIso],
                )
            }
            _ => (
                false,
                vec![
                    EmulationLinuxBypassAlternativeDto::ExternalRescueIso,
                    EmulationLinuxBypassAlternativeDto::LaunchWithoutBypass,
                ],
            ),
        };

        Ok(transport::dto::EmulationLinuxBypassResultDto {
            session_id: session_id.to_string(),
            data_source_id: data_source_id.to_string(),
            partition_index,
            username: username.to_string(),
            password_set: false,
            already_configured: false,
            status: EmulationLinuxBypassStatusDto::Unavailable,
            method: EmulationLinuxBypassMethodDto::LinuxAppliance,
            reason: Some(blocker),
            alternatives,
            payload_staged,
        })
    }
}
