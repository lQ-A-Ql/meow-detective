//! prepare for emulation sessions.
use super::*;
impl EmulationRegistry {
    pub fn prepare_session(
        &self,
        case_conn: &rusqlite::Connection,
        case_root: &Path,
        case_id: &CaseId,
        data_source_id: &DataSourceId,
        recovery_iso: Option<&Path>,
        options: VmOptions,
    ) -> Result<EmulationSessionStatus, EmulationRegistryError> {
        self.reject_duplicate(case_root, data_source_id)?;
        let recovery_media = recovery_iso
            .map(RecoveryMedia::open)
            .transpose()
            .map_err(|error| EmulationRegistryError::RecoveryMedia(error.to_string()))?;
        let prepared = prepare_emulation_source(case_conn, data_source_id)?;
        let guest = guest_profile_for_source(case_conn, case_root, case_id, data_source_id)?;
        let provider = open_block_provider(&prepared.source_path, image_kind(prepared.image_kind))?;
        let identity = ParentIdentity::new(provider.len(), prepared.parent_sha256)?;
        let session_id = format!("emulation-{}", uuid::Uuid::new_v4());
        let workspace = SessionWorkspace::create(case_root, &session_id)
            .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
        let (disk, firmware, backend) = match build_session_disk(&workspace, provider, &identity) {
            Ok(parts) => parts,
            Err(error) => {
                workspace.remove_best_effort();
                return Err(error);
            }
        };
        if let Err(error) = prepare_linux_network_if_needed(
            guest.is_linux,
            options,
            &disk,
            case_conn,
            case_root,
            case_id,
            data_source_id,
        ) {
            return Err(cleanup_prepare_failure(&workspace, &backend, error));
        }
        let maintenance = build_maintenance_for_guest(
            recovery_media.is_some(),
            guest.is_linux,
            case_conn,
            case_root,
            case_id,
            data_source_id,
        )
        .inspect_err(|_| {
            let _ = backend.stop();
            workspace.remove_best_effort();
        })?;
        let materials = prepare_machine_materials(
            &workspace,
            &identity,
            materials::MachineSpec {
                firmware,
                guest_os: &guest.guest_os,
                disk_adapter: guest.disk_adapter,
                disk_adapter_reason: &guest.disk_adapter_reason,
                network_pci_slot: guest.network_pci_slot,
            },
            ProvenanceIds {
                session_id: &session_id,
                case_id: &case_id.0,
                data_source_id: &data_source_id.0,
            },
            recovery_media.as_ref(),
            options,
            maintenance.as_ref(),
        );
        if let Err(error) = materials {
            let _ = backend.stop();
            workspace.remove_best_effort();
            return Err(error);
        }
        let status = EmulationSessionStatus {
            session_id: session_id.clone(),
            data_source_id: data_source_id.0.clone(),
            state: EmulationState::DescriptorReady,
            guest_phase: EmulationGuestPhase::Unknown,
            logical_length: identity.logical_length(),
            maintenance_media: maintenance.is_some(),
            error: None,
        };
        self.insert_entry(
            session_id,
            EmulationEntry {
                case_id: case_id.0.clone(),
                status: status.clone(),
                workspace,
                disk,
                backend: Some(backend),
                vmware: None,
                boot_started_at: None,
                op_lock: Arc::new(Mutex::new(())),
            },
        )?;
        Ok(status)
    }
}
fn build_session_disk(
    workspace: &SessionWorkspace,
    provider: Arc<dyn evidence_block::BlockProvider>,
    identity: &ParentIdentity,
) -> Result<(Arc<CowDisk>, VmwareFirmware, EmulationBackendHandle), EmulationRegistryError> {
    let disk = Arc::new(CowDisk::create(
        workspace.overlay_path(),
        provider,
        identity.clone(),
        CowDiskConfig::default(),
    )?);
    let firmware = detect_firmware(&disk)?;
    let backend =
        emulation_backend::start(Arc::clone(&disk), workspace.root(), workspace.mount_point())
            .map_err(|error| EmulationRegistryError::Backend(error.to_string()))?;
    Ok((disk, firmware, backend))
}
