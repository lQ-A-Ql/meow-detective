use evidence_core::{probe, EvidenceReader, RawImageReader};
use std::io::{Read, Seek, SeekFrom, Write};

fn fixed_vhd(path: &std::path::Path, payload: &[u8], disk_type: u32) {
    let mut bytes = vec![0u8; payload.len().max(512)];
    bytes[..payload.len()].copy_from_slice(payload);
    bytes.extend_from_slice(&[0u8; 512]);
    let footer = bytes.len() - 512;
    bytes[footer..footer + 8].copy_from_slice(b"conectix");
    bytes[footer + 48..footer + 56].copy_from_slice(&(payload.len() as u64).to_be_bytes());
    bytes[footer + 60..footer + 64].copy_from_slice(&disk_type.to_be_bytes());
    std::fs::File::create(path)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
}

#[test]
fn opens_fixed_vhd_with_virtual_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("disk.vhd");
    fixed_vhd(&path, b"fixed payload", 2);
    let mut reader = RawImageReader::open(&path).unwrap();
    assert_eq!(reader.info().kind, "vhd-fixed");
    assert_eq!(reader.info().size, 13);
    let mut data = Vec::new();
    reader.read_to_end(&mut data).unwrap();
    assert_eq!(data, b"fixed payload");
    reader.seek(SeekFrom::Start(0)).unwrap();
}

#[test]
fn rejects_dynamic_vhd_and_does_not_probe_as_raw() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic.vhd");
    fixed_vhd(&path, b"dynamic", 3);
    let result = probe::probe(&path).unwrap();
    assert_eq!(result.candidates, vec!["vhd-dynamic"]);
    assert!(!result.can_read_sectors);
    assert!(RawImageReader::open(&path).is_err());
}

#[test]
fn rejects_fixed_vhd_that_claims_bytes_beyond_data_before_footer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("truncated.vhd");
    fixed_vhd(&path, b"payload", 2);
    let mut bytes = std::fs::read(&path).unwrap();
    let footer = bytes.len() - 512;
    bytes[footer + 48..footer + 56].copy_from_slice(&(4096u64).to_be_bytes());
    std::fs::write(&path, bytes).unwrap();
    let error = RawImageReader::open(&path).unwrap_err();
    assert!(error.to_string().contains("fixed VHD is truncated"));
}

#[test]
fn unknown_extension_is_not_guessed_as_raw() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unknown.container");
    std::fs::write(&path, b"not a recognized image").unwrap();
    let result = probe::probe(&path).unwrap();
    assert!(result.candidates.is_empty());
    assert!(!result.can_read_sectors);
}
