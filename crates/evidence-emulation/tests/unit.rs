use std::sync::Arc;

use evidence_block::{BlockDeviceError, BlockProvider};
use evidence_emulation::{
    CowDisk, CowDiskConfig, EmulationError, ParentIdentity, VmdkAdapter, VmdkDescriptor,
    VmwareFirmware, VmxConfig,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

const DISK_LENGTH: usize = 256 * 1024;

struct MemoryProvider(Vec<u8>);

impl BlockProvider for MemoryProvider {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        let start = offset as usize;
        let end = start + buffer.len();
        buffer.copy_from_slice(&self.0[start..end]);
        Ok(())
    }
}

fn fixture() -> (Arc<dyn BlockProvider>, ParentIdentity, Vec<u8>) {
    let bytes: Vec<u8> = (0..DISK_LENGTH).map(|index| (index % 251) as u8).collect();
    let hash: [u8; 32] = Sha256::digest(&bytes).into();
    let identity = ParentIdentity::new(bytes.len() as u64, hash).unwrap();
    (Arc::new(MemoryProvider(bytes.clone())), identity, bytes)
}

#[test]
fn cow_disk_preserves_parent_and_recovers_committed_cross_cluster_write() {
    let directory = tempdir().unwrap();
    let overlay = directory.path().join("overlay.cow");
    let (parent, identity, original) = fixture();
    let config = CowDiskConfig {
        cluster_size: 4096,
        max_write_length: 64 * 1024,
    };
    let disk = CowDisk::create(&overlay, Arc::clone(&parent), identity.clone(), config).unwrap();
    let patch = vec![0xa5; 6000];
    disk.write_all_at(3500, &patch).unwrap();
    drop(disk);

    let reopened = CowDisk::open(&overlay, parent, identity, config).unwrap();
    let mut actual = vec![0; 10_000];
    reopened.read_exact_at(0, &mut actual).unwrap();
    let mut expected = original[..10_000].to_vec();
    expected[3500..9500].copy_from_slice(&patch);
    assert_eq!(actual, expected);
}

#[test]
fn recovery_discards_complete_records_beyond_committed_superblock() {
    let directory = tempdir().unwrap();
    let overlay = directory.path().join("overlay.cow");
    let (parent, identity, original) = fixture();
    let config = CowDiskConfig {
        cluster_size: 4096,
        max_write_length: 64 * 1024,
    };
    let disk = CowDisk::create(&overlay, Arc::clone(&parent), identity.clone(), config).unwrap();
    disk.write_all_at(0, &[0x11; 512]).unwrap();
    drop(disk);
    let committed_length = std::fs::metadata(&overlay).unwrap().len();
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&overlay)
        .unwrap();
    file.write_all(&[0xcc; 8192]).unwrap();
    drop(file);

    let reopened = CowDisk::open(&overlay, parent, identity, config).unwrap();
    assert_eq!(std::fs::metadata(&overlay).unwrap().len(), committed_length);
    let mut actual = vec![0; 1024];
    reopened.read_exact_at(0, &mut actual).unwrap();
    assert_eq!(&actual[..512], &[0x11; 512]);
    assert_eq!(&actual[512..], &original[512..1024]);
}

#[test]
fn parent_identity_mismatch_is_rejected() {
    let directory = tempdir().unwrap();
    let overlay = directory.path().join("overlay.cow");
    let (parent, identity, _) = fixture();
    CowDisk::create(
        &overlay,
        Arc::clone(&parent),
        identity,
        CowDiskConfig::default(),
    )
    .unwrap();
    let wrong = ParentIdentity::new(parent.len(), [7; 32]).unwrap();
    let error = match CowDisk::open(&overlay, parent, wrong, CowDiskConfig::default()) {
        Ok(_) => panic!("mismatched parent identity was accepted"),
        Err(error) => error,
    };
    assert!(matches!(error, EmulationError::ParentMismatch));
}

#[test]
fn cow_disk_rejects_oversized_and_out_of_bounds_requests() {
    let directory = tempdir().unwrap();
    let (parent, identity, _) = fixture();
    let config = CowDiskConfig {
        cluster_size: 4096,
        max_write_length: 512,
    };
    let disk = CowDisk::create(
        &directory.path().join("overlay.cow"),
        parent,
        identity,
        config,
    )
    .unwrap();

    assert!(matches!(
        disk.write_all_at(0, &[0; 513]),
        Err(EmulationError::WriteTooLarge { .. })
    ));
    assert!(matches!(
        disk.write_all_at(disk.len() - 256, &[0; 512]),
        Err(EmulationError::OutOfBounds { .. })
    ));
    let mut read = [0u8; 512];
    assert!(matches!(
        disk.read_exact_at(disk.len() - 256, &mut read),
        Err(EmulationError::OutOfBounds { .. })
    ));
}

