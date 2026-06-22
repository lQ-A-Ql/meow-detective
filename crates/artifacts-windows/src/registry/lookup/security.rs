use super::txlog_util::find_best_txlog_match;
use super::{
    windows_filetime_to_rfc3339, CachedCredentialEntry, LsaSecretEntry, RegistryHiveReader,
    RegistryValue, SecurityPolicyEntry, TxlogTimestampInfo,
};
use crate::registry::txlog::parse_and_merge_txlogs;
use crate::registry::RegistryError;

/// Extract non-sensitive local security policy metadata from the SECURITY hive.
pub fn extract_security_policy_from_security_hive(
    bytes: &[u8],
    _hive_path: &str,
    _boot_key: Option<[u8; 16]>,
) -> Result<SecurityPolicyEntry, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let policy_key = hive
        .navigate_to(&["Policy"])?
        .ok_or_else(|| "Policy key not found".to_string())?;

    let last_write = policy_key
        .last_write_time
        .and_then(windows_filetime_to_rfc3339);

    let domain_name = read_policy_string(&hive, &["Policy"], "PolPrDmS")?;
    let account_domain_name = read_policy_string(&hive, &["Policy"], "PolAcDmS")?;
    let machine_sid = read_machine_sid(&hive, &["Policy"])?;
    let audit_policy_hex = read_audit_hex(&hive, &["Policy"])?;

    Ok(SecurityPolicyEntry {
        domain_name,
        account_domain_name,
        machine_sid,
        audit_policy_hex,
        source_key_path: "Policy".to_string(),
        last_write,
        txlog_applied: false,
        txlog_timestamps: Vec::new(),
    })
}

/// Like [`extract_security_policy_from_security_hive`], but overlays newer
/// values from .LOG1/.LOG2 transaction-log entries before returning.
///
/// Corrupt or missing transaction logs are treated as non-fatal warnings; the
/// base hive result is still returned.
pub fn extract_security_policy_from_security_hive_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    boot_key: Option<[u8; 16]>,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
) -> Result<SecurityPolicyEntry, RegistryError> {
    let mut entry = extract_security_policy_from_security_hive(bytes, hive_path, boot_key)?;
    let (transactions, _txlog_warnings) = parse_and_merge_txlogs(txlog1, txlog2);
    if transactions.is_empty() {
        entry.txlog_applied = false;
        entry.txlog_timestamps = Vec::new();
        return Ok(entry);
    }

    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    if let Some(txn) = find_best_txlog_match(&transactions, r"Policy", "PolPrDmS") {
        if let Some(data) = txn.data_after.as_deref() {
            if let Some(new_value) = decode_policy_string(data) {
                if entry.domain_name.as_deref() != Some(new_value.as_str()) {
                    entry.domain_name = Some(new_value);
                    txlog_applied = true;
                }
            }
        }
        ts_infos.push(TxlogTimestampInfo {
            field_name: "domainName".to_string(),
            hive_timestamp: None,
            txlog_timestamp: txn.timestamp,
            txlog_used: txn.data_after.is_some(),
        });
    }

    if let Some(txn) = find_best_txlog_match(&transactions, r"Policy", "PolAcDmS") {
        if let Some(data) = txn.data_after.as_deref() {
            if let Some(new_value) = decode_policy_string(data) {
                if entry.account_domain_name.as_deref() != Some(new_value.as_str()) {
                    entry.account_domain_name = Some(new_value);
                    txlog_applied = true;
                }
            }
        }
        ts_infos.push(TxlogTimestampInfo {
            field_name: "accountDomainName".to_string(),
            hive_timestamp: None,
            txlog_timestamp: txn.timestamp,
            txlog_used: txn.data_after.is_some(),
        });
    }

    if let Some(txn) = find_best_txlog_match(&transactions, r"Policy", "PolMachineAccountS") {
        if let Some(data) = txn.data_after.as_deref() {
            if let Some(new_sid) = parse_sid_to_string(data) {
                if entry.machine_sid.as_deref() != Some(new_sid.as_str()) {
                    entry.machine_sid = Some(new_sid);
                    txlog_applied = true;
                }
            }
        }
        ts_infos.push(TxlogTimestampInfo {
            field_name: "machineSid".to_string(),
            hive_timestamp: None,
            txlog_timestamp: txn.timestamp,
            txlog_used: txn.data_after.is_some(),
        });
    }

    if let Some(txn) = find_best_txlog_match(&transactions, r"Policy", "PolAdtEv") {
        if let Some(data) = txn.data_after.as_deref() {
            let new_hex = hex::encode(data);
            if entry.audit_policy_hex.as_deref() != Some(new_hex.as_str()) {
                entry.audit_policy_hex = Some(new_hex);
                txlog_applied = true;
            }
        }
        ts_infos.push(TxlogTimestampInfo {
            field_name: "auditPolicyHex".to_string(),
            hive_timestamp: None,
            txlog_timestamp: txn.timestamp,
            txlog_used: txn.data_after.is_some(),
        });
    }

    entry.txlog_applied = txlog_applied;
    entry.txlog_timestamps = ts_infos;

    Ok(entry)
}

