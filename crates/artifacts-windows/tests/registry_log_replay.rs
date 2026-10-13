#[path = "support/browser_hive.rs"]
mod hive;
#[path = "support/registry_logs.rs"]
mod logs;
use artifacts_windows::registry::{browser::HiveSnapshot, log_replay::*};
use std::sync::Arc;
const LIMIT: usize = 128 * 1024 * 1024;

fn fixture(primary: u32, secondary: u32) -> (Vec<u8>, u32) {
    let mut hive = hive::Hive::new();
    let value = hive.value("number", 4, &1u32.to_le_bytes(), true);
    let root = hive.key("ROOT", &[], &[value]);
    let mut bytes = hive.finish(root);
    logs::seal(&mut bytes, primary, secondary);
    (bytes, value)
}
fn page(base: &[u8], value: u32, number: u32) -> Vec<u8> {
    let mut page = base[4096..4608].to_vec();
    let at = value as usize + 12;
    page[at..at + 4].copy_from_slice(&number.to_le_bytes());
    page
}
fn value(bytes: Vec<u8>) -> String {
    let hive = HiveSnapshot::open(Arc::new(bytes)).unwrap();
    hive.value(hive.values(&hive.root_key().unwrap(), 0, 10).unwrap().items[0].0)
        .unwrap()
        .preview
}
#[test]
fn applies_both_logs_in_sequence_not_filename_order() {
    let (base, offset) = fixture(11, 10);
    let first = logs::log(&base, 10, &[(0, page(&base, offset, 2))]);
    let second = logs::log(&base, 11, &[(0, page(&base, offset, 3))]);
    let inputs = [
        HiveLog {
            slot: LogSlot::Log1,
            bytes: Some(&second),
        },
        HiveLog {
            slot: LogSlot::Log2,
            bytes: Some(&first),
        },
    ];
    let plan = prepare_log_replay(&base, &inputs, LIMIT).unwrap();
    let result = plan.apply(base.clone()).unwrap();
    assert_eq!(result.report.state, RecoveryState::Recovered);
    assert_eq!(result.report.applied_entries, 2);
    assert_eq!(result.report.result_sequence, Some(11));
    assert_eq!(value(result.bytes), "3");
    assert_eq!(value(base), "1");
}
#[test]
fn clean_hive_keeps_original_bytes_even_if_log_contains_newer_data() {
    let (base, offset) = fixture(10, 10);
    let log = logs::log(&base, 11, &[(0, page(&base, offset, 9))]);
    let result = prepare_log_replay(
        &base,
        &[HiveLog {
            slot: LogSlot::Log1,
            bytes: Some(&log),
        }],
        LIMIT,
    )
    .unwrap()
    .apply(base.clone())
    .unwrap();
    assert_eq!(result.report.state, RecoveryState::NotNeeded);
    assert_eq!(result.bytes, base);
    assert_eq!(result.report.applied_entries, 0);
}
#[test]
fn hash_mismatch_does_not_modify_or_expose_a_recovered_hive() {
    let (base, offset) = fixture(11, 10);
    let mut log = logs::log(&base, 10, &[(0, page(&base, offset, 9))]);
    log[600] ^= 1;
    let error = prepare_log_replay(
        &base,
        &[HiveLog {
            slot: LogSlot::Log1,
            bytes: Some(&log),
        }],
        LIMIT,
    )
    .err()
    .unwrap();
    assert_eq!(error.report.logs[0].diagnostic, Some(LogReplayError::Hash));
    assert_eq!(value(base), "1");
}
#[test]
fn identity_and_stale_records_are_rejected() {
    let (base, offset) = fixture(11, 10);
    let mut foreign = logs::log(&base, 10, &[(0, page(&base, offset, 9))]);
    foreign[112] = 2;
    logs::checksum(&mut foreign);
    let error = prepare_log_replay(
        &base,
        &[HiveLog {
            slot: LogSlot::Log1,
            bytes: Some(&foreign),
        }],
        LIMIT,
    )
    .err()
    .unwrap();
    assert_eq!(
        error.report.logs[0].diagnostic,
        Some(LogReplayError::Identity)
    );
    let stale = logs::log(&base, 9, &[(0, page(&base, offset, 9))]);
    assert!(prepare_log_replay(
        &base,
        &[HiveLog {
            slot: LogSlot::Log1,
            bytes: Some(&stale)
        }],
        LIMIT
    )
    .is_err());
}
#[test]
fn discontinuity_exposes_only_a_valid_prefix_with_partial_status() {
    let (base, offset) = fixture(11, 10);
    let first = logs::log(&base, 10, &[(0, page(&base, offset, 2))]);
    let gap = logs::log(&base, 12, &[(0, page(&base, offset, 3))]);
    let result = prepare_log_replay(
        &base,
        &[
            HiveLog {
                slot: LogSlot::Log1,
                bytes: Some(&first),
            },
            HiveLog {
                slot: LogSlot::Log2,
                bytes: Some(&gap),
            },
        ],
        LIMIT,
    )
    .unwrap()
    .apply(base.clone())
    .unwrap();
    assert_eq!(result.report.state, RecoveryState::Partial);
    assert_eq!(result.report.applied_entries, 1);
    assert_eq!(value(result.bytes), "2");
}
#[test]
fn sequence_wraparound_and_old_dirty_vectors_work() {
    let (base, offset) = fixture(0, u32::MAX);
    let first = logs::log(&base, u32::MAX, &[(0, page(&base, offset, 2))]);
    let second = logs::log(&base, 0, &[(0, page(&base, offset, 3))]);
    let result = prepare_log_replay(
        &base,
        &[
            HiveLog {
                slot: LogSlot::Log1,
                bytes: Some(&second),
            },
            HiveLog {
                slot: LogSlot::Log2,
                bytes: Some(&first),
            },
        ],
        LIMIT,
    )
    .unwrap()
    .apply(base.clone())
    .unwrap();
    assert_eq!(value(result.bytes), "3");
    let mut pages = base[4096..8192].to_vec();
    pages[offset as usize + 12..offset as usize + 16].copy_from_slice(&7u32.to_le_bytes());
    let old = logs::old_log(&base, 0, &pages);
    let result = prepare_log_replay(
        &base,
        &[HiveLog {
            slot: LogSlot::Log1,
            bytes: Some(&old),
        }],
        LIMIT,
    )
    .unwrap()
    .apply(base.clone())
    .unwrap();
    assert_eq!(value(result.bytes), "7");
}
#[test]
fn invalid_checksum_uses_only_the_newest_eligible_backup_header() {
    let (mut base, offset) = fixture(11, 10);
    let first = logs::log(&base, 10, &[(0, page(&base, offset, 2))]);
    let second = logs::log(&base, 11, &[(0, page(&base, offset, 3))]);
    base[508] ^= 1;
    let result = prepare_log_replay(
        &base,
        &[
            HiveLog {
                slot: LogSlot::Log1,
                bytes: Some(&first),
            },
            HiveLog {
                slot: LogSlot::Log2,
                bytes: Some(&second),
            },
        ],
        LIMIT,
    )
    .unwrap()
    .apply(base.clone())
    .unwrap();
    assert_eq!(result.report.applied_entries, 1);
    assert_eq!(value(result.bytes), "3");
}
#[test]
fn invalid_bounds_and_conflicting_equal_sequence_logs_fail_closed() {
    let (base, offset) = fixture(11, 10);
    let bad = logs::log(&base, 10, &[(0x40000, page(&base, offset, 2))]);
    assert!(prepare_log_replay(
        &base,
        &[HiveLog {
            slot: LogSlot::Log1,
            bytes: Some(&bad)
        }],
        LIMIT
    )
    .is_err());
    let a = logs::log(&base, 10, &[(0, page(&base, offset, 2))]);
    let b = logs::log(&base, 10, &[(0, page(&base, offset, 3))]);
    let error = prepare_log_replay(
        &base,
        &[
            HiveLog {
                slot: LogSlot::Log1,
                bytes: Some(&a),
            },
            HiveLog {
                slot: LogSlot::Log2,
                bytes: Some(&b),
            },
        ],
        LIMIT,
    )
    .err()
    .unwrap();
    assert_eq!(error.error, LogReplayError::Conflict);
}
