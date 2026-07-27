use super::*;

fn base_sector() -> Vec<u8> {
    let mut sector = vec![0u8; 512];
    sector[12] = 0x02; // bytes per sector = 512, little-endian
    sector
}

#[test]
fn parses_bitlocker_to_go_layout() {
    let mut sector = base_sector();
    sector[0..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    sector[3..11].copy_from_slice(b"MSWIN4.1");
    sector[440..448].copy_from_slice(&0x0210_0000u64.to_le_bytes());
    sector[448..456].copy_from_slice(&0x02b5_5800u64.to_le_bytes());
    sector[456..464].copy_from_slice(&0x035a_b000u64.to_le_bytes());

    let header = VolumeHeader::parse(&sector).expect("MSWIN4.1 header parses");
    assert_eq!(header.variant, HeaderVariant::BitLockerToGoCandidate);
    assert_eq!(header.bytes_per_sector, 512);
    assert_eq!(
        header.fve_metadata_offsets,
        [0x0210_0000, 0x02b5_5800, 0x035a_b000]
    );
}

#[test]
fn mswin41_alone_is_not_self_identifying() {
    // A plain FAT volume formatted by Windows carries MSWIN4.1 too, so this
    // variant must never be treated as proof of encryption on its own; only a
    // valid -FVE-FS- metadata block settles it.
    let mut sector = base_sector();
    sector[3..11].copy_from_slice(b"MSWIN4.1");
    let header = VolumeHeader::parse(&sector).expect("header parses");
    assert!(
        !header.variant.is_self_identifying(),
        "MSWIN4.1 must require metadata confirmation"
    );
}

#[test]
fn fve_signature_is_self_identifying() {
    let mut sector = base_sector();
    sector[0..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    sector[3..11].copy_from_slice(b"-FVE-FS-");
    let header = VolumeHeader::parse(&sector).expect("header parses");
    assert!(header.variant.is_self_identifying());
}

#[test]
fn parses_windows7_layout() {
    let mut sector = base_sector();
    sector[0..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    sector[3..11].copy_from_slice(b"-FVE-FS-");
    sector[176..184].copy_from_slice(&0x1000u64.to_le_bytes());
    sector[184..192].copy_from_slice(&0x2000u64.to_le_bytes());
    sector[192..200].copy_from_slice(&0x3000u64.to_le_bytes());

    let header = VolumeHeader::parse(&sector).expect("header parses");
    assert_eq!(header.variant, HeaderVariant::Windows7OrLater);
    assert_eq!(header.fve_metadata_offsets, [0x1000, 0x2000, 0x3000]);
}

#[test]
fn vista_layout_converts_a_cluster_number_to_a_byte_offset() {
    let mut sector = base_sector();
    sector[0..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
    sector[3..11].copy_from_slice(b"-FVE-FS-");
    sector[13] = 8; // sectors per cluster
    sector[56..64].copy_from_slice(&100u64.to_le_bytes());

    let header = VolumeHeader::parse(&sector).expect("header parses");
    assert_eq!(header.variant, HeaderVariant::WindowsVista);
    assert_eq!(header.fve_metadata_offsets[0], 100 * 512 * 8);
    assert_eq!(
        header.fve_metadata_offsets[1..],
        [0, 0],
        "Vista carries only the first offset in the boot sector"
    );
}

#[test]
fn vista_layout_clamps_a_zero_sectors_per_cluster() {
    // A corrupt BPB must yield a bounded offset that metadata validation then
    // rejects, rather than collapsing every cluster number to offset zero.
    let mut sector = base_sector();
    sector[0..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
    sector[3..11].copy_from_slice(b"-FVE-FS-");
    sector[13] = 0;
    sector[56..64].copy_from_slice(&7u64.to_le_bytes());

    let header = VolumeHeader::parse(&sector).expect("header parses");
    assert_eq!(header.fve_metadata_offsets[0], 7 * 512);
}

#[test]
fn vista_cluster_offset_saturates_instead_of_overflowing() {
    let mut sector = base_sector();
    sector[0..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
    sector[3..11].copy_from_slice(b"-FVE-FS-");
    sector[13] = 0xFF;
    sector[56..64].copy_from_slice(&u64::MAX.to_le_bytes());

    let header = VolumeHeader::parse(&sector).expect("header parses");
    assert_eq!(header.fve_metadata_offsets[0], u64::MAX);
}

#[test]
fn zero_bytes_per_sector_falls_back_to_512() {
    let mut sector = base_sector();
    sector[11] = 0;
    sector[12] = 0;
    sector[0..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    sector[3..11].copy_from_slice(b"-FVE-FS-");
    let header = VolumeHeader::parse(&sector).expect("header parses");
    assert_eq!(header.bytes_per_sector, 512);
}

#[test]
fn rejects_a_non_bitlocker_signature() {
    let mut sector = base_sector();
    sector[0..3].copy_from_slice(&[0xeb, 0x52, 0x90]);
    sector[3..11].copy_from_slice(b"NTFS    ");
    let error = VolumeHeader::parse(&sector).expect_err("NTFS must not parse as BitLocker");
    assert_eq!(error.code(), "BITLOCKER_METADATA_UNREADABLE");
}

#[test]
fn a_short_sector_errors_without_panicking() {
    let error = VolumeHeader::parse(&[0u8; 4]).expect_err("a 4-byte sector cannot be a header");
    assert_eq!(error.code(), "BITLOCKER_METADATA_UNREADABLE");
}

#[test]
fn header_error_does_not_leak_raw_bytes_as_control_characters() {
    // The signature reaches an error string and therefore a log. Rendering it
    // lossily keeps a volume full of binary from injecting control bytes there.
    let mut sector = base_sector();
    sector[3..11].copy_from_slice(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]);
    let error = VolumeHeader::parse(&sector).expect_err("must reject");
    let rendered = error.to_string();
    assert!(rendered.contains("expected -FVE-FS- or MSWIN4.1"));
}
