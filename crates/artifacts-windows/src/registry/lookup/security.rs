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
#[path = "../../../tests/unit/registry/lookup/security.rs"]
mod tests;
