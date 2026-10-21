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
        let recovery_media = open_recovery_media(recovery_iso)?;
        let parent = prepare_parent_with_timing(
            case_conn,
            case_id,
            data_source_id,
            recovery_media.is_some(),
        )?;
        let guest = guest_profile_for_source(case_conn, case_root, case_id, data_source_id)?;
        let session_id = format!("emulation-{}", uuid::Uuid::new_v4());
        let workspace = SessionWorkspace::create(case_root, &session_id)
            .map_err(|error| EmulationRegistryError::Workspace(error.to_string()))?;
        let (disk, firmware, backend) = match build_session_disk(&workspace, &parent) {
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
            parent.source_volume.is_some(),
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
            &parent.identity,
            materials::MachineSpec {
                firmware,
                guest_os: &guest.guest_os,
                disk_adapter: guest.disk_adapter,
                disk_adapter_reason: &guest.disk_adapter_reason,
                network_pci_slot: guest.network_pci_slot,
                derived_ntfs: parent.derived_ntfs.as_ref(),
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
        let status = descriptor_ready_status(
            &session_id,
            data_source_id,
            parent.identity.logical_length(),
            maintenance.is_some(),
        );
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

fn prepare_parent_with_timing(
    case_conn: &rusqlite::Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    recovery_media_available: bool,
) -> Result<app_services::mount_service::PreparedEmulationParent, EmulationRegistryError> {
    let started_at = std::time::Instant::now();
    let parent = app_services::mount_service::prepare_emulation_parent(
        case_conn,
        case_id,
        data_source_id,
        recovery_media_available,
    )?;
    tracing::info!(
        data_source_id = %data_source_id.0,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        "emulation parent preparation completed"
    );
    Ok(parent)
}

fn open_recovery_media(
    path: Option<&Path>,
) -> Result<Option<RecoveryMedia>, EmulationRegistryError> {
    path.map(RecoveryMedia::open)
        .transpose()
        .map_err(|error| EmulationRegistryError::RecoveryMedia(error.to_string()))
}

fn build_session_disk(
    workspace: &SessionWorkspace,
    parent: &app_services::mount_service::PreparedEmulationParent,
) -> Result<(Arc<CowDisk>, VmwareFirmware, EmulationBackendHandle), EmulationRegistryError> {
    let disk = Arc::new(CowDisk::create(
        workspace.overlay_path(),
        Arc::clone(&parent.provider),
        parent.identity.clone(),
        CowDiskConfig {
            source_volume: parent.source_volume,
            ..Default::default()
        },
    )?);
    if let Some(mapping) = parent.source_volume {
        mapping.seed_guest_boot_parameters(&disk)?;
    }
    let firmware = detect_firmware(&disk)?;
    let backend =
        emulation_backend::start(Arc::clone(&disk), workspace.root(), workspace.mount_point())
            .map_err(|error| EmulationRegistryError::Backend(error.to_string()))?;
    Ok((disk, firmware, backend))
}

fn descriptor_ready_status(
    session_id: &str,
    source: &DataSourceId,
    length: u64,
    maintenance: bool,
) -> EmulationSessionStatus {
    EmulationSessionStatus {
        session_id: session_id.to_string(),
        data_source_id: source.0.clone(),
        state: EmulationState::DescriptorReady,
        guest_phase: EmulationGuestPhase::Unknown,
        logical_length: length,
        maintenance_media: maintenance,
        error: None,
    }
}
