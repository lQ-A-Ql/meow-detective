use std::sync::Arc;

use evidence_block::{BlockDeviceError, BlockProvider};
use evidence_emulation::{CowDisk, CowDiskConfig, ParentIdentity};
use sha2::{Digest, Sha256};

use super::{detect_firmware, EmulationState};

struct MemoryProvider(Vec<u8>);

impl BlockProvider for MemoryProvider {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        let start = offset as usize;
        buffer.copy_from_slice(&self.0[start..start + buffer.len()]);
        Ok(())
    }
}

#[test]
fn firmware_detection_uses_the_primary_gpt_header() {
    let directory = tempfile::tempdir().unwrap();
    let mut bytes = vec![0u8; 4096];
    bytes[512..520].copy_from_slice(b"EFI PART");
    let provider: Arc<dyn BlockProvider> = Arc::new(MemoryProvider(bytes));
    let identity = ParentIdentity::new(provider.len(), [1; 32]).unwrap();
    let disk = CowDisk::create(
        &directory.path().join("overlay.cow"),
        provider,
        identity,
        CowDiskConfig::default(),
    )
    .unwrap();

    assert_eq!(
        detect_firmware(&disk).unwrap(),
        evidence_emulation::VmwareFirmware::Efi
    );
    assert_ne!(EmulationState::DescriptorReady, EmulationState::Running);
}

#[test]
#[ignore = "requires FORENSICS_EMULATION_E01_FIXTURE and an installed Dokany driver"]
fn real_e01_mount_uses_a_descriptor_and_sparse_cow_without_materializing_the_disk() {
    use std::io::{Read, Seek, SeekFrom, Write};

    let source = std::env::var_os("FORENSICS_EMULATION_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_EMULATION_E01_FIXTURE");
    let physical_length = std::fs::metadata(&source).unwrap().len();
    let provider =
        evidence_block::open_block_provider(&source, evidence_block::EvidenceImageKind::E01)
            .unwrap();
    let identity = sampled_test_identity(&provider, physical_length);
    let case = tempfile::tempdir().unwrap();
    let session_id = format!("emulation-{}", uuid::Uuid::new_v4());
    let workspace = super::workspace::SessionWorkspace::create(case.path(), &session_id).unwrap();
    let disk = Arc::new(
        CowDisk::create(
            workspace.overlay_path(),
            Arc::clone(&provider),
            identity.clone(),
            CowDiskConfig::default(),
        )
        .unwrap(),
    );
    let backend = crate::emulation_backend::start(
        Arc::clone(&disk),
        workspace.root(),
        workspace.mount_point(),
    )
    .unwrap();
    super::prepare_machine_materials(
        &workspace,
        &identity,
        super::detect_firmware(&disk).unwrap(),
        super::workspace::ProvenanceIds {
            session_id: &session_id,
            case_id: "poc-case",
            data_source_id: "poc-source",
        },
        None,
        evidence_emulation::VmOptions::default(),
        None,
    )
    .unwrap();

    let extent = workspace.mount_point().join("disk.raw");
    let descriptor_length = std::fs::metadata(workspace.root().join("disk.vmdk"))
        .unwrap()
        .len();
    let overlay_before = std::fs::metadata(workspace.overlay_path()).unwrap().len();
    assert_eq!(std::fs::metadata(&extent).unwrap().len(), provider.len());
    assert!(descriptor_length < 4096);
    assert!(overlay_before < 1024 * 1024);

    let offset = 4 * 1024 * 1024u64;
    let mut original = [0u8; 512];
    provider.read_exact_at(offset, &mut original).unwrap();
    let replacement = [0xa5u8; 512];
    let mut mounted = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&extent)
        .unwrap();
    mounted.seek(SeekFrom::Start(offset)).unwrap();
    mounted.write_all(&replacement).unwrap();
    mounted.flush().unwrap();
    mounted.seek(SeekFrom::Start(offset)).unwrap();
    let mut through_mount = [0u8; 512];
    mounted.read_exact(&mut through_mount).unwrap();
    assert_eq!(through_mount, replacement);
    let mut parent_after = [0u8; 512];
    provider.read_exact_at(offset, &mut parent_after).unwrap();
    assert_eq!(parent_after, original);
    assert!(std::fs::metadata(workspace.overlay_path()).unwrap().len() < 1024 * 1024);

    drop(mounted);
    backend.stop().unwrap();
    eprintln!(
        "logical={} physical_e01={} descriptor={} overlay_before={}",
        provider.len(),
        physical_length,
        descriptor_length,
        overlay_before
    );
}

