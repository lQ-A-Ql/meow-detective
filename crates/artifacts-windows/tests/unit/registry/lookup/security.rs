use super::*;
use crate::registry::tests::txlog_fixture::{build_synthetic_log1, SyntheticEntry};
use crate::registry::tests::*;

#[test]
fn extract_security_policy_from_fixture() {
    let mut data = empty_hive("ROOT");
    write_nk(&mut data, 0x20, "ROOT", &[("Policy", 0x200)], &[]);
    write_nk(
        &mut data,
        0x200,
        "Policy",
        &[],
        &[0x400, 0x480, 0x500, 0x580],
    );

    let filetime = 133_801_632_000_000_000u64;
    let policy_abs = super::super::BASE_BLOCK_SIZE + 0x200;
    data[policy_abs + 0x08..policy_abs + 0x10].copy_from_slice(&filetime.to_le_bytes());

    let domain = encode_utf16le("CORP");
    let mut domain_data = Vec::new();
    domain_data.extend_from_slice(&(domain.len() as u32).to_le_bytes());
    domain_data.extend_from_slice(&domain);
    write_binary_value(&mut data, 0x400, "PolPrDmS", &domain_data, 0x5000);
    write_binary_value(
        &mut data,
        0x480,
        "PolAcDmS",
        &encode_utf16le("ACCT"),
        0x5100,
    );
    write_binary_value(
        &mut data,
        0x500,
        "PolMachineAccountS",
        &make_sid(&[21, 123_456_789, 123_456_789, 123_456_789, 1000]),
        0x5200,
    );
    write_binary_value(
        &mut data,
        0x580,
        "PolAdtEv",
        &[0x01, 0x02, 0x03, 0x04],
        0x5300,
    );

    let result = extract_security_policy_from_security_hive(&data, "", None).unwrap();
    assert_eq!(result.domain_name.as_deref(), Some("CORP"));
    assert_eq!(result.account_domain_name.as_deref(), Some("ACCT"));
    assert_eq!(
        result.machine_sid.as_deref(),
        Some("S-1-5-21-123456789-123456789-123456789-1000")
    );
    assert_eq!(result.audit_policy_hex.as_deref(), Some("01020304"));
    assert_eq!(result.source_key_path, "Policy");
    assert_eq!(result.last_write, windows_filetime_to_rfc3339(filetime));
}

#[test]
fn missing_policy_key_returns_error() {
    assert!(extract_security_policy_from_security_hive(&empty_hive("ROOT"), "", None).is_err());
}

#[test]
fn extract_security_policy_with_txlog_overrides_domain_name() {
    let mut data = empty_hive("ROOT");
    write_nk(&mut data, 0x20, "ROOT", &[("Policy", 0x200)], &[]);
    write_nk(
        &mut data,
        0x200,
        "Policy",
        &[],
        &[0x400, 0x480, 0x500, 0x580],
    );
    let domain = encode_utf16le("CORP");
    let mut domain_data = Vec::new();
    domain_data.extend_from_slice(&(domain.len() as u32).to_le_bytes());
    domain_data.extend_from_slice(&domain);
    write_binary_value(&mut data, 0x400, "PolPrDmS", &domain_data, 0x5000);
    write_binary_value(
        &mut data,
        0x480,
        "PolAcDmS",
        &encode_utf16le("ACCT"),
        0x5100,
    );
    write_binary_value(
        &mut data,
        0x500,
        "PolMachineAccountS",
        &make_sid(&[21, 123_456_789, 123_456_789, 123_456_789, 1000]),
        0x5200,
    );
    write_binary_value(
        &mut data,
        0x580,
        "PolAdtEv",
        &[0x01, 0x02, 0x03, 0x04],
        0x5300,
    );

    let txlog_domain = encode_utf16le("TXLOG-CORP");
    let mut txlog_domain_data = Vec::new();
    txlog_domain_data.extend_from_slice(&(txlog_domain.len() as u32).to_le_bytes());
    txlog_domain_data.extend_from_slice(&txlog_domain);
    let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
        operation: 2,
        sequence_number: 200,
        timestamp: Some(0x01DB_A100_0000_0000),
        key_path: r"\Registry\Machine\SECURITY\Policy".to_string(),
        value_name: Some("PolPrDmS".to_string()),
        data_before: None,
        data_after: Some(txlog_domain_data),
    }]);

    let result = extract_security_policy_from_security_hive_with_txlog(
        &data,
        "",
        None,
        Some(&txlog_bytes),
        None,
    )
    .unwrap();
    assert_eq!(result.domain_name.as_deref(), Some("TXLOG-CORP"));
    assert_eq!(result.account_domain_name.as_deref(), Some("ACCT"));
    assert!(result.txlog_applied);
    assert!(result
        .txlog_timestamps
        .iter()
        .any(|ts| ts.field_name == "domainName"));
}

