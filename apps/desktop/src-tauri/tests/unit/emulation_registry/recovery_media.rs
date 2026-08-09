use super::{validate_boot_descriptors, RecoveryMedia, RecoveryMediaError};

#[test]
fn bootable_iso_is_validated_and_fingerprinted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("WinPE.iso");
    write_iso(&path, true);

    let media = RecoveryMedia::open(&path).unwrap();
    assert_eq!(media.file_name(), "WinPE.iso");
    assert!(media.vmware_path().ends_with("WinPE.iso"));
    assert_eq!(media.length(), 19 * 2048);
    assert_eq!(media.sha256().len(), 64);
    let mut file = std::fs::File::open(&path).unwrap();
    assert!(validate_boot_descriptors(&mut file).is_ok());
}

#[test]
fn iso_without_el_torito_descriptor_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("data.iso");
    write_iso(&path, false);

    assert!(RecoveryMedia::open(&path).is_err());
}

#[test]
fn iso_truncated_before_descriptor_terminator_is_not_bootable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("short.iso");
    // Exactly the minimum length: valid primary and boot descriptors but no
    // terminator sector, so validation runs into the end of the file.
    let mut bytes = vec![0u8; 19 * 2048];
    write_descriptor(&mut bytes, 16, 1, b"");
    write_descriptor(&mut bytes, 17, 0, b"EL TORITO SPECIFICATION");
    write_descriptor(&mut bytes, 18, 2, b"SUPPLEMENTARY");
    std::fs::write(&path, bytes).unwrap();

    let error = match RecoveryMedia::open(&path) {
        Ok(_) => panic!("a descriptor set without a terminator must be rejected"),
        Err(error) => error,
    };
    assert!(matches!(error, RecoveryMediaError::NotBootableIso));
}

fn write_iso(path: &std::path::Path, bootable: bool) {
    let mut bytes = vec![0u8; 19 * 2048];
    write_descriptor(&mut bytes, 16, 1, b"");
    write_descriptor(
        &mut bytes,
        17,
        if bootable { 0 } else { 2 },
        if bootable {
            b"EL TORITO SPECIFICATION"
        } else {
            b"SUPPLEMENTARY"
        },
    );
    write_descriptor(&mut bytes, 18, 255, b"");
    std::fs::write(path, bytes).unwrap();
}

fn write_descriptor(bytes: &mut [u8], sector: usize, kind: u8, system_id: &[u8]) {
    let offset = sector * 2048;
    bytes[offset] = kind;
    bytes[offset + 1..offset + 6].copy_from_slice(b"CD001");
    bytes[offset + 6] = 1;
    bytes[offset + 7..offset + 7 + system_id.len()].copy_from_slice(system_id);
}
