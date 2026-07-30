use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use memory_windows::{
    discover_directory_table_base, recover_vmks_structurally, BitLockerMemoryProfile,
    LoadedModuleEntryLayout, RawMemoryImage, TargetedCodeViewIdentity, TargetedKernelIdentity,
    TargetedKernelLayoutProfile, TargetedKernelSearchLimits, X64AddressSpace,
};

const LIUYANG_DRIVER_OBJECT: u64 = 0xFFFF_9486_90B9_3B20;
const LIUYANG_VOLUME_GUID: [u8; 16] = [
    0x13, 0xE7, 0x7A, 0x6B, 0xF9, 0x92, 0xFF, 0x45, 0xB7, 0x2A, 0xC0, 0x16, 0xB2, 0x99, 0x2B, 0x25,
];

#[test]
#[ignore = "requires FORENSICS_LIUYANG_MEMORY_FIXTURE"]
fn discovers_liuyang_cr3_without_kdbg_or_external_tools() {
    let path = std::env::var_os("FORENSICS_LIUYANG_MEMORY_FIXTURE")
        .expect("FORENSICS_LIUYANG_MEMORY_FIXTURE must point to ly-memdump.mem");
    let start = discover_directory_table_base(Path::new(&path)).expect("discover CR3");

    assert_eq!(start.directory_table_base, 0x1AE000);
    assert_eq!(start.physical_address & 0xFFF, 0);
    assert_eq!(start.long_mode_target >> 63, 1);
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_MEMORY_FIXTURE"]
fn recovers_liuyang_active_vmk_through_exact_fvevol_objects() {
    let path = fixture_path();
    let started = Instant::now();
    let recovery = recover_vmks_structurally(
        &path,
        &liuyang_profile(),
        LIUYANG_VOLUME_GUID,
        TargetedKernelSearchLimits::default(),
    )
    .expect("recover structurally sourced VMK");
    let elapsed = started.elapsed();

    assert_eq!(recovery.recovered_vmk_count(), 1);
    assert_eq!(recovery.keyring_datasets_examined(), 0);
    assert!(recovery.devices_examined() > 0);
    assert!(recovery.datum_pointers_examined() > 0);
    assert!(recovery.physical_reads().operations <= 16_384);
    assert!(recovery.physical_reads().bytes_read <= 32 * 1024 * 1024);
    assert!(elapsed <= Duration::from_secs(90));
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_MEMORY_FIXTURE"]
fn confirms_liuyang_fvevol_keyring_is_structurally_valid_and_empty() {
    let raw = RawMemoryImage::open(&fixture_path()).expect("open memory image");
    let mut memory = X64AddressSpace::new(raw, 0x1AE000).expect("open address space");
    let driver_extension = memory
        .read_virtual_u64(LIUYANG_DRIVER_OBJECT + 0x30)
        .expect("read driver extension");
    let mut client = memory
        .read_virtual_u64(driver_extension + 0x28)
        .expect("read client extension list");
    let mut seen = HashSet::new();
    let mut matching_keyring = None;

    while client != 0 && seen.insert(client) && seen.len() <= 64 {
        let next = memory
            .read_virtual_u64(client)
            .expect("read next client extension");
        let identifier = memory
            .read_virtual_u64(client + 8)
            .expect("read client identifier");
        if identifier == LIUYANG_DRIVER_OBJECT {
            let keyring = memory
                .read_virtual_u64(client + 0x10 + 0x278)
                .expect("read client keyring pointer");
            let mut header = [0u8; 0x20];
            memory
                .read_virtual_exact(keyring, &mut header)
                .expect("read client keyring header");
            assert_eq!(&header[..8], b"-FVE-FS-");
            assert_eq!(
                u32::from_le_bytes(header[8..12].try_into().unwrap()),
                0x4000
            );
            assert_eq!(u32::from_le_bytes(header[12..16].try_into().unwrap()), 1);
            matching_keyring = Some((
                u32::from_le_bytes(header[16..20].try_into().unwrap()),
                u32::from_le_bytes(header[20..24].try_into().unwrap()),
            ));
        }
        client = next;
    }

    assert_eq!(matching_keyring, Some((0x20, 0x20)));
}

fn fixture_path() -> PathBuf {
    PathBuf::from(
        std::env::var("FORENSICS_LIUYANG_MEMORY_FIXTURE")
            .expect("FORENSICS_LIUYANG_MEMORY_FIXTURE must be set"),
    )
}

fn liuyang_profile() -> BitLockerMemoryProfile {
    let module_layout =
        LoadedModuleEntryLayout::new(0, 0, 0x30, 0x40, 0x58, 0x60).expect("loader layout");
    let kernel = TargetedKernelLayoutProfile::new(
        "windows-11-26100-ntkrnlmp-953a8de8",
        "26100",
        TargetedKernelIdentity::new(0xD98D_B6A6, 0x0144_F000),
        module_layout,
    )
    .expect("kernel profile")
    .with_codeview_identity(
        TargetedCodeViewIdentity::new("953A8DE8-80B0-818C-32DA-2DEC1D79C2D9", 1, "ntkrnlmp.pdb")
            .expect("kernel CodeView identity"),
    )
    .with_fvevol_identity(TargetedKernelIdentity::new(0x5960_C289, 0x000E_1000))
    .with_fvevol_codeview_identity(
        TargetedCodeViewIdentity::new("47808A31-873E-98CF-7009-95E410CD0095", 1, "fvevol.pdb")
            .expect("FVEVol CodeView identity"),
    );
    BitLockerMemoryProfile::windows_11_26100(kernel).expect("reviewed profile")
}