/// Enumerate LSA secrets under `Policy\Secrets` from the SECURITY hive.
/// Only secret names, version labels (current/backup), encrypted blob hex,
/// source key paths, and last-write timestamps are returned. Decryption is
/// intentionally not performed.
pub fn extract_lsa_secrets_from_security_hive(
    bytes: &[u8],
    _hive_path: &str,
    _boot_key: Option<[u8; 16]>,
) -> Result<Vec<LsaSecretEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let Some(secrets_key) = hive.navigate_to(&["Policy", "Secrets"])? else {
        return Ok(Vec::new());
    };

    let mut entries = Vec::new();
    for (secret_name, secret_nk) in hive.read_subkeys_from_nk(&secrets_key)? {
        let source_key_path = format!("Policy\\Secrets\\{}", secret_name);
        let last_write = secret_nk
            .last_write_time
            .and_then(windows_filetime_to_rfc3339);

        if let Some(blob) = hive.read_raw_value_bytes(&secret_nk, "CurrVal")? {
            entries.push(LsaSecretEntry {
                secret_name: secret_name.clone(),
                version: "current".to_string(),
                encrypted_blob_hex: hex::encode(&blob),
                source_key_path: source_key_path.clone(),
                last_write: last_write.clone(),
            });
        }
        if let Some(blob) = hive.read_raw_value_bytes(&secret_nk, "BkupVal")? {
            entries.push(LsaSecretEntry {
                secret_name: secret_name.clone(),
                version: "backup".to_string(),
                encrypted_blob_hex: hex::encode(&blob),
                source_key_path,
                last_write,
            });
        }
    }

    Ok(entries)
}

/// Enumerate cached domain credential entries under `Security\Cache` from the
/// SECURITY hive. Only entry names, encrypted blob hex, source key paths, and
/// last-write timestamps are returned. Decryption is intentionally not performed.
pub fn extract_cached_credentials_from_security_hive(
    bytes: &[u8],
    _hive_path: &str,
    _boot_key: Option<[u8; 16]>,
) -> Result<Vec<CachedCredentialEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let Some(cache_key) = hive.navigate_to(&["Security", "Cache"])? else {
        return Ok(Vec::new());
    };

    let last_write = cache_key
        .last_write_time
        .and_then(windows_filetime_to_rfc3339);
    let values = hive.read_all_values_from_nk(&cache_key)?;

    let mut entries = Vec::new();
    for (name, value) in values {
        if !is_cache_entry_name(&name) {
            continue;
        }
        if let RegistryValue::Binary(blob) = value {
            entries.push(CachedCredentialEntry {
                entry_name: name,
                encrypted_blob_hex: hex::encode(&blob),
                source_key_path: r"Security\Cache".to_string(),
                last_write: last_write.clone(),
            });
        }
    }
    Ok(entries)
}

fn is_cache_entry_name(name: &str) -> bool {
    name.starts_with("NL$") && name.len() > 3 && name[3..].chars().all(|c| c.is_ascii_digit())
}

fn read_policy_string(
    hive: &RegistryHiveReader<'_>,
    path: &[&str],
    value_name: &str,
) -> Result<Option<String>, String> {
    match hive.lookup_value(path, value_name)? {
        Some(RegistryValue::String(value)) => Ok(Some(value)),
        Some(RegistryValue::Binary(data)) => Ok(decode_policy_string(&data)),
        _ => Ok(None),
    }
}

/// Decode a UTF-16LE null-terminated string from a policy value blob.
/// Defensively skips a 4-byte length header when the leading DWORD is a
/// plausible byte count for the remaining (even-length) payload.
pub(crate) fn decode_policy_string(data: &[u8]) -> Option<String> {
    if data.len() < 2 {
        return None;
    }

    let payload = if data.len() >= 4 {
        let header = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let remainder = data.len().saturating_sub(4);
        if header <= data.len() && remainder.is_multiple_of(2) && header > 0 {
            &data[4..]
        } else {
            data
        }
    } else {
        data
    };

    let mut units = Vec::with_capacity(payload.len() / 2);
    for chunk in payload.chunks(2) {
        if chunk.len() < 2 {
            break;
        }
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }

    if units.is_empty() {
        return None;
    }
    Some(String::from_utf16_lossy(&units))
}