#[test]
fn vmdk_descriptor_uses_exact_sector_count_and_rejects_escaping_paths() {
    let (_, identity, _) = fixture();
    let descriptor =
        VmdkDescriptor::new(&identity, "extent/disk.raw", VmdkAdapter::LsiLogic).unwrap();
    let rendered = descriptor.render();
    assert_eq!(descriptor.sector_count(), DISK_LENGTH as u64 / 512);
    assert!(rendered.contains("RW 512 FLAT \"extent\\disk.raw\" 0"));
    assert!(rendered.contains("ddb.adapterType = \"lsilogic\""));
    assert_eq!(VmdkDescriptor::parse(&rendered).unwrap(), descriptor);
    assert!(VmdkDescriptor::new(&identity, "../disk.raw", VmdkAdapter::Ide).is_err());
    assert!(VmdkDescriptor::new(&identity, "C:\\disk.raw", VmdkAdapter::Ide).is_err());
}

#[test]
fn vmx_disables_host_integrations_and_networking() {
    let config = VmxConfig::new("disk.vmdk", VmwareFirmware::Efi).unwrap();
    let rendered = config.render();

    VmxConfig::validate_rendered(&rendered).unwrap();
    assert!(rendered.contains("ethernet0.present = \"FALSE\""));
    assert!(rendered.contains("sharedFolder.maxNum = \"0\""));
    assert!(rendered.contains("usb.present = \"FALSE\""));
    assert!(rendered.contains("floppy0.present = \"FALSE\""));
    assert!(rendered.contains("firmware = \"efi\""));
    assert!(rendered.contains("bios.bootOrder = \"hdd\""));
    assert!(VmxConfig::new("..\\disk.vmdk", VmwareFirmware::Bios).is_err());
}

#[test]
fn vmx_can_boot_a_user_selected_winpe_iso_before_the_evidence_disk() {
    let rendered = VmxConfig::new("disk.vmdk", VmwareFirmware::Efi)
        .unwrap()
        .with_recovery_iso(r"C:\Recovery\WinPE.iso")
        .unwrap()
        .render();

    VmxConfig::validate_rendered(&rendered).unwrap();
    assert!(rendered.contains("bios.bootOrder = \"cdrom,hdd\""));
    assert!(rendered.contains(r#"ide1:0.fileName = "C:\Recovery\WinPE.iso""#));
    assert!(rendered.contains("ide1:0.deviceType = \"cdrom-image\""));
    assert!(VmxConfig::new("disk.vmdk", VmwareFirmware::Efi)
        .unwrap()
        .with_recovery_iso(r"..\WinPE.iso")
        .is_err());
}

#[test]
fn vmdk_parser_rejects_modified_extent_geometry_and_paths() {
    let (_, identity, _) = fixture();
    let rendered = VmdkDescriptor::new(&identity, "mount/disk.raw", VmdkAdapter::LsiLogic)
        .unwrap()
        .render();

    assert!(VmdkDescriptor::parse(&rendered.replace("RW 512", "RW 0")).is_err());
    assert!(VmdkDescriptor::parse(&rendered.replace("mount\\disk.raw", "..\\disk.raw")).is_err());
    assert!(VmdkDescriptor::parse(&format!("{rendered}RW 1 FLAT \"extra.raw\" 0\n")).is_err());
}

#[test]
fn vmx_validator_rejects_networking_and_missing_isolation_controls() {
    let rendered = VmxConfig::new("disk.vmdk", VmwareFirmware::Bios)
        .unwrap()
        .render();

    assert!(VmxConfig::validate_rendered(&rendered.replace(
        "ethernet0.present = \"FALSE\"",
        "ethernet0.present = \"TRUE\""
    ))
    .is_err());
    assert!(VmxConfig::validate_rendered(
        &rendered.replace("isolation.tools.copy.disable = \"TRUE\"\n", "")
    )
    .is_err());
    assert!(VmxConfig::validate_rendered(
        &rendered.replace("bios.bootOrder = \"hdd\"", "bios.bootOrder = \"cdrom,hdd\"")
    )
    .is_err());
}