#[test]
#[ignore = "requires FORENSICS_EMULATION_E01_FIXTURE, Dokany, VMware, and elevation"]
fn real_e01_launches_vmware_from_the_sparse_mounted_extent() {
    let source = std::env::var_os("FORENSICS_EMULATION_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_EMULATION_E01_FIXTURE");
    let physical_length = std::fs::metadata(&source).unwrap().len();
    let provider =
        evidence_block::open_block_provider(&source, evidence_block::EvidenceImageKind::E01)
            .unwrap();
    let identity = sampled_test_identity(&provider, physical_length);
    let recovery_media = std::env::var_os("FORENSICS_EMULATION_WINPE_ISO")
        .map(std::path::PathBuf::from)
        .map(|path| super::recovery_media::RecoveryMedia::open(&path).unwrap());
    let case = tempfile::tempdir().unwrap();
    let session_id = format!("emulation-{}", uuid::Uuid::new_v4());
    let workspace = super::workspace::SessionWorkspace::create(case.path(), &session_id).unwrap();
    let disk = Arc::new(
        CowDisk::create(
            workspace.overlay_path(),
            Arc::clone(&provider),
            identity.clone(),
            CowDiskConfig::default(),
        )
        .unwrap(),
    );
    let backend = crate::emulation_backend::start(
        Arc::clone(&disk),
        workspace.root(),
        workspace.mount_point(),
    )
    .unwrap();
    super::prepare_machine_materials(
        &workspace,
        &identity,
        super::detect_firmware(&disk).unwrap(),
        super::workspace::ProvenanceIds {
            session_id: &session_id,
            case_id: "poc-case",
            data_source_id: "poc-source",
        },
        recovery_media.as_ref(),
        evidence_emulation::VmOptions::default(),
        None,
    )
    .unwrap();

    let descriptor_length = std::fs::metadata(workspace.root().join("disk.vmdk"))
        .unwrap()
        .len();
    let descriptor = std::fs::read_to_string(workspace.root().join("disk.vmdk")).unwrap();
    let machine = std::fs::read_to_string(workspace.vmx_path()).unwrap();
    assert!(descriptor.contains("ddb.adapterType = \"ide\""));
    assert!(machine.contains("ide0:0.fileName = \"disk.vmdk\""));
    assert!(!machine.contains("scsi0:0.fileName"));
    let mut vmware = VmwarePocGuard::new(super::vmware::launch(workspace.vmx_path()).unwrap());
    let wait_seconds = std::env::var("FORENSICS_EMULATION_BOOT_WAIT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(20);
    std::thread::sleep(std::time::Duration::from_secs(wait_seconds));
    assert!(vmware.control().is_running().unwrap());
    let vmware_log = workspace.root().join("vmware.log");
    let log = std::fs::read_to_string(&vmware_log).unwrap();
    if let Some(output) = std::env::var_os("FORENSICS_EMULATION_VMWARE_LOG") {
        std::fs::copy(&vmware_log, output).unwrap();
    }
    assert!(log.len() > 1024);
    assert!(log.lines().count() > 10);
    assert!(log.contains("ide0:0"));
    if recovery_media.is_some() {
        assert!(log.contains("ide1:0"));
        assert!(log.to_ascii_lowercase().contains("cdrom"));
    }
    vmware.stop();
    disk.flush().unwrap();
    backend.stop().unwrap();

    let overlay_length = std::fs::metadata(workspace.overlay_path()).unwrap().len();
    assert!(descriptor_length < 4096);
    assert!(overlay_length < provider.len() / 100);
    assert_eq!(std::fs::metadata(&source).unwrap().len(), physical_length);
    eprintln!(
        "vmware_sparse_poc logical={} physical_e01={} descriptor={} overlay_after_boot={} vmware_log={}",
        provider.len(),
        physical_length,
        descriptor_length,
        overlay_length,
        log.len()
    );
}

struct VmwarePocGuard(Option<super::vmware::VmwareControl>);

impl VmwarePocGuard {
    fn new(control: super::vmware::VmwareControl) -> Self {
        Self(Some(control))
    }

    fn control(&self) -> &super::vmware::VmwareControl {
        self.0.as_ref().expect("VMware POC control is active")
    }

    fn stop(&mut self) {
        if let Some(control) = self.0.take() {
            control.stop_bounded().unwrap();
        }
    }
}

impl Drop for VmwarePocGuard {
    fn drop(&mut self) {
        if let Some(control) = self.0.take() {
            let _ = control.stop_hard();
        }
    }
}

fn sampled_test_identity(
    provider: &Arc<dyn BlockProvider>,
    physical_length: u64,
) -> ParentIdentity {
    const SAMPLE_LENGTH: usize = 1024 * 1024;
    let mut first = vec![0u8; SAMPLE_LENGTH];
    provider.read_exact_at(0, &mut first).unwrap();
    let tail_offset = provider.len().saturating_sub(SAMPLE_LENGTH as u64);
    let mut last = vec![0u8; SAMPLE_LENGTH.min(provider.len() as usize)];
    provider.read_exact_at(tail_offset, &mut last).unwrap();
    let mut digest = Sha256::new();
    digest.update(b"emulation-poc-sampled-parent-v1");
    digest.update(provider.len().to_le_bytes());
    digest.update(physical_length.to_le_bytes());
    digest.update(first);
    digest.update(last);
    ParentIdentity::new(provider.len(), digest.finalize().into()).unwrap()
}
