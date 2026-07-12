use super::*;
use crate::builders::registry;

#[test]
fn tiny_logical_fixture_exists() {
    let root = tiny_logical_dir();
    assert!(
        root.is_dir(),
        "missing fixture directory: {}",
        root.display()
    );
    assert!(root.join("README.txt").is_file());
    assert!(root.join("Users/alice/notes.txt").is_file());
}

#[test]
fn tiny_raw_fixture_exists_and_has_mbr_signature() {
    let path = tiny_raw_image();
    assert!(path.is_file(), "missing RAW fixture: {}", path.display());

    let bytes = std::fs::read(&path).expect("read tiny RAW fixture");
    assert_eq!(bytes.len(), 1024);
    assert_eq!(&bytes[510..512], &[0x55, 0xAA]);
    assert_eq!(&bytes[512..520], b"FWB-TINY");
}

#[test]
fn tiny_e01_fixture_exists_and_has_evf_header() {
    let path = tiny_e01_image();
    assert!(path.is_file(), "missing E01 fixture: {}", path.display());

    let bytes = std::fs::read(&path).expect("read tiny E01 fixture");
    assert!(bytes.len() < 1024 * 1024);
    assert_eq!(&bytes[0..3], b"EVF");
    assert!(bytes
        .windows(b"FWB-TINY-E01".len())
        .any(|w| w == b"FWB-TINY-E01"));
}

#[test]
fn tiny_system_evtx_fixture_exists_and_has_evtx_header() {
    let path = tiny_system_evtx();
    assert!(
        path.is_file(),
        "missing System.evtx fixture: {}",
        path.display()
    );

    let bytes = std::fs::read(&path).expect("read tiny System.evtx fixture");
    assert!(bytes.len() < 2 * 1024 * 1024);
    assert_eq!(&bytes[0..8], b"ElfFile\0");
}

#[test]
fn tiny_registry_system_fixture_matches_builder() {
    let path = tiny_registry_system_hive();
    assert!(
        path.is_file(),
        "missing SYSTEM registry fixture: {}",
        path.display()
    );

    let bytes = std::fs::read(&path).expect("read tiny SYSTEM hive fixture");
    assert_eq!(&bytes[0..4], b"regf");
    assert_eq!(bytes, registry::synthetic_system_hive());
}

#[test]
fn tiny_registry_software_fixture_matches_builder() {
    let path = tiny_registry_software_hive();
    assert!(
        path.is_file(),
        "missing SOFTWARE registry fixture: {}",
        path.display()
    );

    let bytes = std::fs::read(&path).expect("read tiny SOFTWARE hive fixture");
    assert_eq!(&bytes[0..4], b"regf");
    assert_eq!(bytes, registry::synthetic_software_hive());
}

#[test]
fn local_e01_fixture_is_opt_in() {
    let fixture = local_e01_fixture();
    if let Some(path) = fixture {
        assert!(path.is_file());
    }
}

#[test]
fn local_liuyang_e01_fixture_is_opt_in() {
    let fixture = local_liuyang_e01_fixture();
    if let Some(path) = fixture {
        assert!(path.is_file());
    }
}