#[test]
fn extract_lsa_secrets_from_fixture() {
    let mut data = empty_hive("ROOT");
    write_nk(&mut data, 0x20, "ROOT", &[("Policy", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Policy", &[("Secrets", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Secrets", &[("$MACHINE.ACC", 0x400)], &[]);
    write_nk(&mut data, 0x400, "$MACHINE.ACC", &[], &[0x600, 0x680]);

    let filetime = 133_801_632_000_000_000u64;
    let secret_abs = super::super::BASE_BLOCK_SIZE + 0x400;
    data[secret_abs + 0x08..secret_abs + 0x10].copy_from_slice(&filetime.to_le_bytes());
    write_binary_value(
        &mut data,
        0x600,
        "CurrVal",
        &[0xAA, 0xBB, 0xCC, 0xDD],
        0x5000,
    );
    write_binary_value(
        &mut data,
        0x680,
        "BkupVal",
        &[0x11, 0x22, 0x33, 0x44, 0x55],
        0x5100,
    );

    let result = extract_lsa_secrets_from_security_hive(&data, "", None).unwrap();
    assert_eq!(result.len(), 2);
    let current = result
        .iter()
        .find(|entry| entry.version == "current")
        .unwrap();
    assert_eq!(current.secret_name, "$MACHINE.ACC");
    assert_eq!(current.encrypted_blob_hex, "aabbccdd");
    assert_eq!(current.source_key_path, r"Policy\Secrets\$MACHINE.ACC");
    assert_eq!(current.last_write, windows_filetime_to_rfc3339(filetime));
    let backup = result
        .iter()
        .find(|entry| entry.version == "backup")
        .unwrap();
    assert_eq!(backup.encrypted_blob_hex, "1122334455");
}

#[test]
fn extract_cached_credentials_from_fixture() {
    let mut data = empty_hive("ROOT");
    write_nk(&mut data, 0x20, "ROOT", &[("Security", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Security", &[("Cache", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Cache", &[], &[0x500, 0x580, 0x600]);

    let filetime = 133_801_632_000_000_000u64;
    let cache_abs = super::super::BASE_BLOCK_SIZE + 0x300;
    data[cache_abs + 0x08..cache_abs + 0x10].copy_from_slice(&filetime.to_le_bytes());
    write_binary_value(&mut data, 0x500, "NL$1", &[0x01, 0x02, 0x03, 0x04], 0x5000);
    write_binary_value(
        &mut data,
        0x580,
        "NL$2",
        &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE],
        0x5100,
    );
    write_binary_value(&mut data, 0x600, "Other", &[0xFF, 0xFF], 0x5200);

    let mut result = extract_cached_credentials_from_security_hive(&data, "", None).unwrap();
    result.sort_by(|a, b| a.entry_name.cmp(&b.entry_name));
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].entry_name, "NL$1");
    assert_eq!(result[0].encrypted_blob_hex, "01020304");
    assert_eq!(result[0].source_key_path, r"Security\Cache");
    assert_eq!(result[0].last_write, windows_filetime_to_rfc3339(filetime));
    assert_eq!(result[1].entry_name, "NL$2");
    assert_eq!(result[1].encrypted_blob_hex, "aabbccddee");
}
