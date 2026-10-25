#[path = "support/browser_hive.rs"]
mod hive;
#[path = "support/registry_logs.rs"]
mod logs;

use artifacts_windows::registry::{
    browser::HiveSnapshot, log_replay::RecoveryState, RegistrySnapshot,
};
use std::sync::Arc;

fn string_hive(path: &[&str], name: &str, text: &str) -> (Vec<u8>, u32) {
    let mut hive = hive::Hive::new();
    let raw: Vec<_> = text
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    let value = hive.value(name, 1, &raw, false);
    let mut root = hive.key(path.last().unwrap(), &[], &[value]);
    for parent in path[..path.len() - 1].iter().rev() {
        root = hive.key(parent, &[root], &[]);
    }
    let mut bytes = hive.finish(root);
    logs::seal(&mut bytes, 11, 10);
    (bytes, value)
}

fn replacement_log(base: &[u8], value: u32, text: &str) -> Vec<u8> {
    let mut pages = base[4096..].to_vec();
    let data_offset = u32::from_le_bytes(
        pages[value as usize + 12..value as usize + 16]
            .try_into()
            .unwrap(),
    ) as usize
        + 4;
    let raw: Vec<_> = text
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    pages[data_offset..data_offset + raw.len()].copy_from_slice(&raw);
    logs::log(base, 10, &[(0, pages)])
}

fn fixture(primary: u32, secondary: u32) -> (Vec<u8>, u32) {
    let mut hive = hive::Hive::new();
    let value = hive.value("number", 4, &1u32.to_le_bytes(), true);
    let root = hive.key("ROOT", &[], &[value]);
    let mut bytes = hive.finish(root);
    logs::seal(&mut bytes, primary, secondary);
    (bytes, value)
}

#[test]
fn snapshot_replays_pages_and_preserves_base_bytes_and_provenance() {
    let (base, value) = fixture(11, 10);
    let mut page = base[4096..4608].to_vec();
    page[value as usize + 12..value as usize + 16].copy_from_slice(&7u32.to_le_bytes());
    let log = logs::log(&base, 10, &[(0, page)]);
    let snapshot = RegistrySnapshot::prepare(&base, Some(&log), None).unwrap();
    assert!(snapshot.logs_applied());
    assert_eq!(snapshot.report().unwrap().state, RecoveryState::Recovered);
    assert!(!snapshot.provenance().is_empty());
    let view = HiveSnapshot::open_with_provenance(
        Arc::new(snapshot.bytes().to_vec()),
        snapshot.provenance().to_vec(),
    )
    .unwrap();
    assert_eq!(view.value(value).unwrap().preview, "7");
    let original = HiveSnapshot::open(Arc::new(base)).unwrap();
    assert_eq!(original.value(value).unwrap().preview, "1");
}

#[test]
fn snapshot_keeps_owned_input_without_recovery_or_an_additional_copy() {
    let (base, _) = fixture(10, 10);
    let pointer = base.as_ptr();
    let snapshot = RegistrySnapshot::from_owned(base, None, Some(&[])).unwrap();
    assert_eq!(snapshot.bytes().as_ptr(), pointer);
    assert!(!snapshot.logs_applied());
    assert!(snapshot.report().is_none());
    assert!(snapshot.provenance().is_empty());
    let snapshot = snapshot.into_owned();
    assert_eq!(snapshot.bytes().as_ptr(), pointer);
}

#[test]
fn clean_snapshot_with_a_log_keeps_the_borrowed_input() {
    let (base, _) = fixture(10, 10);
    let log = logs::log(&base, 10, &[(0, base[4096..4608].to_vec())]);
    let snapshot = RegistrySnapshot::prepare(&base, Some(&log), None).unwrap();
    assert_eq!(snapshot.bytes().as_ptr(), base.as_ptr());
    assert_eq!(snapshot.report().unwrap().state, RecoveryState::NotNeeded);
    assert!(!snapshot.logs_applied());
}

#[test]
fn snapshot_rejects_an_invalid_log_for_a_dirty_hive() {
    let (base, _) = fixture(11, 10);
    assert!(RegistrySnapshot::prepare(&base, Some(&[0; 8192]), None).is_err());
    assert!(RegistrySnapshot::prepare(b"invalid", None, None).is_err());
}

#[test]
fn system_extraction_and_browser_read_the_same_replayed_value() {
    let (base, value) = string_hive(
        &[
            "SYSTEM",
            "ControlSet001",
            "Control",
            "ComputerName",
            "ComputerName",
        ],
        "ComputerName",
        "OLD-PC",
    );
    let log = replacement_log(&base, value, "NEW-PC");
    let info =
        artifacts_windows::extract_system_hive_fields_with_txlog(&base, "SYSTEM", &log).unwrap();
    assert_eq!(info.computer_name.unwrap().value, "NEW-PC");
    assert!(info.txlog_applied);
    assert!(info.txlog_timestamps.is_empty());
    let snapshot = RegistrySnapshot::prepare(&base, Some(&log), None).unwrap();
    let view = HiveSnapshot::open(Arc::new(snapshot.bytes().to_vec())).unwrap();
    assert_eq!(view.value(value).unwrap().preview, "NEW-PC");
}

#[test]
fn software_and_ntuser_queries_use_page_recovery() {
    let (software, value) = string_hive(
        &["SOFTWARE", "Microsoft", "Windows NT", "CurrentVersion"],
        "ProductName",
        "OLD-OS",
    );
    let log = replacement_log(&software, value, "NEW-OS");
    let info =
        artifacts_windows::extract_software_hive_fields_with_txlog(&software, "SOFTWARE", &log)
            .unwrap();
    assert_eq!(info.product_name.unwrap().value, "NEW-OS");
    assert!(info.txlog_applied);
    let (ntuser, value) = string_hive(
        &[
            "NTUSER",
            "Software",
            "Microsoft",
            "Windows",
            "CurrentVersion",
            "Run",
        ],
        "App",
        "old.exe",
    );
    let log = replacement_log(&ntuser, value, "new.exe");
    let info =
        artifacts_windows::extract_ntuser_fields_with_txlog(&ntuser, "NTUSER.DAT", &log).unwrap();
    assert_eq!(info.run_keys[0].command, "new.exe");
    assert!(info.txlog_applied);
    assert!(info.txlog_timestamps.is_empty());
}

#[test]
fn security_query_uses_the_same_page_recovery() {
    let (base, value) = string_hive(&["SECURITY", "Policy"], "PolPrDmS", "OLD");
    let log = replacement_log(&base, value, "NEW");
    let info = artifacts_windows::extract_security_policy_from_security_hive_with_txlog(
        &base,
        "SECURITY",
        None,
        Some(&log),
        None,
    )
    .unwrap();
    assert_eq!(info.domain_name.as_deref(), Some("NEW"));
    assert!(info.txlog_applied);
    assert!(info.txlog_timestamps.is_empty());
}

#[test]
fn snapshot_accepts_old_dirty_page_logs() {
    let (base, value) = fixture(11, 10);
    let mut pages = base[4096..8192].to_vec();
    pages[value as usize + 12..value as usize + 16].copy_from_slice(&8u32.to_le_bytes());
    let log = logs::old_log(&base, 11, &pages);
    let snapshot = RegistrySnapshot::prepare(&base, Some(&log), None).unwrap();
    assert!(snapshot.logs_applied());
    let view = HiveSnapshot::open(Arc::new(snapshot.bytes().to_vec())).unwrap();
    assert_eq!(view.value(value).unwrap().preview, "8");
}
