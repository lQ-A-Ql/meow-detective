use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use evidence_block::{BlockDeviceError, BlockProvider};
use evidence_emulation::{
    CowDisk, CowDiskConfig, EmulationError, ParentIdentity, VmOptions, VmdkAdapter, VmdkDescriptor,
    VmwareFirmware, VmxConfig,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

const DISK_LENGTH: usize = 256 * 1024;

fn validate_default(value: &str) -> Result<(), EmulationError> {
    VmxConfig::validate_rendered(value, VmOptions::default(), false)
}

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

struct CountingProvider {
    bytes: Vec<u8>,
    reads: Arc<AtomicU64>,
}

impl BlockProvider for CountingProvider {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        let start = offset as usize;
        buffer.copy_from_slice(&self.bytes[start..start + buffer.len()]);
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
fn cow_disk_preserves_parent_and_applies_committed_cross_cluster_write() {
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

    let mut actual = vec![0; 10_000];
    disk.read_exact_at(0, &mut actual).unwrap();
    let mut expected = original[..10_000].to_vec();
    expected[3500..9500].copy_from_slice(&patch);
    assert_eq!(actual, expected);
}

#[test]
fn cow_disk_cluster_cache_serves_repeated_reads_and_tracks_writes() {
    let directory = tempdir().unwrap();
    let bytes = vec![0x31; DISK_LENGTH];
    let reads = Arc::new(AtomicU64::new(0));
    let provider: Arc<dyn BlockProvider> = Arc::new(CountingProvider {
        bytes,
        reads: Arc::clone(&reads),
    });
    let identity = ParentIdentity::new(provider.len(), [0x42; 32]).unwrap();
    let config = CowDiskConfig {
        cluster_size: 4096,
        max_write_length: 64 * 1024,
    };
    let disk = CowDisk::create(
        &directory.path().join("overlay.cow"),
        provider,
        identity,
        config,
    )
    .unwrap();
    let mut actual = [0u8; 512];

    disk.read_exact_at(0, &mut actual).unwrap();
    disk.read_exact_at(512, &mut actual).unwrap();
    assert_eq!(reads.load(Ordering::Relaxed), 1);

    disk.write_all_at(1024, &[0xa5; 512]).unwrap();
    disk.read_exact_at(1024, &mut actual).unwrap();
    assert_eq!(actual, [0xa5; 512]);
    assert_eq!(reads.load(Ordering::Relaxed), 1);
}

#[test]
fn parent_identity_mismatch_is_rejected() {
    let directory = tempdir().unwrap();
    let overlay = directory.path().join("overlay.cow");
    let (parent, identity, _) = fixture();
    let wrong = ParentIdentity::new(identity.logical_length() * 2, [7; 32]).unwrap();
    let error = match CowDisk::create(&overlay, parent, wrong, CowDiskConfig::default()) {
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
fn vmdk_descriptor_round_trips_the_ide_adapter() {
    let (_, identity, _) = fixture();
    let descriptor = VmdkDescriptor::new(&identity, "mount/disk.raw", VmdkAdapter::Ide).unwrap();
    let rendered = descriptor.render();

    assert!(rendered.contains("ddb.adapterType = \"ide\""));
    assert_eq!(VmdkDescriptor::parse(&rendered).unwrap(), descriptor);
}

#[test]
fn vmx_disables_host_integrations_and_networking() {
    let config = VmxConfig::new("disk.vmdk", VmwareFirmware::Efi).unwrap();
    let rendered = config.render();

    validate_default(&rendered).unwrap();
    assert!(rendered.contains("ethernet0.present = \"FALSE\""));
    assert!(rendered.contains("sharedFolder.maxNum = \"0\""));
    assert!(rendered.contains("usb.present = \"FALSE\""));
    assert!(rendered.contains("floppy0.present = \"FALSE\""));
    assert!(rendered.contains("firmware = \"efi\""));
    assert!(rendered.contains("ide0:0.deviceType = \"disk\""));
    assert!(!rendered.contains("scsi0:0.fileName"));
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

    validate_default(&rendered).unwrap();
    assert!(rendered.contains("bios.bootOrder = \"cdrom,hdd\""));
    assert!(rendered.contains("ide0:0.deviceType = \"disk\""));
    assert!(rendered.contains(r#"ide0:0.fileName = "disk.vmdk""#));
    assert!(!rendered.contains("scsi0:0.fileName"));
    assert!(rendered.contains(r#"ide1:0.fileName = "C:\Recovery\WinPE.iso""#));
    assert!(rendered.contains("ide1:0.deviceType = \"cdrom-image\""));
    assert!(VmxConfig::new("disk.vmdk", VmwareFirmware::Efi)
        .unwrap()
        .with_recovery_iso(r"..\WinPE.iso")
        .is_err());
}

#[test]
fn iso9660_image_carries_payloads_with_a_stable_layout() {
    use evidence_emulation::{build_iso, IsoFile};

    let tool = vec![0x4du8; 5000];
    let targets = br#"{"installs":[],"recommendedBootRoute":"recoveryMedia"}"#.to_vec();
    let files = [
        IsoFile {
            name: "MEOWMTN.EXE",
            data: &tool,
        },
        IsoFile {
            name: "TARGETS.JSON",
            data: &targets,
        },
    ];
    let image = build_iso(&files).unwrap();
    let again = build_iso(&files).unwrap();
    assert_eq!(image, again, "ISO output must be deterministic");
    assert_eq!(image.len() % 2048, 0);

    let pvd = &image[16 * 2048..17 * 2048];
    assert_eq!(pvd[0], 1);
    assert_eq!(&pvd[1..6], b"CD001");
    let terminator = &image[17 * 2048..18 * 2048];
    assert_eq!(terminator[0], 255);

    let root = &image[20 * 2048..21 * 2048];
    let entries = parse_iso_root(root);
    assert_eq!(entries.len(), 2);
    let (tool_extent, tool_length) = entries["MEOWMTN.EXE;1"];
    let (targets_extent, targets_length) = entries["TARGETS.JSON;1"];
    let tool_start = tool_extent as usize * 2048;
    assert_eq!(
        &image[tool_start..tool_start + tool_length as usize],
        tool.as_slice()
    );
    let targets_start = targets_extent as usize * 2048;
    assert_eq!(
        &image[targets_start..targets_start + targets_length as usize],
        targets.as_slice()
    );
}

fn parse_iso_root(sector: &[u8]) -> std::collections::HashMap<String, (u32, u32)> {
    let mut entries = std::collections::HashMap::new();
    let mut offset = 0usize;
    while offset < sector.len() && sector[offset] != 0 {
        let length = sector[offset] as usize;
        let name_length = sector[offset + 32] as usize;
        let name =
            String::from_utf8_lossy(&sector[offset + 33..offset + 33 + name_length]).into_owned();
        if name_length > 1 {
            let extent = u32::from_le_bytes(sector[offset + 2..offset + 6].try_into().unwrap());
            let size = u32::from_le_bytes(sector[offset + 10..offset + 14].try_into().unwrap());
            entries.insert(name, (extent, size));
        }
        offset += length;
    }
    entries
}

#[test]
fn vmx_options_enable_exactly_one_isolation_exception_each() {
    let network = VmxConfig::new("disk.vmdk", VmwareFirmware::Bios)
        .unwrap()
        .with_options(VmOptions {
            network: true,
            clipboard: false,
            time_sync: false,
        })
        .render();
    VmxConfig::validate_rendered(
        &network,
        VmOptions {
            network: true,
            clipboard: false,
            time_sync: false,
        },
        false,
    )
    .unwrap();
    assert!(network.contains("ethernet0.present = \"TRUE\""));
    assert!(network.contains("ethernet0.connectionType = \"hostonly\""));
    assert!(network.contains("isolation.tools.copy.disable = \"TRUE\""));

    let clipboard = VmxConfig::new("disk.vmdk", VmwareFirmware::Bios)
        .unwrap()
        .with_options(VmOptions {
            network: false,
            clipboard: true,
            time_sync: true,
        })
        .render();
    VmxConfig::validate_rendered(
        &clipboard,
        VmOptions {
            network: false,
            clipboard: true,
            time_sync: true,
        },
        false,
    )
    .unwrap();
    assert!(!clipboard.contains("isolation.tools.copy.disable"));
    assert!(clipboard.contains("tools.syncTime = \"TRUE\""));
    assert!(!clipboard.contains("time.synchronize"));
    assert!(clipboard.contains("ethernet0.present = \"FALSE\""));
}

#[test]
fn vmx_can_explicitly_select_lsi_logic() {
    let rendered = VmxConfig::new("disk.vmdk", VmwareFirmware::Efi)
        .unwrap()
        .with_disk_adapter(VmdkAdapter::LsiLogic)
        .render();

    validate_default(&rendered).unwrap();
    assert!(rendered.contains("scsi0.virtualDev = \"lsilogic\""));
    assert!(rendered.contains(r#"scsi0:0.fileName = "disk.vmdk""#));
    assert!(!rendered.contains("ide0:0.fileName"));
}

#[test]
fn vmx_validator_rejects_mixed_or_missing_evidence_disk_controllers() {
    let rendered = VmxConfig::new("disk.vmdk", VmwareFirmware::Bios)
        .unwrap()
        .render();
    assert!(validate_default(&rendered.replace("ide0:0.present = \"TRUE\"\n", "")).is_err());
    assert!(validate_default(&format!(
        "{rendered}ide0:0.deviceType = \"disk\"\nide0:0.fileName = \"disk.vmdk\"\nide0:0.present = \"TRUE\"\n"
    ))
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

    assert!(validate_default(&rendered.replace(
        "ethernet0.present = \"FALSE\"",
        "ethernet0.present = \"TRUE\""
    ))
    .is_err());
    assert!(
        validate_default(&rendered.replace("isolation.tools.copy.disable = \"TRUE\"\n", ""))
            .is_err()
    );
    assert!(validate_default(
        &rendered.replace("bios.bootOrder = \"hdd\"", "bios.bootOrder = \"cdrom,hdd\"")
    )
    .is_err());
}
