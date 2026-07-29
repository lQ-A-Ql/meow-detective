use std::path::{Path, PathBuf};

use memory_windows::{
    discover_directory_table_base, discover_kernel, scan_bitlocker_key_candidates, RawMemoryImage,
};

#[test]
#[ignore = "requires FORENSICS_LIUYANG_MEMORY_FIXTURE"]
fn discovers_fvevol_from_the_liuyang_raw_memory_fixture() {
    let path = PathBuf::from(
        std::env::var("FORENSICS_LIUYANG_MEMORY_FIXTURE")
            .expect("FORENSICS_LIUYANG_MEMORY_FIXTURE must be set"),
    );
    let discovery = discover_kernel(&path).expect("discover Windows kernel from raw memory");
    let fvevol = discovery.fvevol.expect("fvevol.sys must be loaded");
    assert!(fvevol.name.eq_ignore_ascii_case("fvevol.sys"));
    assert!(discovery.fvevol_identity.is_some());
}

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
fn inventories_liuyang_bitlocker_key_candidates_without_exposing_keys() {
    let path = std::env::var_os("FORENSICS_LIUYANG_MEMORY_FIXTURE")
        .expect("FORENSICS_LIUYANG_MEMORY_FIXTURE must point to ly-memdump.mem");
    let mut image = RawMemoryImage::open(Path::new(&path)).expect("open memory image");
    let candidates =
        scan_bitlocker_key_candidates(&mut image, 8_192, 256).expect("scan key candidates");

    for candidate in &candidates {
        eprintln!(
            "candidate tag={:?} bits={:?} pool={:#X} schedule_offset={:#X}",
            candidate.pool_tag(),
            candidate.bits(),
            candidate.pool_physical_address(),
            candidate.schedule_offset()
        );
    }
    assert!(
        !candidates.is_empty(),
        "the live BitLocker volume should retain at least one AES schedule"
    );
}
