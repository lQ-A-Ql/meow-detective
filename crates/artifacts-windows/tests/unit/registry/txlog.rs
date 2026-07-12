use super::*;
use crate::registry::tests::txlog_fixture::*;

fn entry(operation: u16, sequence_number: u32) -> SyntheticEntry {
    SyntheticEntry {
        operation,
        sequence_number,
        timestamp: Some(0x01db_9f8c_0000_0000),
        key_path: r"HKLM\SOFTWARE\Test".to_string(),
        value_name: Some("Value".to_string()),
        data_before: Some(vec![1, 2]),
        data_after: Some(vec![3, 4]),
    }
}

#[test]
fn rejects_short_and_invalid_logs() {
    assert!(parse_transaction_log(&[0u8; 100])
        .unwrap_err()
        .to_string()
        .contains("too short"));
    let mut invalid = vec![0u8; 5000];
    invalid[0..4].copy_from_slice(b"BEEF");
    assert!(parse_transaction_log(&invalid)
        .unwrap_err()
        .to_string()
        .contains("unrecognised"));
}

#[test]
fn accepts_primary_and_secondary_headers() {
    let primary = parse_transaction_log(&build_synthetic_log1(&[])).unwrap();
    let secondary = parse_transaction_log(&build_synthetic_log2(&[])).unwrap();
    assert!(primary.primary);
    assert!(!secondary.primary);
    assert!(primary.transactions.is_empty());
}

#[test]
fn parses_set_value_before_and_after_data() {
    let parsed = parse_transaction_log(&build_synthetic_log1(&[entry(2, 1)])).unwrap();
    let transaction = &parsed.transactions[0];
    assert_eq!(
        transaction.operation,
        RegistryTransactionOperation::SetValue
    );
    assert_eq!(transaction.key_path, r"HKLM\SOFTWARE\Test");
    assert_eq!(transaction.value_name.as_deref(), Some("Value"));
    assert_eq!(transaction.data_before.as_deref(), Some([1, 2].as_slice()));
    assert_eq!(transaction.data_after.as_deref(), Some([3, 4].as_slice()));
    assert!(transaction.timestamp.is_some());
}

#[test]
fn preserves_operation_specific_data_semantics() {
    let entries = (0..=4)
        .map(|operation| entry(operation, operation as u32 + 1))
        .collect::<Vec<_>>();
    let parsed = parse_transaction_log(&build_synthetic_log1(&entries)).unwrap();
    assert_eq!(parsed.transactions.len(), 5);
    assert_eq!(
        parsed.transactions[0].operation,
        RegistryTransactionOperation::CreateKey
    );
    assert!(parsed.transactions[0].value_name.is_none());
    assert!(parsed.transactions[0].data_before.is_none());
    assert!(parsed.transactions[0].data_after.is_some());
    assert!(parsed.transactions[1].data_before.is_none());
    assert!(parsed.transactions[1].data_after.is_none());
    assert!(parsed.transactions[2].data_before.is_some());
    assert!(parsed.transactions[3].data_after.is_none());
    assert_eq!(
        parsed.transactions[4].operation,
        RegistryTransactionOperation::RenameKey
    );
}

#[test]
fn detects_ring_buffer_wraparound_once() {
    let parsed = parse_transaction_log(&build_synthetic_log1(&[
        entry(2, 100),
        entry(2, 10),
        entry(2, 5),
    ]))
    .unwrap();
    assert_eq!(
        parsed
            .warnings
            .iter()
            .filter(|warning| warning.contains("wraparound"))
            .count(),
        1
    );
}

#[test]
fn monotonic_sequences_do_not_warn() {
    let parsed = parse_transaction_log(&build_synthetic_log1(&[
        entry(2, 1),
        entry(2, 2),
        entry(2, 3),
    ]))
    .unwrap();
    assert!(!parsed
        .warnings
        .iter()
        .any(|warning| warning.contains("wraparound")));
}

#[test]
fn parses_unicode_paths_and_value_names() {
    let mut unicode = entry(2, 1);
    unicode.key_path = r"HKLM\软件\测试".to_string();
    unicode.value_name = Some("显示名称".to_string());
    let parsed = parse_transaction_log(&build_synthetic_log1(&[unicode])).unwrap();
    assert_eq!(parsed.transactions[0].key_path, r"HKLM\软件\测试");
    assert_eq!(
        parsed.transactions[0].value_name.as_deref(),
        Some("显示名称")
    );
}

#[test]
fn zero_size_entry_stops_parsing() {
    let mut data = build_synthetic_log1(&[entry(2, 1)]);
    data.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(parse_transaction_log(&data).unwrap().transactions.len(), 1);
}

#[test]
fn truncated_entry_warns_and_stops() {
    let mut data = build_synthetic_log1(&[entry(2, 1)]);
    data.truncate(data.len() - 2);
    let parsed = parse_transaction_log(&data).unwrap();
    assert!(parsed.transactions.is_empty());
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.contains("past EOF")));
}

#[test]
fn unknown_operation_is_skipped() {
    let parsed = parse_transaction_log(&build_synthetic_log1(&[entry(99, 1)])).unwrap();
    assert!(parsed.transactions.is_empty());
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.contains("unknown operation")));
}

#[test]
fn implausible_timestamp_is_omitted() {
    let mut invalid_time = entry(2, 1);
    invalid_time.timestamp = Some(1);
    let parsed = parse_transaction_log(&build_synthetic_log1(&[invalid_time])).unwrap();
    assert!(parsed.transactions[0].timestamp.is_none());
}

#[test]
fn merge_orders_both_logs_by_sequence() {
    let log1 = build_synthetic_log1(&[entry(2, 20)]);
    let log2 = build_synthetic_log2(&[entry(2, 10)]);
    let (transactions, warnings) = parse_and_merge_txlogs(Some(&log1), Some(&log2));
    assert!(warnings.is_empty());
    assert_eq!(
        transactions
            .iter()
            .map(|transaction| transaction.sequence_number)
            .collect::<Vec<_>>(),
        vec![10, 20]
    );
}

#[test]
fn merge_treats_corrupt_companion_as_warning() {
    let valid = build_synthetic_log1(&[entry(2, 1)]);
    let (transactions, warnings) = parse_and_merge_txlogs(Some(&valid), Some(b"bad"));
    assert_eq!(transactions.len(), 1);
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("LOG2 parse failed")));
}