fn read_machine_sid(
    hive: &RegistryHiveReader<'_>,
    path: &[&str],
) -> Result<Option<String>, String> {
    match hive.lookup_value(path, "PolMachineAccountS")? {
        Some(RegistryValue::Binary(data)) => Ok(parse_sid_to_string(&data)),
        _ => Ok(None),
    }
}

/// Parse a raw Windows SID binary into the standard `S-1-5-...` text form.
pub(crate) fn parse_sid_to_string(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }
    let revision = data[0];
    let sub_authority_count = data[1] as usize;
    let expected_len = 8 + sub_authority_count * 4;
    if data.len() < expected_len {
        return None;
    }

    let identifier_authority = ((data[2] as u64) << 40)
        | ((data[3] as u64) << 32)
        | ((data[4] as u64) << 24)
        | ((data[5] as u64) << 16)
        | ((data[6] as u64) << 8)
        | (data[7] as u64);

    let mut result = format!("S-{}-{}", revision, identifier_authority);
    for index in 0..sub_authority_count {
        let offset = 8 + index * 4;
        let sub = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        result.push_str(&format!("-{}", sub));
    }
    Some(result)
}

fn read_audit_hex(hive: &RegistryHiveReader<'_>, path: &[&str]) -> Result<Option<String>, String> {
    match hive.lookup_value(path, "PolAdtEv")? {
        Some(RegistryValue::Binary(data)) => Ok(Some(hex::encode(&data))),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_common::*;
    use super::*;
    use crate::registry::txlog::fixture::{build_synthetic_log1, SyntheticEntry};

    #[test]
    fn extract_security_policy_from_fixture() {
        let mut data = empty_hive("ROOT");
        // ROOT -> Policy
        write_nk(&mut data, 0x20, "ROOT", &[("Policy", 0x200)], &[]);
        // Policy key with the four security policy values (spaced 0x80 apart to
        // avoid overlapping the default 128-byte VK cells).
        write_nk(
            &mut data,
            0x200,
            "Policy",
            &[],
            &[0x400, 0x480, 0x500, 0x580],
        );

        // Set a known last-write FILETIME on the Policy key (2025-01-01T00:00:00Z)
        let filetime = 133_801_632_000_000_000u64;
        let policy_abs = super::super::BASE_BLOCK_SIZE + 0x200;
        data[policy_abs + 0x08..policy_abs + 0x10].copy_from_slice(&filetime.to_le_bytes());

        // PolPrDmS with a 4-byte length header followed by the UTF-16LE domain name
        let domain = encode_utf16le("CORP");
        let mut domain_data = Vec::new();
        domain_data.extend_from_slice(&(domain.len() as u32).to_le_bytes());
        domain_data.extend_from_slice(&domain);
        write_binary_value(&mut data, 0x400, "PolPrDmS", &domain_data, 0x5000);

        // PolAcDmS without a length header (whole blob is UTF-16LE)
        let account = encode_utf16le("ACCT");
        write_binary_value(&mut data, 0x480, "PolAcDmS", &account, 0x5100);

        // PolMachineAccountS: machine SID binary
        let sid = make_sid(&[21, 123_456_789, 123_456_789, 123_456_789, 1000]);
        write_binary_value(&mut data, 0x500, "PolMachineAccountS", &sid, 0x5200);

        // PolAdtEv: raw audit policy binary blob
        let audit = vec![0x01, 0x02, 0x03, 0x04];
        write_binary_value(&mut data, 0x580, "PolAdtEv", &audit, 0x5300);

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
        let data = empty_hive("ROOT");
        let result = extract_security_policy_from_security_hive(&data, "", None);
        assert!(result.is_err());
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

        // Base domain name is CORP.
        let domain = encode_utf16le("CORP");
        let mut domain_data = Vec::new();
        domain_data.extend_from_slice(&(domain.len() as u32).to_le_bytes());
        domain_data.extend_from_slice(&domain);
        write_binary_value(&mut data, 0x400, "PolPrDmS", &domain_data, 0x5000);

        let account = encode_utf16le("ACCT");
        write_binary_value(&mut data, 0x480, "PolAcDmS", &account, 0x5100);
        let sid = make_sid(&[21, 123_456_789, 123_456_789, 123_456_789, 1000]);
        write_binary_value(&mut data, 0x500, "PolMachineAccountS", &sid, 0x5200);
        let audit = vec![0x01, 0x02, 0x03, 0x04];
        write_binary_value(&mut data, 0x580, "PolAdtEv", &audit, 0x5300);

        // Txlog overrides PolPrDmS to TXLOG-CORP.
        let txlog_domain = encode_utf16le("TXLOG-CORP");
        let mut txlog_domain_data = Vec::new();
        txlog_domain_data.extend_from_slice(&(txlog_domain.len() as u32).to_le_bytes());
        txlog_domain_data.extend_from_slice(&txlog_domain);
        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
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
        assert!(
            result
                .txlog_timestamps
                .iter()
                .any(|ts| ts.field_name == "domainName"),
            "missing txlog timestamp for domain name: {:?}",
            result.txlog_timestamps
        );
    }

    #[test]
    fn extract_lsa_secrets_from_fixture() {
        let mut data = empty_hive("ROOT");
        // ROOT -> Policy -> Secrets -> $MACHINE.ACC
        write_nk(&mut data, 0x20, "ROOT", &[("Policy", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Policy", &[("Secrets", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Secrets", &[("$MACHINE.ACC", 0x400)], &[]);
        write_nk(&mut data, 0x400, "$MACHINE.ACC", &[], &[0x600, 0x680]);

        // Set a known last-write FILETIME on the secret key (2025-01-01T00:00:00Z)
        let filetime = 133_801_632_000_000_000u64;
        let secret_abs = super::super::BASE_BLOCK_SIZE + 0x400;
        data[secret_abs + 0x08..secret_abs + 0x10].copy_from_slice(&filetime.to_le_bytes());

        let current_blob = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let backup_blob = vec![0x11, 0x22, 0x33, 0x44, 0x55];
        write_binary_value(&mut data, 0x600, "CurrVal", &current_blob, 0x5000);
        write_binary_value(&mut data, 0x680, "BkupVal", &backup_blob, 0x5100);

        let result = extract_lsa_secrets_from_security_hive(&data, "", None).unwrap();
        assert_eq!(result.len(), 2);

        let current = result
            .iter()
            .find(|entry| entry.version == "current")
            .expect("current entry");
        assert_eq!(current.secret_name, "$MACHINE.ACC");
        assert_eq!(current.encrypted_blob_hex, "aabbccdd");
        assert_eq!(current.source_key_path, r"Policy\Secrets\$MACHINE.ACC");
        assert_eq!(current.last_write, windows_filetime_to_rfc3339(filetime));

        let backup = result
            .iter()
            .find(|entry| entry.version == "backup")
            .expect("backup entry");
        assert_eq!(backup.secret_name, "$MACHINE.ACC");
        assert_eq!(backup.encrypted_blob_hex, "1122334455");
        assert_eq!(backup.source_key_path, r"Policy\Secrets\$MACHINE.ACC");
        assert_eq!(backup.last_write, windows_filetime_to_rfc3339(filetime));
    }

    #[test]
    fn extract_cached_credentials_from_fixture() {
        let mut data = empty_hive("ROOT");
        // ROOT -> Security -> Cache
        write_nk(&mut data, 0x20, "ROOT", &[("Security", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Security", &[("Cache", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Cache", &[], &[0x500, 0x580, 0x600]);

        // Set a known last-write FILETIME on the Cache key (2025-01-01T00:00:00Z)
        let filetime = 133_801_632_000_000_000u64;
        let cache_abs = super::super::BASE_BLOCK_SIZE + 0x300;
        data[cache_abs + 0x08..cache_abs + 0x10].copy_from_slice(&filetime.to_le_bytes());

        let blob1 = vec![0x01, 0x02, 0x03, 0x04];
        let blob2 = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        let other_blob = vec![0xFF, 0xFF];
        write_binary_value(&mut data, 0x500, "NL$1", &blob1, 0x5000);
        write_binary_value(&mut data, 0x580, "NL$2", &blob2, 0x5100);
        write_binary_value(&mut data, 0x600, "Other", &other_blob, 0x5200);

        let mut result = extract_cached_credentials_from_security_hive(&data, "", None).unwrap();
        result.sort_by(|a, b| a.entry_name.cmp(&b.entry_name));
        assert_eq!(result.len(), 2);

        let first = &result[0];
        assert_eq!(first.entry_name, "NL$1");
        assert_eq!(first.encrypted_blob_hex, "01020304");
        assert_eq!(first.source_key_path, r"Security\Cache");
        assert_eq!(first.last_write, windows_filetime_to_rfc3339(filetime));

        let second = &result[1];
        assert_eq!(second.entry_name, "NL$2");
        assert_eq!(second.encrypted_blob_hex, "aabbccddee");
        assert_eq!(second.source_key_path, r"Security\Cache");
        assert_eq!(second.last_write, windows_filetime_to_rfc3339(filetime));
    }
}
