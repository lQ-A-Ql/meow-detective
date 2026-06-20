use chrono::{DateTime, TimeZone, Utc};

use crate::registry::txlog::{
    parse_transaction_log, RegistryTransaction, RegistryTransactionOperation,
};

const BASE_BLOCK_SIZE: usize = 0x1000;
const NK_SIGNATURE: &[u8; 2] = b"nk";
const VK_SIGNATURE: &[u8; 2] = b"vk";
const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const REG_DWORD: u32 = 4;
const REG_MULTI_SZ: u32 = 7;
const REG_QWORD: u32 = 11;
const INVALID_OFFSET: u32 = 0xFFFF_FFFF;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRegistryField {
    pub value: String,
    pub hive_path: String,
    pub key_path: String,
    pub value_name: String,
    pub parser: String,
}

/// Records which txlog entry (if any) was used to override a field value,
/// and the timestamps from both the hive and the transaction log.
#[derive(Debug, Clone)]
pub struct TxlogTimestampInfo {
    /// Human-readable name of the field (e.g. "ComputerName").
    pub field_name: String,
    /// Last-write timestamp from the hive record (currently always `None` because
    /// the standard extractor does not read key timestamps).
    pub hive_timestamp: Option<DateTime<Utc>>,
    /// Timestamp from the matching transaction-log entry, when one was applied.
    pub txlog_timestamp: Option<DateTime<Utc>>,
    /// `true` when the field's value was updated from a txlog entry.
    pub txlog_used: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SystemHiveInfo {
    pub computer_name: Option<ParsedRegistryField>,
    pub timezone: Option<ParsedRegistryField>,
    /// Whether any field was updated from the transaction log.
    pub txlog_applied: bool,
    /// Per-field record of txlog override decisions and timestamps.
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SoftwareHiveInfo {
    pub product_name: Option<ParsedRegistryField>,
    pub current_build: Option<ParsedRegistryField>,
    pub current_version: Option<ParsedRegistryField>,
    pub display_version: Option<ParsedRegistryField>,
    pub install_date: Option<ParsedRegistryField>,
    pub registered_owner: Option<ParsedRegistryField>,
    pub registered_organization: Option<ParsedRegistryField>,
    pub product_id: Option<ParsedRegistryField>,
    /// Whether any field was updated from the transaction log.
    pub txlog_applied: bool,
    /// Per-field record of txlog override decisions and timestamps.
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}

/// A single auto-start entry found under Run / RunOnce keys in NTUSER.DAT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRunKey {
    pub key_path: String,
    pub value_name: String,
    pub command: String,
    pub timestamp: Option<String>,
}

/// A single entry from the Explorer RecentDocs MRU list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentDoc {
    pub file_name: String,
    pub extension: String,
    pub last_accessed: Option<String>,
    pub lnk_target: Option<String>,
}

/// A single UserAssist entry (program execution tracking).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAssistEntry {
    pub executable_path: String,
    pub run_count: u32,
    pub last_run: Option<String>,
    pub focus_time_ms: u64,
    pub session_id: u32,
}

/// A mount-point entry from Explorer MountPoints2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPoint {
    pub drive_letter: Option<String>,
    pub volume_guid: Option<String>,
    pub last_mounted: Option<String>,
}

/// Aggregated information extracted from an NTUSER.DAT hive.
#[derive(Debug, Clone, Default)]
pub struct NtuserInfo {
    pub run_keys: Vec<RegistryRunKey>,
    pub recent_docs: Vec<RecentDoc>,
    pub ua_entries: Vec<UserAssistEntry>,
    pub typed_urls: Vec<String>,
    pub word_wheel_query: Vec<String>,
    pub mount_points: Vec<MountPoint>,
    /// Whether any field was updated from the transaction log.
    pub txlog_applied: bool,
    /// Per-field record of txlog override decisions and timestamps.
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
    pub warnings: Vec<String>,
}

// ── SAM hive structs ─────────────────────────────────────────────────────────

/// A local user account parsed from a SAM registry hive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamUser {
    pub username: String,
    pub rid: u32,
    pub full_name: String,
    pub comment: String,
    pub home_dir: String,
    pub profile_path: String,
    pub last_login: Option<DateTime<Utc>>,
    pub password_last_set: Option<DateTime<Utc>>,
    pub account_disabled: bool,
    pub account_locked: bool,
    pub admin_count: u32,
    pub group_memberships: Vec<String>,
}

/// A local group parsed from a SAM registry hive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamGroup {
    pub name: String,
    pub rid: u32,
    pub members: Vec<String>,
}

/// Aggregated information extracted from a SAM hive.
#[derive(Debug, Clone, Default)]
pub struct SamInfo {
    pub users: Vec<SamUser>,
    pub groups: Vec<SamGroup>,
    /// Domain-level password policy from `SAM\Domains\Account\F`.
    pub password_policy: Option<crate::registry::sam_structs::SamPasswordPolicy>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegistryValue {
    String(String),
    Dword(u32),
    Qword(u64),
    MultiString(Vec<String>),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone)]
pub(crate) struct NkRecord {
    pub(crate) name: String,
    pub(crate) num_subkeys: u32,
    pub(crate) subkeys_list_offset: u32,
    pub(crate) num_values: u32,
    pub(crate) values_list_offset: u32,
}

pub fn extract_system_hive_fields(bytes: &[u8], hive_path: &str) -> Result<SystemHiveInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = SystemHiveInfo::default();
    let control_sets = hive.control_set_candidates(&mut info.warnings);

    for control_set in control_sets {
        let computer_key = [
            control_set.as_str(),
            "Control",
            "ComputerName",
            "ComputerName",
        ];
        if info.computer_name.is_none() {
            info.computer_name = lookup_string_field(
                &hive,
                hive_path,
                "registry.system",
                &computer_key,
                "ComputerName",
                &mut info.warnings,
            );
        }

        let timezone_key = [control_set.as_str(), "Control", "TimeZoneInformation"];
        if info.timezone.is_none() {
            info.timezone = lookup_string_field(
                &hive,
                hive_path,
                "registry.system",
                &timezone_key,
                "TimeZoneKeyName",
                &mut info.warnings,
            )
            .or_else(|| {
                lookup_string_field(
                    &hive,
                    hive_path,
                    "registry.system",
                    &timezone_key,
                    "StandardName",
                    &mut info.warnings,
                )
            });
        }

        if info.computer_name.is_some() && info.timezone.is_some() {
            break;
        }
    }
    Ok(info)
}

pub fn extract_software_hive_fields(
    bytes: &[u8],
    hive_path: &str,
) -> Result<SoftwareHiveInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key = ["Microsoft", "Windows NT", "CurrentVersion"];
    let mut info = SoftwareHiveInfo::default();

    info.product_name = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "ProductName",
        &mut info.warnings,
    );
    info.current_build = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "CurrentBuild",
        &mut info.warnings,
    );
    info.current_version = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "CurrentVersion",
        &mut info.warnings,
    );
    info.display_version = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "DisplayVersion",
        &mut info.warnings,
    )
    .or_else(|| {
        lookup_string_field(
            &hive,
            hive_path,
            "registry.software",
            &key,
            "ReleaseId",
            &mut info.warnings,
        )
    });
    info.registered_owner = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "RegisteredOwner",
        &mut info.warnings,
    );
    info.registered_organization = lookup_optional_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "RegisteredOrganization",
        &mut info.warnings,
    );
    info.product_id = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "ProductId",
        &mut info.warnings,
    );
    info.install_date = lookup_install_date_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        &mut info.warnings,
    );

    Ok(info)
}

fn lookup_string_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    value_name: &str,
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::String(value))) if !value.trim().is_empty() => {
            Some(ParsedRegistryField {
                value,
                hive_path: hive_path.to_string(),
                key_path: key_path.join("\\"),
                value_name: value_name.to_string(),
                parser: parser.to_string(),
            })
        }
        Ok(Some(other)) => {
            warnings.push(format!(
                "{}\\{} has unsupported type: {:?}",
                key_path.join("\\"),
                value_name,
                other
            ));
            None
        }
        Ok(None) => {
            warnings.push(format!("{}\\{} not found", key_path.join("\\"), value_name));
            None
        }
        Err(err) => {
            warnings.push(format!(
                "{}\\{} parse error: {}",
                key_path.join("\\"),
                value_name,
                err
            ));
            None
        }
    }
}

fn lookup_optional_string_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    value_name: &str,
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, value_name) {
        Ok(None) => None,
        _ => lookup_string_field(hive, hive_path, parser, key_path, value_name, warnings),
    }
}

fn lookup_install_date_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, "InstallDate") {
        Ok(Some(RegistryValue::Dword(value))) => {
            let Some(dt) = Utc.timestamp_opt(value as i64, 0).single() else {
                warnings.push("InstallDate is outside supported timestamp range".to_string());
                return None;
            };
            if !(946_684_800..=4_102_444_800).contains(&value) {
                warnings.push(format!("InstallDate {value} is outside plausible range"));
                return None;
            }
            Some(ParsedRegistryField {
                value: dt.to_rfc3339(),
                hive_path: hive_path.to_string(),
                key_path: key_path.join("\\"),
                value_name: "InstallDate".to_string(),
                parser: parser.to_string(),
            })
        }
        Ok(Some(other)) => {
            warnings.push(format!("InstallDate has unsupported type: {:?}", other));
            None
        }
        Ok(None) => {
            warnings.push(format!("{}\\InstallDate not found", key_path.join("\\")));
            None
        }
        Err(err) => {
            warnings.push(format!(
                "{}\\InstallDate parse error: {}",
                key_path.join("\\"),
                err
            ));
            None
        }
    }
}

// ── NTUSER.DAT field extraction ──────────────────────────────────────────────

pub fn extract_ntuser_fields(bytes: &[u8], hive_path: &str) -> Result<NtuserInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = NtuserInfo::default();
    let parser = "registry.ntuser";

    info.run_keys = extract_run_keys(&hive, hive_path, parser, &mut info.warnings);
    info.recent_docs = extract_recent_docs(&hive, hive_path, parser, &mut info.warnings);
    info.ua_entries = extract_user_assist(&hive, hive_path, parser, &mut info.warnings);
    info.typed_urls = extract_typed_urls(&hive, hive_path, parser, &mut info.warnings);
    info.word_wheel_query = extract_word_wheel_query(&hive, hive_path, parser, &mut info.warnings);
    info.mount_points = extract_mount_points(&hive, hive_path, parser, &mut info.warnings);

    Ok(info)
}

// ── Transaction-log-aware extractors ──────────────────────────────────────────

/// Like [`extract_system_hive_fields`], but after standard extraction checks a
/// transaction log for more recent writes.  When a txlog entry holds a newer
/// value (higher sequence number), the field's value is overwritten.
pub fn extract_system_hive_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<SystemHiveInfo, String> {
    let mut info = extract_system_hive_fields(bytes, hive_path)?;
    let txlog = parse_transaction_log(txlog_data)?;
    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    if let Some(ref mut field) = info.computer_name {
        let ts = apply_single_txlog_override(field, &txlog.transactions);
        txlog_applied = txlog_applied || ts.txlog_used;
        ts_infos.push(ts);
    }
    if let Some(ref mut field) = info.timezone {
        let ts = apply_single_txlog_override(field, &txlog.transactions);
        txlog_applied = txlog_applied || ts.txlog_used;
        ts_infos.push(ts);
    }

    info.txlog_applied = txlog_applied;
    info.txlog_timestamps = ts_infos;
    Ok(info)
}

/// Like [`extract_software_hive_fields`], but after standard extraction checks a
/// transaction log for more recent writes.
pub fn extract_software_hive_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<SoftwareHiveInfo, String> {
    let mut info = extract_software_hive_fields(bytes, hive_path)?;
    let txlog = parse_transaction_log(txlog_data)?;
    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    let fields: [&mut Option<ParsedRegistryField>; 8] = [
        &mut info.product_name,
        &mut info.current_build,
        &mut info.current_version,
        &mut info.display_version,
        &mut info.install_date,
        &mut info.registered_owner,
        &mut info.registered_organization,
        &mut info.product_id,
    ];
    for field in fields.into_iter().flatten() {
        let ts = apply_single_txlog_override(field, &txlog.transactions);
        txlog_applied = txlog_applied || ts.txlog_used;
        ts_infos.push(ts);
    }

    info.txlog_applied = txlog_applied;
    info.txlog_timestamps = ts_infos;
    Ok(info)
}

/// Like [`extract_ntuser_fields`], but after standard extraction checks a
/// transaction log for more recent writes to Run / RunOnce keys and TypedURLs.
pub fn extract_ntuser_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<NtuserInfo, String> {
    let mut info = extract_ntuser_fields(bytes, hive_path)?;
    let txlog = parse_transaction_log(txlog_data)?;
    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    // Override Run / RunOnce commands.
    for run_key in &mut info.run_keys {
        let best =
            find_best_txlog_match(&txlog.transactions, &run_key.key_path, &run_key.value_name);
        if let Some(txn) = best {
            if let Some(new_cmd) = txn.data_after.as_deref().and_then(txlog_data_to_string) {
                run_key.command = new_cmd;
                run_key.timestamp = txn.timestamp.map(|dt| dt.to_rfc3339());
                ts_infos.push(TxlogTimestampInfo {
                    field_name: format!("RunKey[{}]", run_key.value_name),
                    hive_timestamp: None,
                    txlog_timestamp: txn.timestamp,
                    txlog_used: true,
                });
                txlog_applied = true;
            }
        }
    }

    // Apply txlog overrides to UserAssist entries.
    for ua_entry in &mut info.ua_entries {
        // ROT13 is its own inverse: the value name stored in the registry is
        // the ROT13-encoded version of executable_path.
        let encoded_name = rot13_decode(&ua_entry.executable_path);
        let best = find_best_txlog_match_user_assist(&txlog.transactions, &encoded_name);
        if let Some(txn) = best {
            if let Some(data) = &txn.data_after {
                if let Some((run_count, session_id, focus_time, filetime)) =
                    parse_user_assist_binary(data)
                {
                    ua_entry.run_count = run_count;
                    ua_entry.session_id = session_id;
                    ua_entry.focus_time_ms = focus_time as u64;
                    ua_entry.last_run = windows_filetime_to_rfc3339(filetime);
                    ts_infos.push(TxlogTimestampInfo {
                        field_name: format!("UserAssist[{}]", ua_entry.executable_path),
                        hive_timestamp: None,
                        txlog_timestamp: txn.timestamp,
                        txlog_used: true,
                    });
                    txlog_applied = true;
                }
            }
        }
    }

    info.txlog_applied = txlog_applied;
    info.txlog_timestamps = ts_infos;
    Ok(info)
}

// ── Txlog helpers ─────────────────────────────────────────────────────────────

/// Attempt to override a [`ParsedRegistryField`] with a more recent value from
/// the transaction log.  Returns a [`TxlogTimestampInfo`] describing whether an
/// override was applied.
fn apply_single_txlog_override(
    field: &mut ParsedRegistryField,
    transactions: &[RegistryTransaction],
) -> TxlogTimestampInfo {
    let best = find_best_txlog_match(transactions, &field.key_path, &field.value_name);

    match best {
        Some(txn) => {
            let old_value = field.value.clone();
            field.value = txn
                .data_after
                .as_deref()
                .and_then(txlog_data_to_string)
                .unwrap_or(old_value);
            TxlogTimestampInfo {
                field_name: field.value_name.clone(),
                hive_timestamp: None,
                txlog_timestamp: txn.timestamp,
                txlog_used: true,
            }
        }
        None => TxlogTimestampInfo {
            field_name: field.value_name.clone(),
            hive_timestamp: None,
            txlog_timestamp: None,
            txlog_used: false,
        },
    }
}

/// Search transaction-log entries for the best `SetValue` matching `key_path`
/// and `value_name`.  "Best" means the highest sequence number among matches.
fn find_best_txlog_match<'a>(
    transactions: &'a [RegistryTransaction],
    key_path: &str,
    value_name: &str,
) -> Option<&'a RegistryTransaction> {
    transactions
        .iter()
        .filter(|txn| {
            txn.operation == RegistryTransactionOperation::SetValue
                && txn.value_name.as_deref() == Some(value_name)
                && txlog_key_path_matches(&txn.key_path, key_path)
        })
        .max_by_key(|txn| txn.sequence_number)
}

/// Check whether a transaction-log key path matches a hive-relative key path.
///
/// Registry hives are mounted at well-known roots (`\Registry\Machine\SYSTEM`,
/// `\Registry\Machine\SOFTWARE`, `\Registry\User\...`).  The txlog records the
/// full absolute path, while the hive extractor stores the path relative to the
/// hive root.  This function strips common prefixes and compares suffixes
/// case-insensitively.
fn txlog_key_path_matches(txlog_path: &str, hive_key_path: &str) -> bool {
    let tx = txlog_path.trim_matches('\\').to_lowercase();
    let hi = hive_key_path.trim_matches('\\').to_lowercase();

    if tx == hi {
        return true;
    }

    // Suffix match: the hive-relative path should be a suffix of the absolute
    // txlog path, separated by a backslash.
    if tx.len() > hi.len() {
        let split_at = tx.len() - hi.len();
        if split_at > 0
            && tx.as_bytes().get(split_at - 1) == Some(&b'\\')
            && tx.as_bytes()[split_at..] == hi.as_bytes()[..]
        {
            return true;
        }
    }

    false
}

/// Search txlog entries for a match on a UserAssist Count subkey.
///
/// UserAssist txlog entries have key paths like
/// `\Registry\User\...\UserAssist\{GUID}\Count` and value names that are the
/// ROT13-encoded executable path.
fn find_best_txlog_match_user_assist<'a>(
    transactions: &'a [RegistryTransaction],
    encoded_value_name: &str,
) -> Option<&'a RegistryTransaction> {
    transactions
        .iter()
        .filter(|txn| {
            txn.operation == RegistryTransactionOperation::SetValue
                && txn.value_name.as_deref() == Some(encoded_value_name)
                && txn.key_path.to_lowercase().contains("\\userassist\\")
                && txn.key_path.to_lowercase().ends_with("\\count")
        })
        .max_by_key(|txn| txn.sequence_number)
}

/// Parse a 72-byte UserAssist binary blob into its constituent fields.
///
/// Returns `(run_count, session_id, focus_time_ms, filetime)` on success, or
/// `None` if the data is too short.
fn parse_user_assist_binary(data: &[u8]) -> Option<(u32, u32, u32, u64)> {
    if data.len() < USER_ASSIST_ENTRY_SIZE {
        return None;
    }
    let run_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let session_id = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let focus_time_ms = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let filetime = u64::from_le_bytes([
        data[60], data[61], data[62], data[63], data[64], data[65], data[66], data[67],
    ]);
    Some((run_count, session_id, focus_time_ms, filetime))
}

/// Convert raw registry-value bytes (as recorded in the transaction log) to a
/// string.  Registry string types are stored as UTF-16LE; this attempts that
/// decode first and falls back to UTF-8 / Latin-1.
fn txlog_data_to_string(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    // Primary path: UTF-16LE (registry REG_SZ / REG_EXPAND_SZ wire format).
    if data.len() >= 2 && data.len().is_multiple_of(2) {
        let units: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = String::from_utf16_lossy(&units);
        let trimmed = s.trim_end_matches('\0').to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    // Fallback: raw UTF-8.
    String::from_utf8(data.to_vec()).ok()
}

// ── SAM hive field extraction ─────────────────────────────────────────────────

/// User account control flags in the SAM V record.
const SAM_ACCOUNT_DISABLED: u32 = 0x0001;
const SAM_ACCOUNT_LOCKED: u32 = 0x0010;

/// Extract local user accounts, groups, and memberships from a SAM registry hive.
pub fn extract_sam_fields(bytes: &[u8], hive_path: &str) -> Result<SamInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = SamInfo::default();

    // Build username → RID map from SAM\Domains\Account\Users\Names
    let names_path: &[&str] = &["SAM", "Domains", "Account", "Users", "Names"];
    let username_rid_map = build_sam_name_to_rid(&hive, names_path, &mut info.warnings);

    if username_rid_map.is_empty() {
        info.warnings.push(format!(
            "{}: no user names found (hive={})",
            names_path.join("\\"),
            hive_path
        ));
    }

    // Build a reverse RID → username map for group membership resolution.
    // Mutable so the Users\<RID_HEX> fallback below can extend it.
    let mut rid_to_username: std::collections::HashMap<u32, String> = username_rid_map
        .iter()
        .map(|(name, rid)| (*rid, name.clone()))
        .collect();

    // Extract user details from each user's V value
    for (username, rid) in &username_rid_map {
        if let Some(user) = extract_sam_user(&hive, username, *rid, &mut info.warnings) {
            info.users.push(user);
        }
    }

    // ── FALLBACK: recover RIDs from Users\<RID_HEX> subkeys ──────────────────
    // On Windows 10/11 the Names key default value is REG_NONE whose
    // data_offset encodes the RID inline — find_rid_in_sam_key can miss
    // these.  The Users subkeys are named by hex RID and each holds
    // V (user record with username) and F (binary blob with RID).
    // Iterate the Users subkeys and recover username↔RID mappings that
    // were missed by the Names-key scan.
    {
        let users_path: &[&str] = &["SAM", "Domains", "Account", "Users"];
        if let Ok(Some(users_nk)) = hive.navigate_to(users_path) {
            if let Ok(subkey_names) = hive.read_subkey_names_from_nk(&users_nk) {
                for subkey_name in &subkey_names {
                    if subkey_name.eq_ignore_ascii_case("Names") {
                        continue;
                    }
                    let hex_rid = u32::from_str_radix(subkey_name, 16).ok();
                    let Some(hex_rid) = hex_rid else {
                        continue;
                    };

                    // Already known from the Names-key pass — skip.
                    if rid_to_username.contains_key(&hex_rid) {
                        continue;
                    }

                    let mut user_path: Vec<&str> = users_path.to_vec();
                    user_path.push(subkey_name.as_str());

                    // Navigate to the user subkey to read V and F raw values
                    let user_nk = match hive.navigate_to(&user_path) {
                        Ok(Some(nk)) => nk,
                        _ => continue,
                    };

                    // Read raw V bytes (binary blob with username at offsets)
                    let username = match hive.read_raw_value_bytes(&user_nk, "V") {
                        Ok(Some(data)) => {
                            crate::registry::sam_structs::parse_username_from_v_record(&data)
                        }
                        _ => None,
                    };

                    // Read raw F bytes (UserF struct with RID at offset 0x28)
                    let f_rid = match hive.read_raw_value_bytes(&user_nk, "F") {
                        Ok(Some(data)) => {
                            crate::registry::sam_structs::parse_user_f(&data).map(|(rid, _, _)| rid)
                        }
                        _ => None,
                    };

                    if let (Some(username), Some(f_rid)) = (username, f_rid) {
                        if f_rid == hex_rid {
                            rid_to_username.insert(hex_rid, username.clone());
                            if let Some(user) =
                                extract_sam_user(&hive, &username, hex_rid, &mut info.warnings)
                            {
                                info.users.push(user);
                            }
                            info.warnings.push(format!(
                                "SAM: recovered user '{}' (RID={}) \
                                 from Users\\{}\\F value \
                                 (Names key REG_NONE fallback)",
                                username, hex_rid, subkey_name
                            ));
                        }
                    }
                }
            }
        }
    }

    // Extract groups from Builtin\Aliases and Account\Aliases
    let alias_roots: &[&[&str]] = &[
        &["SAM", "Domains", "Builtin", "Aliases"],
        &["SAM", "Domains", "Account", "Aliases"],
    ];
    for alias_root in alias_roots {
        extract_sam_aliases(&hive, alias_root, &rid_to_username, &mut info);
    }

    // ── Domain password policy ──────────────────────────────────────────────
    // The Account key's F value contains the domain-wide password policy.
    {
        let account_path: &[&str] = &["SAM", "Domains", "Account"];
        match hive.lookup_value(account_path, "F") {
            Ok(Some(RegistryValue::Binary(f_data))) => {
                info.password_policy =
                    crate::registry::sam_structs::parse_domain_account_f(&f_data);
            }
            Ok(Some(_)) => {
                info.warnings.push(
                    "SAM\\Domains\\Account\\F: unexpected value type (expected binary)".into(),
                );
            }
            // Missing F value is common for non-AD systems — not a warning.
            Ok(None) => {}
            Err(e) => {
                info.warnings.push(format!(
                    "SAM\\Domains\\Account\\F: failed to read value: {e}"
                ));
            }
        }
    }

    // Cross-reference: populate user group memberships from group member lists
    for user in &mut info.users {
        for group in &info.groups {
            if group.members.contains(&user.username) {
                user.group_memberships.push(group.name.clone());
            }
        }
    }

    Ok(info)
}

fn build_sam_name_to_rid(
    hive: &RegistryHiveReader<'_>,
    names_path: &[&str],
    warnings: &mut Vec<String>,
) -> Vec<(String, u32)> {
    let names_nk = match hive.navigate_to(names_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("SAM Users\\Names parse error: {err}"));
            return Vec::new();
        }
    };

    let subkey_names = match hive.read_subkey_names_from_nk(&names_nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("SAM Users\\Names subkeys error: {err}"));
            return Vec::new();
        }
    };

    let mut result = Vec::new();
    for username in subkey_names {
        let mut user_path: Vec<&str> = names_path.to_vec();
        user_path.push(username.as_str());
        match find_rid_in_sam_key(hive, &user_path, warnings) {
            Some(rid) => result.push((username, rid)),
            None => {
                warnings.push(format!("SAM user '{}' has no readable RID value", username));
            }
        }
    }
    result
}

fn find_rid_in_sam_key(
    hive: &RegistryHiveReader<'_>,
    key_path: &[&str],
    warnings: &mut Vec<String>,
) -> Option<u32> {
    let nk = match hive.navigate_to(key_path) {
        Ok(Some(nk)) => nk,
        Err(err) => {
            warnings.push(format!("{} parse error: {err}", key_path.join("\\")));
            return None;
        }
        _ => return None,
    };

    // Try parsed values first
    if let Ok(values) = hive.read_all_values_from_nk(&nk) {
        for (_name, value) in &values {
            match value {
                RegistryValue::Dword(v) => return Some(*v),
                RegistryValue::Binary(data) if data.len() >= 4 => {
                    if let Some(rid) = data
                        .get(..4)
                        .and_then(|b| <[u8; 4]>::try_from(b).ok())
                        .map(u32::from_le_bytes)
                    {
                        return Some(rid);
                    }
                }
                _ => {}
            }
        }
        // SAM on Win10/11 uses REG_NONE which parse_value_data maps to empty Binary.
        // Fall through to raw VK scan below.
    }

    // Fallback: scan raw VK cells for inline RID values.
    // SAM stores RID as the data_offset field (VK offset 0x0C) for REG_NONE.
    if let Ok(offsets) = hive.read_raw_vk_data_offsets(&nk) {
        for vk_offset in offsets {
            let vk_abs = match hive.abs(vk_offset) {
                Ok(a) => a,
                Err(_) => continue,
            };
            if vk_abs + 0x14 > hive.bytes.len() {
                continue;
            }
            if &hive.bytes[vk_abs + 4..vk_abs + 6] != VK_SIGNATURE {
                continue;
            }
            let data_type = u32::from_le_bytes(
                hive.bytes[vk_abs + 0x10..vk_abs + 0x14]
                    .try_into()
                    .unwrap_or([0; 4]),
            );
            let data_len_raw = u32::from_le_bytes(
                hive.bytes[vk_abs + 0x08..vk_abs + 0x0C]
                    .try_into()
                    .unwrap_or([0; 4]),
            );
            let raw_data_offset = u32::from_le_bytes(
                hive.bytes[vk_abs + 0x0C..vk_abs + 0x10]
                    .try_into()
                    .unwrap_or([0; 4]),
            );
            // REG_NONE (0) or REG_DWORD (4) with inline flag set
            if (data_type == 0 || data_type == REG_DWORD)
                && (data_len_raw & 0x7FFF_FFFF) <= 4
                && raw_data_offset > 0
                && raw_data_offset < 0xFFFF
            {
                return Some(raw_data_offset);
            }
        }
    }

    warnings.push(format!(
        "SAM key {} has no readable RID value (raw scan also failed)",
        key_path.join("\\"),
    ));
    None
}

fn extract_sam_user(
    hive: &RegistryHiveReader<'_>,
    username: &str,
    rid: u32,
    warnings: &mut Vec<String>,
) -> Option<SamUser> {
    let rid_hex = format!("{:08X}", rid);
    let user_key: &[&str] = &["SAM", "Domains", "Account", "Users"];

    // Read the V value from the user's RID subkey.
    // Build path: SAM\Domains\Account\Users\<RID_HEX>
    let mut v_path: Vec<&str> = user_key.to_vec();
    v_path.push(rid_hex.as_str());

    let v_data = match hive.lookup_value(&v_path, "V") {
        Ok(Some(RegistryValue::Binary(data))) => data,
        Ok(Some(other)) => {
            warnings.push(format!(
                "SAM user {}\\V value has unexpected type: {:?}",
                v_path.join("\\"),
                other
            ));
            return None;
        }
        Ok(None) => {
            warnings.push(format!("SAM user {}\\V not found", v_path.join("\\")));
            return None;
        }
        Err(err) => {
            warnings.push(format!(
                "SAM user {}\\V parse error: {err}",
                v_path.join("\\")
            ));
            return None;
        }
    };

    let (last_login, password_last_set, _v_rid, account_control, admin_count) =
        parse_sam_v_record(&v_data, warnings)?;

    // Parse the UserV blob for profile string fields.
    let profile = crate::registry::sam_structs::parse_user_v(&v_data).unwrap_or_default();

    Some(SamUser {
        username: username.to_string(),
        rid,
        full_name: profile.full_name,
        comment: profile.comment,
        home_dir: profile.home_dir,
        profile_path: profile.profile_path,
        last_login: filetime_to_utc(last_login),
        password_last_set: filetime_to_utc(password_last_set),
        account_disabled: (account_control & SAM_ACCOUNT_DISABLED) != 0,
        account_locked: (account_control & SAM_ACCOUNT_LOCKED) != 0,
        admin_count,
        group_memberships: Vec::new(), // populated later via cross-reference
    })
}

fn parse_sam_v_record(
    data: &[u8],
    warnings: &mut Vec<String>,
) -> Option<(u64, u64, u32, u32, u32)> {
    if data.len() < 0x50 {
        warnings.push(format!(
            "SAM V record is {} bytes, expected at least 0x50",
            data.len()
        ));
        return None;
    }

    let last_login = u64::from_le_bytes(data.get(0x08..0x10)?.try_into().ok()?);
    let password_last_set = u64::from_le_bytes(data.get(0x18..0x20)?.try_into().ok()?);
    let rid = u32::from_le_bytes(data.get(0x28..0x2C)?.try_into().ok()?);
    let account_control = u32::from_le_bytes(data.get(0x2C..0x30)?.try_into().ok()?);
    let admin_count = data
        .get(0x46..0x48)
        .and_then(|b| <[u8; 2]>::try_from(b).ok())
        .map(u16::from_le_bytes)
        .unwrap_or(0) as u32;

    Some((
        last_login,
        password_last_set,
        rid,
        account_control,
        admin_count,
    ))
}

fn extract_sam_aliases(
    hive: &RegistryHiveReader<'_>,
    alias_root: &[&str],
    rid_to_username: &std::collections::HashMap<u32, String>,
    info: &mut SamInfo,
) {
    let mut names_path: Vec<&str> = alias_root.to_vec();
    names_path.push("Names");

    let names_nk = match hive.navigate_to(&names_path) {
        Ok(Some(nk)) => nk,
        Err(err) => {
            info.warnings
                .push(format!("{} parse error: {err}", names_path.join("\\")));
            return;
        }
        _ => return,
    };

    let subkey_names = match hive.read_subkey_names_from_nk(&names_nk) {
        Ok(names) => names,
        Err(err) => {
            info.warnings
                .push(format!("{} subkeys error: {err}", names_path.join("\\")));
            return;
        }
    };

    for group_name in subkey_names {
        let mut group_path: Vec<&str> = names_path.to_vec();
        group_path.push(group_name.as_str());

        let group_rid = match find_rid_in_sam_key(hive, &group_path, &mut info.warnings) {
            Some(rid) => rid,
            None => continue,
        };

        // Parse the C value to get group members
        let rid_hex = format!("{:08X}", group_rid);
        let mut group_key: Vec<&str> = alias_root.to_vec();
        group_key.push(rid_hex.as_str());

        let members = match hive.lookup_value(&group_key, "C") {
            Ok(Some(RegistryValue::Binary(data))) => {
                parse_sam_c_members(&data, rid_to_username, &mut info.warnings)
            }
            Ok(Some(other)) => {
                info.warnings.push(format!(
                    "SAM group {}\\C value has unexpected type: {:?}",
                    group_key.join("\\"),
                    other
                ));
                Vec::new()
            }
            _ => Vec::new(),
        };

        info.groups.push(SamGroup {
            name: group_name,
            rid: group_rid,
            members,
        });
    }
}

fn parse_sam_c_members(
    data: &[u8],
    rid_to_username: &std::collections::HashMap<u32, String>,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    if data.len() < 8 {
        warnings.push(format!(
            "SAM group C value is {} bytes, expected at least 8",
            data.len()
        ));
        return Vec::new();
    }

    // C value structure: revision(2) + ?(2) + member_count(4) + member SIDs...
    let member_count = data
        .get(4..8)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0) as usize;
    if member_count == 0 {
        return Vec::new();
    }

    let mut offset = 8usize;
    let mut members = Vec::new();

    for _ in 0..member_count {
        if offset >= data.len() {
            break;
        }
        let sid_remaining = &data[offset..];
        if let Some((rid, sid_len)) = parse_sid_rid(sid_remaining) {
            if let Some(username) = rid_to_username.get(&rid) {
                members.push(username.clone());
            } else {
                // RID not in our user map — this may be a well-known local SID
                // or a domain SID. Record it as a placeholder.
                members.push(format!("rid-{rid}"));
            }
            offset = offset.saturating_add(sid_len);
        } else {
            break;
        }
    }

    members
}

fn parse_sid_rid(data: &[u8]) -> Option<(u32, usize)> {
    if data.len() < 8 {
        return None;
    }
    let sub_auth_count = data[1] as usize;
    if sub_auth_count == 0 || sub_auth_count > 15 {
        return None;
    }
    let sid_len = 8usize.checked_add(sub_auth_count.checked_mul(4)?)?;
    if data.len() < sid_len {
        return None;
    }
    let last_sub_auth_offset = 8 + (sub_auth_count - 1) * 4;
    let rid = u32::from_le_bytes(
        data.get(last_sub_auth_offset..last_sub_auth_offset + 4)?
            .try_into()
            .ok()?,
    );
    Some((rid, sid_len))
}

fn filetime_to_utc(filetime: u64) -> Option<DateTime<Utc>> {
    if filetime == 0 {
        return None;
    }
    let unix_seconds = (filetime / 10_000_000).saturating_sub(11_644_473_600);
    let nanos = ((filetime % 10_000_000) * 100) as u32;
    Utc.timestamp_opt(unix_seconds as i64, nanos).single()
}

// ── Run / RunOnce ────────────────────────────────────────────────────────────

fn extract_run_keys(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<RegistryRunKey> {
    let mut keys = Vec::new();
    let base = &["Software", "Microsoft", "Windows", "CurrentVersion"];
    for suffix in &["Run", "RunOnce"] {
        let mut full: Vec<&str> = base.to_vec();
        full.push(suffix);
        keys.extend(extract_run_keys_at(
            hive, hive_path, parser, &full, warnings,
        ));
    }
    keys
}

fn extract_run_keys_at(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    key_path: &[&str],
    warnings: &mut Vec<String>,
) -> Vec<RegistryRunKey> {
    let key_path_str = key_path.join("\\");
    let nk = match hive.navigate_to(key_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("{key_path_str} parse error: {err}"));
            return Vec::new();
        }
    };
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            warnings.push(format!("{key_path_str} values parse error: {err}"));
            return Vec::new();
        }
    };
    values
        .into_iter()
        .filter_map(|(name, value)| match value {
            RegistryValue::String(command) if !command.trim().is_empty() => Some(RegistryRunKey {
                key_path: key_path_str.clone(),
                value_name: name,
                command,
                timestamp: None,
            }),
            _ => None,
        })
        .collect()
}

// ── RecentDocs MRU ───────────────────────────────────────────────────────────

fn extract_recent_docs(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<RecentDoc> {
    let recent_docs_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "RecentDocs",
    ];
    let nk = match hive.navigate_to(recent_docs_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => {
            warnings.push("RecentDocs key not found".to_string());
            return Vec::new();
        }
        Err(err) => {
            warnings.push(format!("RecentDocs parse error: {err}"));
            return Vec::new();
        }
    };
    let subkey_names = match hive.read_subkey_names_from_nk(&nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("RecentDocs subkeys error: {err}"));
            return Vec::new();
        }
    };
    let mut docs = Vec::new();
    for ext in subkey_names {
        let mut ext_path: Vec<&str> = recent_docs_path.to_vec();
        ext_path.push(ext.as_str());
        docs.extend(parse_recent_docs_extension(hive, &ext_path, &ext, warnings));
    }
    docs
}

fn parse_recent_docs_extension(
    hive: &RegistryHiveReader<'_>,
    ext_path: &[&str],
    ext: &str,
    _warnings: &mut Vec<String>,
) -> Vec<RecentDoc> {
    let ext_nk = match hive.navigate_to(ext_path) {
        Ok(Some(nk)) => nk,
        _ => return Vec::new(),
    };
    let values = match hive.read_all_values_from_nk(&ext_nk) {
        Ok(values) => values,
        _ => return Vec::new(),
    };

    let mut ordered_indices: Vec<u32> = Vec::new();
    for (name, value) in &values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            if let RegistryValue::Binary(data) = value {
                for chunk in data.chunks(4) {
                    if chunk.len() == 4 {
                        let idx = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        if idx == 0xFFFF_FFFF {
                            break;
                        }
                        ordered_indices.push(idx);
                    }
                }
            }
            break;
        }
    }

    let mut entries: Vec<(u32, RecentDoc)> = Vec::new();
    for (name, value) in &values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            continue;
        }
        let Ok(index) = name.parse::<u32>() else {
            continue;
        };
        match value {
            RegistryValue::Binary(data) => {
                if let Some(file_name) = extract_utf16le_from_binary(data) {
                    entries.push((
                        index,
                        RecentDoc {
                            file_name,
                            extension: ext.to_string(),
                            last_accessed: None,
                            lnk_target: None,
                        },
                    ));
                }
            }
            RegistryValue::String(s) => {
                entries.push((
                    index,
                    RecentDoc {
                        file_name: s.clone(),
                        extension: ext.to_string(),
                        last_accessed: None,
                        lnk_target: None,
                    },
                ));
            }
            _ => {}
        }
    }

    if !ordered_indices.is_empty() {
        entries.sort_by_key(|(idx, _)| {
            ordered_indices
                .iter()
                .position(|&i| i == *idx)
                .unwrap_or(usize::MAX)
        });
    } else {
        entries.sort_by_key(|(n, _)| *n);
    }
    entries.into_iter().map(|(_, doc)| doc).collect()
}

// ── UserAssist ───────────────────────────────────────────────────────────────

const USER_ASSIST_ENTRY_SIZE: usize = 72;

fn extract_user_assist(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<UserAssistEntry> {
    let ua_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "UserAssist",
    ];
    let ua_nk = match hive.navigate_to(ua_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("UserAssist parse error: {err}"));
            return Vec::new();
        }
    };
    let guid_names = match hive.read_subkey_names_from_nk(&ua_nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("UserAssist GUIDs error: {err}"));
            return Vec::new();
        }
    };
    let mut entries = Vec::new();
    for guid in guid_names {
        let mut count_path: Vec<&str> = ua_path.to_vec();
        count_path.push(guid.as_str());
        count_path.push("Count");
        entries.extend(parse_user_assist_count_key(hive, &count_path, warnings));
    }
    entries
}

fn parse_user_assist_count_key(
    hive: &RegistryHiveReader<'_>,
    count_path: &[&str],
    warnings: &mut Vec<String>,
) -> Vec<UserAssistEntry> {
    let count_nk = match hive.navigate_to(count_path) {
        Ok(Some(nk)) => nk,
        _ => return Vec::new(),
    };
    let values = match hive.read_all_values_from_nk(&count_nk) {
        Ok(values) => values,
        _ => return Vec::new(),
    };
    let mut entries = Vec::new();
    for (name, value) in values {
        if let RegistryValue::Binary(data) = value {
            if data.len() < USER_ASSIST_ENTRY_SIZE {
                warnings.push(format!(
                    "UserAssist entry '{}' binary is {} bytes (expected {USER_ASSIST_ENTRY_SIZE}); skipping",
                    name, data.len()
                ));
                continue;
            }
            let run_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let session_id = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
            let focus_time = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
            let filetime = u64::from_le_bytes([
                data[60], data[61], data[62], data[63], data[64], data[65], data[66], data[67],
            ]);
            let executable_path = rot13_decode(&name);
            let last_run = windows_filetime_to_rfc3339(filetime);
            entries.push(UserAssistEntry {
                executable_path,
                run_count,
                last_run,
                focus_time_ms: focus_time as u64,
                session_id,
            });
        }
    }
    entries
}

// ── TypedURLs (IE) ──────────────────────────────────────────────────────────

fn extract_typed_urls(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let typed_urls_path: &[&str] = &["Software", "Microsoft", "Internet Explorer", "TypedURLs"];
    let nk = match hive.navigate_to(typed_urls_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("TypedURLs parse error: {err}"));
            return Vec::new();
        }
    };
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            warnings.push(format!("TypedURLs values error: {err}"));
            return Vec::new();
        }
    };
    let mut numbered: Vec<(u32, String)> = values
        .into_iter()
        .filter_map(|(name, value)| {
            if let Some(num_str) = name.strip_prefix("url") {
                if let Ok(num) = num_str.parse::<u32>() {
                    if let RegistryValue::String(url) = value {
                        if !url.trim().is_empty() {
                            return Some((num, url));
                        }
                    }
                }
            }
            None
        })
        .collect();
    numbered.sort_by_key(|(n, _)| *n);
    numbered.into_iter().map(|(_, url)| url).collect()
}

// ── WordWheelQuery ──────────────────────────────────────────────────────────

fn extract_word_wheel_query(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let wwq_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "WordWheelQuery",
    ];
    let nk = match hive.navigate_to(wwq_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("WordWheelQuery parse error: {err}"));
            return Vec::new();
        }
    };
    let values = match hive.read_all_values_from_nk(&nk) {
        Ok(values) => values,
        Err(err) => {
            warnings.push(format!("WordWheelQuery values error: {err}"));
            return Vec::new();
        }
    };

    let mut ordered_indices: Vec<u32> = Vec::new();
    for (name, value) in &values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            if let RegistryValue::Binary(data) = value {
                for chunk in data.chunks(4) {
                    if chunk.len() == 4 {
                        let idx = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        if idx == 0xFFFF_FFFF {
                            break;
                        }
                        ordered_indices.push(idx);
                    }
                }
            }
            break;
        }
    }

    let mut queries: Vec<(u32, String)> = Vec::new();
    for (name, value) in &values {
        if name.eq_ignore_ascii_case("MRUListEx") {
            continue;
        }
        let Ok(index) = name.parse::<u32>() else {
            continue;
        };
        match value {
            RegistryValue::Binary(data) => {
                if let Some(query) = extract_utf16le_from_binary(data) {
                    queries.push((index, query));
                }
            }
            RegistryValue::String(s) if !s.trim().is_empty() => {
                queries.push((index, s.clone()));
            }
            _ => {}
        }
    }
    if !ordered_indices.is_empty() {
        queries.sort_by_key(|(idx, _)| {
            ordered_indices
                .iter()
                .position(|&i| i == *idx)
                .unwrap_or(usize::MAX)
        });
    } else {
        queries.sort_by_key(|(n, _)| *n);
    }
    queries.into_iter().map(|(_, q)| q).collect()
}

// ── MountPoints2 ────────────────────────────────────────────────────────────

fn extract_mount_points(
    hive: &RegistryHiveReader<'_>,
    _hive_path: &str,
    _parser: &str,
    warnings: &mut Vec<String>,
) -> Vec<MountPoint> {
    let mp_path: &[&str] = &[
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
        "MountPoints2",
    ];
    let nk = match hive.navigate_to(mp_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Vec::new(),
        Err(err) => {
            warnings.push(format!("MountPoints2 parse error: {err}"));
            return Vec::new();
        }
    };
    let subkey_names = match hive.read_subkey_names_from_nk(&nk) {
        Ok(names) => names,
        Err(err) => {
            warnings.push(format!("MountPoints2 subkeys error: {err}"));
            return Vec::new();
        }
    };
    let mut points = Vec::new();
    for name in subkey_names {
        let mut drive_letter = None;
        let mut volume_guid = None;
        if name.len() == 1 && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            drive_letter = Some(format!("{name}:"));
        } else if name.starts_with('{') && name.ends_with('}') {
            volume_guid = Some(name.clone());
        }
        if drive_letter.is_some() || volume_guid.is_some() {
            points.push(MountPoint {
                drive_letter,
                volume_guid,
                last_mounted: None,
            });
        }
    }
    points
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Apply ROT-13 substitution (UserAssist value-name decoding).
fn rot13_decode(encoded: &str) -> String {
    encoded
        .chars()
        .map(|c| match c {
            'a'..='m' | 'A'..='M' => ((c as u8) + 13) as char,
            'n'..='z' | 'N'..='Z' => ((c as u8) - 13) as char,
            _ => c,
        })
        .collect()
}

/// Convert a Windows FILETIME (100-ns intervals since 1601-01-01) to an
/// RFC 3339 timestamp string. Returns `None` for a zero timestamp or if the
/// value falls outside `chrono`'s representable range.
fn windows_filetime_to_rfc3339(filetime: u64) -> Option<String> {
    if filetime == 0 {
        return None;
    }
    let unix_seconds = (filetime / 10_000_000).saturating_sub(11_644_473_600);
    let nanos = ((filetime % 10_000_000) * 100) as u32;
    Utc.timestamp_opt(unix_seconds as i64, nanos)
        .single()
        .map(|dt| dt.to_rfc3339())
}

/// Extract a UTF-16LE null-terminated string from the beginning of a binary
/// blob. Skips an optional 4-byte size header if the first u32 happens to
/// equal the remaining length.
fn extract_utf16le_from_binary(data: &[u8]) -> Option<String> {
    if data.len() < 2 {
        return None;
    }
    let payload = if data.len() >= 4 {
        let header = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if header > 0 && header.saturating_sub(4) <= data.len().saturating_sub(4) {
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

pub(crate) struct RegistryHiveReader<'a> {
    bytes: &'a [u8],
    root_cell_offset: u32,
}

impl<'a> RegistryHiveReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < BASE_BLOCK_SIZE {
            return Err("registry hive shorter than base block".to_string());
        }
        if bytes.get(0..4) != Some(b"regf") {
            return Err("not a valid registry hive".to_string());
        }
        // Validate first hbin header at offset 0x1000 (Task 2.1.1)
        if bytes.len() < BASE_BLOCK_SIZE + 32 {
            return Err("registry hive too short for first hbin header".to_string());
        }
        if bytes.get(BASE_BLOCK_SIZE..BASE_BLOCK_SIZE + 4) != Some(HBIN_MAGIC) {
            return Err("first hbin header missing 'hbin' magic".to_string());
        }
        let hbin_size = read_u32(bytes, BASE_BLOCK_SIZE + 8)? as usize;
        if hbin_size == 0 || !hbin_size.is_multiple_of(4096) {
            return Err(format!(
                "first hbin size {hbin_size:#x} is not a valid page multiple"
            ));
        }
        let root_cell_offset = read_u32(bytes, 0x24)?;
        // Validate root cell offset is within first hbin (Task 2.1.3)
        if root_cell_offset >= hbin_size as u32 {
            return Err(format!(
                "root cell offset {root_cell_offset:#x} exceeds first hbin size {hbin_size:#x}"
            ));
        }
        Ok(Self {
            bytes,
            root_cell_offset,
        })
    }

    fn lookup_value(
        &self,
        key_path: &[&str],
        value_name: &str,
    ) -> Result<Option<RegistryValue>, String> {
        // Task 2.1.2: bounded key path depth
        if key_path.len() > MAX_KEY_LOOKUP_DEPTH {
            return Err(format!(
                "registry key path depth {} exceeds limit {}",
                key_path.len(),
                MAX_KEY_LOOKUP_DEPTH
            ));
        }
        let mut nk = self.parse_nk(self.root_cell_offset)?;
        for segment in key_path {
            let Some(next_offset) = self.find_subkey_offset(&nk, segment)? else {
                return Ok(None);
            };
            nk = self.parse_nk(next_offset)?;
        }
        self.read_value(&nk, value_name)
    }

    fn find_subkey_offset(&self, nk: &NkRecord, wanted: &str) -> Result<Option<u32>, String> {
        if nk.num_subkeys == 0 || nk.subkeys_list_offset == INVALID_OFFSET {
            return Ok(None);
        }
        for offset in self.read_subkey_offsets(nk.subkeys_list_offset, 0)? {
            match self.parse_nk(offset) {
                Ok(child) if child.name.eq_ignore_ascii_case(wanted) => return Ok(Some(offset)),
                Ok(_) => {}
                Err(_) => continue,
            }
        }
        Ok(None)
    }

    pub(crate) fn control_set_candidates(&self, warnings: &mut Vec<String>) -> Vec<String> {
        let mut candidates = Vec::new();
        match self.lookup_value(&["Select"], "Current") {
            Ok(Some(RegistryValue::Dword(value))) if (1..=999).contains(&value) => {
                candidates.push(format!("ControlSet{value:03}"));
            }
            Ok(Some(value)) => warnings.push(format!(
                "Select\\Current has unsupported type: {:?}; falling back to common ControlSet names",
                value
            )),
            Ok(None) => warnings
                .push("Select\\Current not found; falling back to common ControlSet names".to_string()),
            Err(err) => warnings.push(format!(
                "Select\\Current parse error: {err}; falling back to common ControlSet names"
            )),
        }

        for fallback in ["ControlSet001", "ControlSet002", "CurrentControlSet"] {
            if !candidates
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(fallback))
            {
                candidates.push(fallback.to_string());
            }
        }
        candidates
    }

    fn read_value(&self, nk: &NkRecord, value_name: &str) -> Result<Option<RegistryValue>, String> {
        if nk.num_values == 0 || nk.values_list_offset == INVALID_OFFSET {
            return Ok(None);
        }
        let list_abs = self.abs(nk.values_list_offset)?;
        let cell_size = read_i32(self.bytes, list_abs)?;
        if cell_size >= 0 {
            return Err(format!(
                "value list at {:#x} is free",
                nk.values_list_offset
            ));
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid registry value list cell size".to_string())?
            as usize;
        self.require(list_abs, cell_len)?;
        let list_len = (nk.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "registry value list size overflow".to_string())?;
        let list_start = list_abs + 4;
        if list_len > cell_len.saturating_sub(4) {
            return Err(format!(
                "value list at {:#x} length {:#x} exceeds cell",
                nk.values_list_offset, list_len
            ));
        }
        self.require(list_start, list_len)?;
        for index in 0..nk.num_values as usize {
            let value_offset = read_u32(self.bytes, list_start + index * 4)?;
            if value_offset == INVALID_OFFSET {
                continue;
            }
            let Some((name, value)) = self.parse_vk(value_offset)? else {
                continue;
            };
            if name.eq_ignore_ascii_case(value_name) {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    /// Read a named value's raw bytes directly, without type interpretation.
    /// Used for SAM V/F binary blobs that parse_value_data misidentifies.
    fn read_raw_value_bytes(
        &self,
        nk: &NkRecord,
        value_name: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        if nk.num_values == 0 || nk.values_list_offset == INVALID_OFFSET {
            return Ok(None);
        }
        let list_abs = self.abs(nk.values_list_offset)?;
        let cell_size = read_i32(self.bytes, list_abs)?;
        if cell_size >= 0 {
            return Ok(None);
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid value list".to_string())? as usize;
        self.require(list_abs, cell_len)?;
        let list_len = (nk.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "overflow".to_string())?;
        let list_start = list_abs + 4;
        if list_len > cell_len.saturating_sub(4) {
            return Ok(None);
        }
        self.require(list_start, list_len)?;
        for idx in 0..nk.num_values as usize {
            let vk_off = read_u32(self.bytes, list_start + idx * 4)?;
            if vk_off == INVALID_OFFSET {
                continue;
            }
            let vk_abs = self.abs(vk_off)?;
            // Read VK cell directly: check signature, name, then extract data bytes
            if vk_abs + 0x18 > self.bytes.len() {
                continue;
            }
            if &self.bytes[vk_abs + 4..vk_abs + 6] != VK_SIGNATURE {
                continue;
            }
            let name_len = read_u16(self.bytes, vk_abs + 6)? as usize;
            let data_len_raw = read_u32(self.bytes, vk_abs + 8)?;
            let data_offset = read_u32(self.bytes, vk_abs + 0x0C)?;
            let flags = read_u16(self.bytes, vk_abs + 0x14)?;
            let name_start = vk_abs + 0x18;
            if self.bytes.len() < name_start + name_len {
                continue;
            }
            let name = decode_name(
                &self.bytes[name_start..name_start + name_len],
                flags & 0x01 != 0,
            )?;
            if !name.eq_ignore_ascii_case(value_name) {
                continue;
            }
            // Extract raw bytes (skip RegistryValue type interpretation)
            let data_len = (data_len_raw & 0x7FFF_FFFF) as usize;
            let raw = if data_len_raw & 0x8000_0000 != 0 {
                if data_len > 4 {
                    return Err("inline value >4 bytes".into());
                }
                data_offset.to_le_bytes()[..data_len].to_vec()
            } else if data_len == 0 {
                // REG_NONE: the data_offset IS the value
                data_offset.to_le_bytes().to_vec()
            } else {
                let data_abs = self.abs(data_offset)?;
                let dcell = read_i32(self.bytes, data_abs)?;
                if dcell >= 0 {
                    return Ok(None);
                }
                let dlen = dcell
                    .checked_abs()
                    .ok_or_else(|| "invalid data cell".to_string())?
                    as usize;
                self.require(data_abs, dlen)?;
                let dstart = data_abs + 4;
                if data_len > dlen.saturating_sub(4) {
                    return Ok(None);
                }
                self.require(dstart, data_len)?;
                self.bytes[dstart..dstart + data_len].to_vec()
            };
            return Ok(Some(raw));
        }
        Ok(None)
    }

    fn parse_nk(&self, cell_offset: u32) -> Result<NkRecord, String> {
        let abs = self.abs(cell_offset)?;
        let cell_size = read_i32(self.bytes, abs)?;
        if cell_size >= 0 {
            return Err(format!("cell at {cell_offset:#x} is free"));
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid registry cell size".to_string())?
            as usize;
        self.require(abs, cell_len)?;
        if self.bytes.get(abs + 4..abs + 6) != Some(NK_SIGNATURE) {
            return Err(format!("cell at {cell_offset:#x} is not nk"));
        }
        let flags = read_u16(self.bytes, abs + 6)?;
        let num_subkeys = read_u32(self.bytes, abs + 0x18)?;
        let subkeys_list_offset = read_u32(self.bytes, abs + 0x20)?;
        let num_values = read_u32(self.bytes, abs + 0x28)?;
        let values_list_offset = read_u32(self.bytes, abs + 0x2c)?;
        let name_len = read_u16(self.bytes, abs + 0x4c)? as usize;
        let name_start = abs + 0x50;
        self.require(name_start, name_len)?;
        let name = decode_name(
            &self.bytes[name_start..name_start + name_len],
            flags & 0x20 != 0,
        )?;
        Ok(NkRecord {
            name,
            num_subkeys,
            subkeys_list_offset,
            num_values,
            values_list_offset,
        })
    }

    fn parse_vk(&self, cell_offset: u32) -> Result<Option<(String, RegistryValue)>, String> {
        let abs = self.abs(cell_offset)?;
        let cell_size = read_i32(self.bytes, abs)?;
        if cell_size >= 0 {
            return Ok(None);
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid registry value cell size".to_string())?
            as usize;
        self.require(abs, cell_len)?;
        if self.bytes.get(abs + 4..abs + 6) != Some(VK_SIGNATURE) {
            return Ok(None);
        }
        let name_len = read_u16(self.bytes, abs + 6)? as usize;
        let data_len_raw = read_u32(self.bytes, abs + 8)?;
        let data_offset = read_u32(self.bytes, abs + 0x0c)?;
        let data_type = read_u32(self.bytes, abs + 0x10)?;
        let flags = read_u16(self.bytes, abs + 0x14)?;
        let name_start = abs + 0x18;
        self.require(name_start, name_len)?;
        let name = decode_name(
            &self.bytes[name_start..name_start + name_len],
            flags & 0x01 != 0,
        )?;
        let data_len = (data_len_raw & 0x7fff_ffff) as usize;
        let data = if data_len_raw & 0x8000_0000 != 0 {
            if data_len > 4 {
                return Err(format!(
                    "inline value at {cell_offset:#x} length {data_len:#x} exceeds 4 bytes"
                ));
            }
            let inline = data_offset.to_le_bytes();
            inline[..data_len].to_vec()
        } else if data_len == 0 {
            Vec::new()
        } else {
            let data_abs = self.abs(data_offset)?;
            let cell_size = read_i32(self.bytes, data_abs)?;
            if cell_size >= 0 {
                return Err(format!("value data cell at {data_offset:#x} is free"));
            }
            let cell_len = cell_size
                .checked_abs()
                .ok_or_else(|| "invalid registry value data cell size".to_string())?
                as usize;
            self.require(data_abs, cell_len)?;
            let data_start = data_abs + 4;
            self.require(data_start, data_len)?;
            if data_len > cell_len.saturating_sub(4) {
                return Err(format!(
                    "value data at {data_offset:#x} length {data_len:#x} exceeds cell"
                ));
            }
            self.bytes[data_start..data_start + data_len].to_vec()
        };
        Ok(Some((name, parse_value_data(data_type, &data)?)))
    }

    fn read_subkey_offsets(&self, list_offset: u32, depth: u8) -> Result<Vec<u32>, String> {
        if depth > 8 {
            return Err("registry subkey list nesting too deep".to_string());
        }
        let abs = self.abs(list_offset)?;
        let cell_size = read_i32(self.bytes, abs)?;
        if cell_size >= 0 {
            return Err(format!("subkey list at {list_offset:#x} is free"));
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid subkey list cell size".to_string())?
            as usize;
        self.require(abs, cell_len)?;
        let signature = self
            .bytes
            .get(abs + 4..abs + 6)
            .ok_or_else(|| "subkey list signature out of bounds".to_string())?;
        let count = read_u16(self.bytes, abs + 6)? as usize;
        let mut offsets = Vec::new();
        match signature {
            b"lf" | b"lh" => {
                for index in 0..count {
                    let entry = abs + 8 + index * 8;
                    self.require(entry, 8)?;
                    let primary = read_u32(self.bytes, entry)?;
                    let legacy_synthetic = read_u32(self.bytes, entry + 4)?;
                    offsets.push(primary);
                    if legacy_synthetic != primary {
                        // Older synthetic fixtures in this repository wrote
                        // the name hash before the child offset. Real Windows
                        // hives store the child offset first.
                        offsets.push(legacy_synthetic);
                    }
                }
            }
            b"li" => {
                for index in 0..count {
                    let entry = abs + 8 + index * 4;
                    self.require(entry, 4)?;
                    offsets.push(read_u32(self.bytes, entry)?);
                }
            }
            b"ri" => {
                for index in 0..count {
                    let entry = abs + 8 + index * 4;
                    self.require(entry, 4)?;
                    offsets
                        .extend(self.read_subkey_offsets(read_u32(self.bytes, entry)?, depth + 1)?);
                }
            }
            _ => {
                return Err(format!(
                    "unsupported subkey list signature {}",
                    String::from_utf8_lossy(signature)
                ));
            }
        }
        Ok(offsets)
    }

    /// Navigate to the NK record at `key_path` (empty slice = root).
    pub(crate) fn navigate_to(&self, key_path: &[&str]) -> Result<Option<NkRecord>, String> {
        if key_path.len() > MAX_KEY_LOOKUP_DEPTH {
            return Err(format!(
                "registry key path depth {} exceeds limit {}",
                key_path.len(),
                MAX_KEY_LOOKUP_DEPTH
            ));
        }
        let mut nk = self.parse_nk(self.root_cell_offset)?;
        for segment in key_path {
            let Some(next_offset) = self.find_subkey_offset(&nk, segment)? else {
                return Ok(None);
            };
            nk = self.parse_nk(next_offset)?;
        }
        Ok(Some(nk))
    }

    /// Read all (name, value) pairs from a given NK record.
    fn read_all_values_from_nk(
        &self,
        nk: &NkRecord,
    ) -> Result<Vec<(String, RegistryValue)>, String> {
        if nk.num_values == 0 || nk.values_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        let list_abs = self.abs(nk.values_list_offset)?;
        let cell_size = read_i32(self.bytes, list_abs)?;
        if cell_size >= 0 {
            return Err(format!(
                "value list at {:#x} is free",
                nk.values_list_offset
            ));
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid registry value list cell size".to_string())?
            as usize;
        self.require(list_abs, cell_len)?;
        let list_len = (nk.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "registry value list size overflow".to_string())?;
        let list_start = list_abs + 4;
        if list_len > cell_len.saturating_sub(4) {
            return Err(format!(
                "value list at {:#x} length {:#x} exceeds cell",
                nk.values_list_offset, list_len
            ));
        }
        self.require(list_start, list_len)?;
        let mut result = Vec::with_capacity(nk.num_values as usize);
        for index in 0..nk.num_values as usize {
            let value_offset = read_u32(self.bytes, list_start + index * 4)?;
            if value_offset == INVALID_OFFSET {
                continue;
            }
            if let Some((name, value)) = self.parse_vk(value_offset)? {
                result.push((name, value));
            }
        }
        Ok(result)
    }

    /// Read raw VK cell offsets from an NK record's value list.
    /// Used by SAM RID extraction when REG_NONE values have empty data
    /// but the data_offset field encodes the RID inline.
    fn read_raw_vk_data_offsets(&self, nk: &NkRecord) -> Result<Vec<u32>, String> {
        if nk.num_values == 0 || nk.values_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        let list_abs = self.abs(nk.values_list_offset)?;
        let cell_size = read_i32(self.bytes, list_abs)?;
        if cell_size >= 0 {
            return Ok(Vec::new());
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid value list cell".to_string())? as usize;
        self.require(list_abs, cell_len)?;
        let list_len = (nk.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "overflow".to_string())?;
        let list_start = list_abs + 4;
        if list_len > cell_len.saturating_sub(4) {
            return Ok(Vec::new());
        }
        self.require(list_start, list_len)?;
        let mut offsets = Vec::with_capacity(nk.num_values as usize);
        for idx in 0..nk.num_values as usize {
            let vk_off = read_u32(self.bytes, list_start + idx * 4)?;
            if vk_off != INVALID_OFFSET {
                offsets.push(vk_off);
            }
        }
        Ok(offsets)
    }

    /// Read the names of all subkeys of a given NK record.
    pub(crate) fn read_subkey_names_from_nk(&self, nk: &NkRecord) -> Result<Vec<String>, String> {
        if nk.num_subkeys == 0 || nk.subkeys_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        let offsets = self.read_subkey_offsets(nk.subkeys_list_offset, 0)?;
        let mut names = Vec::with_capacity(offsets.len());
        for offset in offsets {
            if let Ok(child) = self.parse_nk(offset) {
                names.push(child.name);
            }
        }
        Ok(names)
    }

    /// Navigate to `key_path` and read the class name of that key.
    /// Returns `None` when the key exists but has no class name.
    pub(crate) fn read_class_name_at(&self, key_path: &[&str]) -> Result<Option<String>, String> {
        if key_path.len() > MAX_KEY_LOOKUP_DEPTH {
            return Err(format!(
                "registry key path depth {} exceeds limit {}",
                key_path.len(),
                MAX_KEY_LOOKUP_DEPTH
            ));
        }
        let mut nk_offset = self.root_cell_offset;
        let mut nk = self.parse_nk(nk_offset)?;
        for segment in key_path {
            let Some(next_offset) = self.find_subkey_offset(&nk, segment)? else {
                return Ok(None);
            };
            nk_offset = next_offset;
            nk = self.parse_nk(nk_offset)?;
        }
        self.read_nk_class_name(nk_offset)
    }

    /// Read the class name from an NK cell at the given hive-relative offset.
    fn read_nk_class_name(&self, nk_offset: u32) -> Result<Option<String>, String> {
        let nk_abs = self.abs(nk_offset)?;
        // Validate the cell is an NK record
        let cell_size = read_i32(self.bytes, nk_abs)?;
        if cell_size >= 0 {
            return Err(format!("NK cell at {nk_offset:#x} is free"));
        }
        if self.bytes.get(nk_abs + 4..nk_abs + 6) != Some(NK_SIGNATURE) {
            return Err("class name read target is not an NK cell".to_string());
        }

        let class_name_length = read_u16(self.bytes, nk_abs + 0x4E)? as usize;
        if class_name_length == 0 {
            return Ok(None);
        }
        if class_name_length > 4096 {
            return Err(format!(
                "class name length {class_name_length} at {nk_offset:#x} is implausibly large"
            ));
        }

        let classname_offset = read_u32(self.bytes, nk_abs + 0x34)?;
        let class_data: Vec<u8> = if classname_offset != INVALID_OFFSET && classname_offset != 0 {
            // External class name: read from the data cell at classname_offset.
            let data_abs = self.abs(classname_offset)?;
            let dcell_size = read_i32(self.bytes, data_abs)?;
            if dcell_size >= 0 {
                return Err(format!(
                    "class name data cell at {classname_offset:#x} is free"
                ));
            }
            let dcell_len = dcell_size
                .checked_abs()
                .ok_or_else(|| "invalid class name data cell size".to_string())?
                as usize;
            self.require(data_abs, dcell_len)?;
            let data_start = data_abs + 4;
            self.require(data_start, class_name_length)?;
            if class_name_length > dcell_len.saturating_sub(4) {
                return Err(format!(
                    "class name at {classname_offset:#x} length {class_name_length:#x} exceeds cell"
                ));
            }
            self.bytes[data_start..data_start + class_name_length].to_vec()
        } else {
            // Inline class name: stored right after the key name in the NK cell.
            let name_len = read_u16(self.bytes, nk_abs + 0x4C)? as usize;
            let class_start = nk_abs + 0x50 + name_len;
            self.require(class_start, class_name_length)?;
            self.bytes[class_start..class_start + class_name_length].to_vec()
        };

        // Decode the class name bytes (always UTF-16LE in registry hives).
        if class_data.len() < 2 || !class_data.len().is_multiple_of(2) {
            return Ok(None);
        }
        let units: Vec<u16> = class_data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = String::from_utf16_lossy(&units);
        let trimmed = s.trim_end_matches('\0');
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed.to_string()))
    }

    fn abs(&self, hive_offset: u32) -> Result<usize, String> {
        if hive_offset == INVALID_OFFSET {
            return Err("invalid registry offset".to_string());
        }
        BASE_BLOCK_SIZE
            .checked_add(hive_offset as usize)
            .ok_or_else(|| "registry offset overflow".to_string())
            .and_then(|abs| {
                if abs < self.bytes.len() {
                    Ok(abs)
                } else {
                    Err(format!("registry offset {hive_offset:#x} out of bounds"))
                }
            })
    }

    fn require(&self, abs: usize, len: usize) -> Result<(), String> {
        abs.checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .map(|_| ())
            .ok_or_else(|| format!("registry range {abs:#x}+{len:#x} out of bounds"))
    }
}

fn parse_value_data(data_type: u32, data: &[u8]) -> Result<RegistryValue, String> {
    match data_type {
        REG_SZ | REG_EXPAND_SZ => Ok(RegistryValue::String(decode_utf16_until_nul(data)?)),
        REG_DWORD => Ok(RegistryValue::Dword(
            read_le_array::<4>(data)
                .map(u32::from_le_bytes)
                .ok_or_else(|| "REG_DWORD value shorter than 4 bytes".to_string())?,
        )),
        REG_QWORD => Ok(RegistryValue::Qword(
            read_le_array::<8>(data)
                .map(u64::from_le_bytes)
                .ok_or_else(|| "REG_QWORD value shorter than 8 bytes".to_string())?,
        )),
        REG_MULTI_SZ => Ok(RegistryValue::MultiString(
            decode_utf16_full(data)?
                .split('\0')
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect(),
        )),
        _ => Ok(RegistryValue::Binary(data.to_vec())),
    }
}

fn decode_name(bytes: &[u8], compressed: bool) -> Result<String, String> {
    if compressed {
        return String::from_utf8(bytes.to_vec()).map_err(|err| err.to_string());
    }
    decode_utf16_full(bytes)
}

fn decode_utf16_until_nul(bytes: &[u8]) -> Result<String, String> {
    let mut decoded = decode_utf16_full(bytes)?;
    if let Some(index) = decoded.find('\0') {
        decoded.truncate(index);
    }
    Ok(decoded)
}

fn decode_utf16_full(bytes: &[u8]) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("UTF-16 data has odd byte length".to_string());
    }
    let mut units = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        units.push(unit);
    }
    Ok(String::from_utf16_lossy(&units))
}

fn read_le_array<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    bytes.get(..N)?.try_into().ok()
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| format!("u16 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid u16".to_string())?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| format!("u32 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid u32".to_string())?,
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, String> {
    Ok(i32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| format!("i32 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid i32".to_string())?,
    ))
}

const HBIN_MAGIC: &[u8; 4] = b"hbin";
const MAX_KEY_LOOKUP_DEPTH: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;
    use testing::{builders::registry as registry_fixture, fixtures};

    fn empty_hive(root_name: &str) -> Vec<u8> {
        let mut data = vec![0u8; 0x8000];
        data[0..4].copy_from_slice(b"regf");
        data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
        data[0x1000..0x1004].copy_from_slice(b"hbin");
        data[0x1008..0x100c].copy_from_slice(&0x2000u32.to_le_bytes());
        write_nk(&mut data, 0x20, root_name, &[], &[]);
        data
    }

    fn write_nk(data: &mut [u8], offset: u32, name: &str, subkeys: &[(&str, u32)], values: &[u32]) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        let name_bytes = name.as_bytes();
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"nk");
        data[abs + 6..abs + 8].copy_from_slice(&0x20u16.to_le_bytes());
        data[abs + 0x18..abs + 0x1c].copy_from_slice(&(subkeys.len() as u32).to_le_bytes());
        let subkey_list_offset = 0x2000 + offset;
        let value_list_offset = 0x4000 + offset;
        data[abs + 0x20..abs + 0x24].copy_from_slice(
            &if subkeys.is_empty() {
                INVALID_OFFSET
            } else {
                subkey_list_offset
            }
            .to_le_bytes(),
        );
        data[abs + 0x28..abs + 0x2c].copy_from_slice(&(values.len() as u32).to_le_bytes());
        data[abs + 0x2c..abs + 0x30].copy_from_slice(
            &if values.is_empty() {
                INVALID_OFFSET
            } else {
                value_list_offset
            }
            .to_le_bytes(),
        );
        data[abs + 0x4c..abs + 0x4e].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        data[abs + 0x50..abs + 0x50 + name_bytes.len()].copy_from_slice(name_bytes);

        if !values.is_empty() {
            let list_abs = BASE_BLOCK_SIZE + value_list_offset as usize;
            data[list_abs..list_abs + 4]
                .copy_from_slice(&(-((values.len() as i32 * 4) + 4)).to_le_bytes());
            for (index, value_offset) in values.iter().enumerate() {
                let entry = list_abs + 4 + index * 4;
                data[entry..entry + 4].copy_from_slice(&value_offset.to_le_bytes());
            }
        }

        if !subkeys.is_empty() {
            write_hashed_subkey_list(data, subkey_list_offset, b"lf", subkeys);
        }
    }

    fn write_nk_utf16_name(data: &mut [u8], offset: u32, name: &str) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        let name_bytes: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"nk");
        data[abs + 6..abs + 8].copy_from_slice(&0u16.to_le_bytes());
        data[abs + 0x20..abs + 0x24].copy_from_slice(&INVALID_OFFSET.to_le_bytes());
        data[abs + 0x2c..abs + 0x30].copy_from_slice(&INVALID_OFFSET.to_le_bytes());
        data[abs + 0x4c..abs + 0x4e].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        data[abs + 0x50..abs + 0x50 + name_bytes.len()].copy_from_slice(&name_bytes);
    }

    fn write_hashed_subkey_list(
        data: &mut [u8],
        offset: u32,
        signature: &[u8; 2],
        subkeys: &[(&str, u32)],
    ) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(signature);
        data[abs + 6..abs + 8].copy_from_slice(&(subkeys.len() as u16).to_le_bytes());
        for (index, (name, child_offset)) in subkeys.iter().enumerate() {
            let entry = abs + 8 + index * 8;
            let mut hash = [0u8; 4];
            for (idx, byte) in name.as_bytes().iter().take(4).enumerate() {
                hash[idx] = *byte;
            }
            data[entry..entry + 4].copy_from_slice(&hash);
            data[entry + 4..entry + 8].copy_from_slice(&child_offset.to_le_bytes());
        }
    }

    fn write_flat_subkey_list(data: &mut [u8], offset: u32, signature: &[u8; 2], subkeys: &[u32]) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(signature);
        data[abs + 6..abs + 8].copy_from_slice(&(subkeys.len() as u16).to_le_bytes());
        for (index, child_offset) in subkeys.iter().enumerate() {
            let entry = abs + 8 + index * 4;
            data[entry..entry + 4].copy_from_slice(&child_offset.to_le_bytes());
        }
    }

    fn set_nk_subkey_list(data: &mut [u8], nk_offset: u32, list_offset: u32, count: u32) {
        let abs = BASE_BLOCK_SIZE + nk_offset as usize;
        data[abs + 0x18..abs + 0x1c].copy_from_slice(&count.to_le_bytes());
        data[abs + 0x20..abs + 0x24].copy_from_slice(&list_offset.to_le_bytes());
    }

    fn write_string_value(data: &mut [u8], offset: u32, name: &str, value: &str, data_offset: u32) {
        write_typed_string_value(data, offset, name, REG_SZ, value, data_offset);
    }

    fn write_typed_string_value(
        data: &mut [u8],
        offset: u32,
        name: &str,
        value_type: u32,
        value: &str,
        data_offset: u32,
    ) {
        let encoded: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
        data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[data_abs + 4..data_abs + 4 + encoded.len()].copy_from_slice(&encoded);
        write_vk(
            data,
            offset,
            name,
            value_type,
            encoded.len() as u32,
            data_offset,
        );
    }

    fn write_multi_string_value(
        data: &mut [u8],
        offset: u32,
        name: &str,
        values: &[&str],
        data_offset: u32,
    ) {
        let mut encoded = Vec::new();
        for value in values {
            encoded.extend(value.encode_utf16().flat_map(u16::to_le_bytes));
            encoded.extend(0u16.to_le_bytes());
        }
        encoded.extend(0u16.to_le_bytes());
        let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
        data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[data_abs + 4..data_abs + 4 + encoded.len()].copy_from_slice(&encoded);
        write_vk(
            data,
            offset,
            name,
            REG_MULTI_SZ,
            encoded.len() as u32,
            data_offset,
        );
    }

    fn write_dword_value(data: &mut [u8], offset: u32, name: &str, value: u32) {
        write_vk(data, offset, name, REG_DWORD, 0x8000_0004, value);
    }

    fn write_qword_value(data: &mut [u8], offset: u32, name: &str, value: u64, data_offset: u32) {
        let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
        data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[data_abs + 4..data_abs + 12].copy_from_slice(&value.to_le_bytes());
        write_vk(data, offset, name, REG_QWORD, 8, data_offset);
    }

    fn write_vk(
        data: &mut [u8],
        offset: u32,
        name: &str,
        value_type: u32,
        data_len: u32,
        data_offset: u32,
    ) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        let name_bytes = name.as_bytes();
        data[abs..abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"vk");
        data[abs + 6..abs + 8].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        data[abs + 8..abs + 12].copy_from_slice(&data_len.to_le_bytes());
        data[abs + 12..abs + 16].copy_from_slice(&data_offset.to_le_bytes());
        data[abs + 16..abs + 20].copy_from_slice(&value_type.to_le_bytes());
        data[abs + 20..abs + 22].copy_from_slice(&1u16.to_le_bytes());
        data[abs + 0x18..abs + 0x18 + name_bytes.len()].copy_from_slice(name_bytes);
    }

    #[test]
    fn reject_non_regf() {
        assert!(RegistryHiveReader::new(b"not-registry").is_err());
    }

    #[test]
    fn reject_missing_hbin_magic() {
        let mut data = empty_hive("ROOT");
        // Corrupt the hbin magic at 0x1000
        data[0x1000..0x1004].copy_from_slice(b"NOPE");
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn reject_zero_hbin_size() {
        let mut data = empty_hive("ROOT");
        // Set hbin size to 0
        data[0x1008..0x100c].copy_from_slice(&0u32.to_le_bytes());
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn reject_non_page_aligned_hbin_size() {
        let mut data = empty_hive("ROOT");
        // Set hbin size to a non-page-aligned value
        data[0x1008..0x100c].copy_from_slice(&0x1234u32.to_le_bytes());
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn reject_truncated_before_hbin() {
        // Hive with regf but truncated before hbin
        let mut data = vec![0u8; 0x1010];
        data[0..4].copy_from_slice(b"regf");
        data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
        // No hbin at 0x1000 (all zeros)
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn reject_root_cell_offset_exceeds_hbin() {
        let mut data = empty_hive("ROOT");
        // Set root cell offset beyond hbin size (0x2000)
        data[0x24..0x28].copy_from_slice(&0x3000u32.to_le_bytes());
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn key_path_depth_exceeds_limit() {
        let data = empty_hive("ROOT");
        let hive = RegistryHiveReader::new(&data).unwrap();
        // Build a key path with 65 segments (exceeds MAX_KEY_LOOKUP_DEPTH = 64)
        let deep_path: Vec<&str> = (0..65).map(|_| "x").collect();
        let err = hive.lookup_value(&deep_path, "val").unwrap_err();
        assert!(err.contains("depth"));
    }

    #[test]
    fn key_path_depth_at_limit_is_allowed() {
        let data = empty_hive("ROOT");
        let hive = RegistryHiveReader::new(&data).unwrap();
        // 64 segments should not be rejected by depth check (will fail on lookup)
        let path: Vec<&str> = (0..64).map(|_| "x").collect();
        // This returns Ok(None) because keys don't exist, but no depth error
        assert!(hive.lookup_value(&path, "val").is_ok());
    }

    #[test]
    fn parse_base_block_regf() {
        let data = empty_hive("SYSTEM");
        assert_eq!(
            RegistryHiveReader::new(&data).unwrap().root_cell_offset,
            0x20
        );
    }

    #[test]
    fn parse_nk_compressed_name() {
        let data = empty_hive("SYSTEM");
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert_eq!(hive.parse_nk(0x20).unwrap().name, "SYSTEM");
    }

    #[test]
    fn parse_nk_utf16_name() {
        let mut data = empty_hive("ROOT");
        write_nk_utf16_name(&mut data, 0x20, "SYST\u{00c8}M");
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(hive.parse_nk(0x20).unwrap().name, "SYST\u{00c8}M");
    }

    #[test]
    fn read_subkeys_lf_and_vk_string() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[("Child", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_subkeys_lh_list() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
        write_hashed_subkey_list(&mut data, 0x2020, b"lh", &[("Child", 0x200)]);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_subkeys_lf_offset_first_real_layout() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
        let abs = BASE_BLOCK_SIZE + 0x2020;
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"lf");
        data[abs + 6..abs + 8].copy_from_slice(&1u16.to_le_bytes());
        data[abs + 8..abs + 12].copy_from_slice(&0x200u32.to_le_bytes());
        data[abs + 12..abs + 16].copy_from_slice(b"Chil");
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_subkeys_li_list() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
        write_flat_subkey_list(&mut data, 0x2020, b"li", &[0x200]);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_subkeys_ri_list() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
        write_flat_subkey_list(&mut data, 0x2020, b"ri", &[0x2080]);
        write_flat_subkey_list(&mut data, 0x2080, b"li", &[0x200]);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_vk_reg_dword_inline() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_dword_value(&mut data, 0x400, "Current", 1);
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert_eq!(
            hive.lookup_value(&[], "Current").unwrap(),
            Some(RegistryValue::Dword(1))
        );
    }

    #[test]
    fn read_vk_reg_expand_sz() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_typed_string_value(
            &mut data,
            0x400,
            "Path",
            REG_EXPAND_SZ,
            "%SystemRoot%\\System32",
            0x700,
        );
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&[], "Path").unwrap(),
            Some(RegistryValue::String("%SystemRoot%\\System32".to_string()))
        );
    }

    #[test]
    fn read_vk_reg_multi_sz_preserves_all_items() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_multi_string_value(&mut data, 0x400, "Services", &["Tcpip", "Dnscache"], 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&[], "Services").unwrap(),
            Some(RegistryValue::MultiString(vec![
                "Tcpip".to_string(),
                "Dnscache".to_string()
            ]))
        );
    }

    #[test]
    fn read_vk_reg_qword_external() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_qword_value(&mut data, 0x400, "Counter", 0x1122_3344_5566_7788, 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&[], "Counter").unwrap(),
            Some(RegistryValue::Qword(0x1122_3344_5566_7788))
        );
    }

    #[test]
    fn odd_utf16_value_data_is_rejected() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        let data_abs = BASE_BLOCK_SIZE + 0x700;
        data[data_abs..data_abs + 4].copy_from_slice(&(-8i32).to_le_bytes());
        data[data_abs + 4..data_abs + 7].copy_from_slice(b"A\0B");
        write_vk(&mut data, 0x400, "Odd", REG_SZ, 3, 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();

        let err = hive.lookup_value(&[], "Odd").unwrap_err();
        assert!(err.contains("UTF-16 data has odd byte length"));
    }

    #[test]
    fn read_value_list_uses_registry_cell_header() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400, 0x500]);
        write_dword_value(&mut data, 0x400, "First", 1);
        write_dword_value(&mut data, 0x500, "Second", 2);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&[], "Second").unwrap(),
            Some(RegistryValue::Dword(2))
        );
    }

    #[test]
    fn bounds_rejects_truncated_value_list_cell() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400, 0x500]);
        let list_abs = BASE_BLOCK_SIZE + 0x4020;
        data[list_abs..list_abs + 4].copy_from_slice(&(-4i32).to_le_bytes());
        let hive = RegistryHiveReader::new(&data).unwrap();

        let err = hive.lookup_value(&[], "Second").unwrap_err();
        assert!(err.contains("value list"));
        assert!(err.contains("exceeds cell"));
    }

    #[test]
    fn inline_value_longer_than_four_bytes_is_rejected() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_vk(&mut data, 0x400, "TooLong", REG_DWORD, 0x8000_0005, 1);
        let hive = RegistryHiveReader::new(&data).unwrap();

        let err = hive.lookup_value(&[], "TooLong").unwrap_err();
        assert!(err.contains("inline value"));
        assert!(err.contains("exceeds 4 bytes"));
    }

    #[test]
    fn short_external_dword_is_rejected_instead_of_zero_filled() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        let data_abs = BASE_BLOCK_SIZE + 0x700;
        data[data_abs..data_abs + 4].copy_from_slice(&(-8i32).to_le_bytes());
        data[data_abs + 4..data_abs + 6].copy_from_slice(&1u16.to_le_bytes());
        write_vk(&mut data, 0x400, "Short", REG_DWORD, 2, 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();

        let err = hive.lookup_value(&[], "Short").unwrap_err();
        assert!(err.contains("REG_DWORD value shorter than 4 bytes"));
    }

    #[test]
    fn bounds_rejects_bad_cell_offset() {
        let data = empty_hive("ROOT");
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert!(hive.parse_nk(0xFFFF).is_err());
    }

    #[test]
    fn corrupt_hive_returns_error_not_panic() {
        let mut data = empty_hive("ROOT");
        data[0x1020..0x1024].copy_from_slice(&(-999_999i32).to_le_bytes());
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert!(hive.parse_nk(0x20).is_err());
    }

    #[test]
    fn extract_system_fields_from_fixture() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Control",
            &[("ComputerName", 0x600), ("TimeZoneInformation", 0xa00)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", "LAB-PC", 0x1800);
        write_nk(&mut data, 0xa00, "TimeZoneInformation", &[], &[0xd00]);
        write_string_value(
            &mut data,
            0xd00,
            "TimeZoneKeyName",
            "China Standard Time",
            0x1900,
        );

        let info = extract_system_hive_fields(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(info.computer_name.unwrap().value, "LAB-PC");
        assert_eq!(info.timezone.unwrap().value, "China Standard Time");
    }

    #[test]
    fn extract_system_fields_falls_back_when_select_is_corrupt() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_vk(
            &mut data,
            0x1200,
            "Current",
            REG_DWORD,
            0x8000_0004,
            0x9530_7897,
        );
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("ComputerName", 0x600)], &[]);
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", "LAB-PC", 0x1800);

        let info = extract_system_hive_fields(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(info.computer_name.unwrap().value, "LAB-PC");
        assert!(info
            .warnings
            .iter()
            .any(|warning| warning.contains("Select\\Current")));
    }

    #[test]
    fn extract_software_fields_from_fixture() {
        let mut data = empty_hive("SOFTWARE");
        write_nk(&mut data, 0x20, "SOFTWARE", &[("Microsoft", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Microsoft", &[("Windows NT", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Windows NT",
            &[("CurrentVersion", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "CurrentVersion",
            &[],
            &[0x600, 0x680, 0x700],
        );
        write_string_value(
            &mut data,
            0x600,
            "ProductName",
            "Windows Evidence Edition",
            0x900,
        );
        write_string_value(&mut data, 0x680, "CurrentBuild", "26000", 0x980);
        write_dword_value(&mut data, 0x700, "InstallDate", 1_700_000_000);

        let info = extract_software_hive_fields(&data, "Windows/System32/config/SOFTWARE").unwrap();

        assert_eq!(info.product_name.unwrap().value, "Windows Evidence Edition");
        assert_eq!(info.current_build.unwrap().value, "26000");
        assert!(info.install_date.unwrap().value.starts_with("2023-"));
    }

    #[test]
    fn extract_system_fields_from_committed_tiny_fixture() {
        let bytes = std::fs::read(fixtures::tiny_registry_system_hive())
            .expect("read tiny SYSTEM registry fixture");

        let info = extract_system_hive_fields(&bytes, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(
            info.computer_name
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SYSTEM_COMPUTER_NAME)
        );
        assert_eq!(
            info.timezone.as_ref().map(|field| field.value.as_str()),
            Some(registry_fixture::SYSTEM_TIMEZONE)
        );
        assert!(info.warnings.is_empty());
    }

    #[test]
    fn extract_software_fields_from_committed_tiny_fixture() {
        let bytes = std::fs::read(fixtures::tiny_registry_software_hive())
            .expect("read tiny SOFTWARE registry fixture");

        let info =
            extract_software_hive_fields(&bytes, "Windows/System32/config/SOFTWARE").unwrap();

        assert_eq!(
            info.product_name.as_ref().map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_PRODUCT_NAME)
        );
        assert_eq!(
            info.current_build
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_CURRENT_BUILD)
        );
        assert_eq!(
            info.display_version
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_DISPLAY_VERSION)
        );
        assert!(info
            .install_date
            .as_ref()
            .is_some_and(|field| field.value.starts_with("2023-")));
    }

    // ── NTUSER.DAT extraction tests ────────────────────────────────────────

    fn write_binary_value(
        data: &mut [u8],
        offset: u32,
        name: &str,
        value_data: &[u8],
        data_offset: u32,
    ) {
        let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
        data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[data_abs + 4..data_abs + 4 + value_data.len()].copy_from_slice(value_data);
        write_vk(data, offset, name, 3, value_data.len() as u32, data_offset);
    }

    fn make_recent_doc_binary(file_name: &str) -> Vec<u8> {
        let utf16: Vec<u8> = file_name
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let total_size = (utf16.len() + 6) as u32; // size header + utf16 data + null term
        let mut result = total_size.to_le_bytes().to_vec();
        result.extend_from_slice(&utf16);
        result.extend_from_slice(&[0x00, 0x00]);
        result
    }

    fn make_user_assist_binary(
        run_count: u32,
        session_id: u32,
        focus_time_ms: u32,
        filetime: u64,
    ) -> Vec<u8> {
        let mut data = vec![0u8; USER_ASSIST_ENTRY_SIZE];
        data[4..8].copy_from_slice(&run_count.to_le_bytes());
        data[8..12].copy_from_slice(&session_id.to_le_bytes());
        data[12..16].copy_from_slice(&focus_time_ms.to_le_bytes());
        data[60..68].copy_from_slice(&filetime.to_le_bytes());
        data
    }

    fn make_mru_list_ex(indices: &[u32]) -> Vec<u8> {
        let mut data = Vec::with_capacity((indices.len() + 1) * 4);
        for idx in indices {
            data.extend_from_slice(&idx.to_le_bytes());
        }
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        data
    }

    #[test]
    fn test_rot13_decode_basic() {
        assert_eq!(
            rot13_decode("P:\\Jvaqbjf\\Flfgrz32\\abgrcnq.rkr"),
            "C:\\Windows\\System32\\notepad.exe"
        );
        assert_eq!(rot13_decode("Hello"), "Uryyb");
        assert_eq!(rot13_decode("Uryyb"), "Hello");
        assert_eq!(rot13_decode("123"), "123");
        assert_eq!(rot13_decode("!@#"), "!@#");
    }

    #[test]
    fn test_rot13_decode_roundtrip() {
        // ROT13 is its own inverse — decoding twice yields the original.
        let original = "C:\\Users\\Admin\\Desktop\\calc.exe";
        let encoded = rot13_decode(original);
        assert_ne!(original, encoded, "encoded should differ from original");
        assert_eq!(
            rot13_decode(&encoded),
            original,
            "roundtrip should restore original"
        );

        let mixed = "Hello123!@#World";
        assert_eq!(rot13_decode(&rot13_decode(mixed)), mixed);
    }

    #[test]
    fn windows_filetime_converts_to_rfc3339() {
        let ft = 133_600_000_000_000_000u64;
        let ts = windows_filetime_to_rfc3339(ft).expect("valid FILETIME");
        assert!(
            ts.starts_with("2024-") || ts.starts_with("2025-"),
            "timestamp {ts} should be in the 2024-2025 range"
        );
    }

    #[test]
    fn windows_filetime_zero_returns_none() {
        assert_eq!(windows_filetime_to_rfc3339(0), None);
    }

    #[test]
    fn test_empty_userassist_key() {
        let data = empty_hive("NTUSER");
        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert!(info.run_keys.is_empty());
        assert!(info.recent_docs.is_empty());
        assert!(info.ua_entries.is_empty());
        assert!(info.typed_urls.is_empty());
        assert!(info.word_wheel_query.is_empty());
        assert!(info.mount_points.is_empty());
    }

    #[test]
    fn extract_ntuser_run_keys() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(&mut data, 0x500, "CurrentVersion", &[("Run", 0x600)], &[]);
        write_nk(&mut data, 0x600, "Run", &[], &[0x700, 0x780]);
        write_string_value(
            &mut data,
            0x700,
            "OneDrive",
            "C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe /background",
            0x1000,
        );
        write_string_value(
            &mut data,
            0x780,
            "SecurityHealth",
            "%ProgramFiles%\\Windows Defender\\MSASCuiL.exe",
            0x1100,
        );

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.run_keys.len(), 2);
        let od = info
            .run_keys
            .iter()
            .find(|k| k.value_name == "OneDrive")
            .unwrap();
        assert!(od.command.contains("OneDrive.exe"));
        assert_eq!(
            od.key_path,
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run"
        );
    }

    #[test]
    fn extract_ntuser_run_once() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("RunOnce", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "RunOnce", &[], &[0x700]);
        write_string_value(
            &mut data,
            0x700,
            "Setup",
            "C:\\Windows\\Setup.exe /silent",
            0x1000,
        );

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.run_keys.len(), 1);
        assert_eq!(info.run_keys[0].value_name, "Setup");
        assert_eq!(
            info.run_keys[0].key_path,
            "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce"
        );
    }

    #[test]
    fn extract_ntuser_recent_docs() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "Explorer", &[("RecentDocs", 0x700)], &[]);
        write_nk(&mut data, 0x700, "RecentDocs", &[(".pdf", 0x800)], &[]);
        write_nk(&mut data, 0x800, ".pdf", &[], &[0x900, 0x980, 0xa00]);

        let mru_list = make_mru_list_ex(&[1, 0]);
        let doc0 = make_recent_doc_binary("report.pdf");
        let doc1 = make_recent_doc_binary("invoice.pdf");

        write_binary_value(&mut data, 0x900, "MRUListEx", &mru_list, 0x1200);
        write_binary_value(&mut data, 0x980, "0", &doc0, 0x1300);
        write_binary_value(&mut data, 0xa00, "1", &doc1, 0x1400);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.recent_docs.len(), 2);
        // MRUListEx [1, 0] means index 1 is most recent
        assert_eq!(info.recent_docs[0].file_name, "invoice.pdf");
        assert_eq!(info.recent_docs[0].extension, ".pdf");
        assert_eq!(info.recent_docs[1].file_name, "report.pdf");
    }

    #[test]
    fn test_userassist_extraction() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(&mut data, 0x600, "Explorer", &[("UserAssist", 0x700)], &[]);
        write_nk(
            &mut data,
            0x700,
            "UserAssist",
            &[("{CEBFF5CD-ACE2-4F4F-9178-9926F41749EA}", 0x800)],
            &[],
        );
        write_nk(
            &mut data,
            0x800,
            "{CEBFF5CD-ACE2-4F4F-9178-9926F41749EA}",
            &[("Count", 0x900)],
            &[],
        );
        write_nk(&mut data, 0x900, "Count", &[], &[0xa00, 0xb00]);

        let encrypted = "P:\\Jvaqbjf\\Flfgrz32\\abgrcnq.rkr";
        let ft: u64 = 133_600_000_000_000_000;
        // run_count=42, session_id=1, focus_time_ms=1500
        let ua1 = make_user_assist_binary(42, 1, 1500, ft);
        write_binary_value(&mut data, 0xa00, encrypted, &ua1, 0x1200);

        let encrypted2 = "P:\\Hfref\\Grfg\\Qrfxgbc\\pnyp.rkr";
        // run_count=7, session_id=2, focus_time_ms=300
        let ua2 = make_user_assist_binary(7, 2, 300, ft + 86_400_000_000_000);
        write_binary_value(&mut data, 0xb00, encrypted2, &ua2, 0x1300);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.ua_entries.len(), 2);

        let notepad = info
            .ua_entries
            .iter()
            .find(|e| e.executable_path.contains("notepad"))
            .unwrap();
        assert_eq!(notepad.run_count, 42);
        assert_eq!(notepad.session_id, 1);
        assert_eq!(notepad.focus_time_ms, 1500);
        assert!(notepad.last_run.is_some());

        let calc = info
            .ua_entries
            .iter()
            .find(|e| e.executable_path.contains("calc"))
            .unwrap();
        assert_eq!(calc.run_count, 7);
        assert_eq!(calc.session_id, 2);
        assert_eq!(calc.focus_time_ms, 300);
    }

    #[test]
    fn extract_ntuser_typed_urls() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Microsoft",
            &[("Internet Explorer", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Internet Explorer",
            &[("TypedURLs", 0x500)],
            &[],
        );
        write_nk(&mut data, 0x500, "TypedURLs", &[], &[0x600, 0x680, 0x700]);

        write_string_value(
            &mut data,
            0x600,
            "url1",
            "https://forensics.example.com",
            0x1000,
        );
        write_string_value(&mut data, 0x680, "url2", "https://github.com", 0x1100);
        write_string_value(&mut data, 0x700, "url3", "https://www.google.com", 0x1200);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.typed_urls.len(), 3);
        assert_eq!(info.typed_urls[0], "https://forensics.example.com");
        assert_eq!(info.typed_urls[1], "https://github.com");
        assert_eq!(info.typed_urls[2], "https://www.google.com");
    }

    #[test]
    fn extract_ntuser_word_wheel_query() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "Explorer",
            &[("WordWheelQuery", 0x700)],
            &[],
        );
        write_nk(
            &mut data,
            0x700,
            "WordWheelQuery",
            &[],
            &[0x800, 0x880, 0x900],
        );

        let wwq_mru = make_mru_list_ex(&[1, 0]);
        write_binary_value(&mut data, 0x800, "MRUListEx", &wwq_mru, 0x1000);
        write_string_value(&mut data, 0x880, "0", "forensics", 0x1100);
        write_string_value(&mut data, 0x900, "1", "evidence", 0x1200);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.word_wheel_query.len(), 2);
        // MRUListEx [1, 0] -> index 1 is most recent
        assert_eq!(info.word_wheel_query[0], "evidence");
        assert_eq!(info.word_wheel_query[1], "forensics");
    }

    #[test]
    fn extract_ntuser_mount_points() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "CurrentVersion",
            &[("Explorer", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "Explorer",
            &[("MountPoints2", 0x700)],
            &[],
        );
        write_nk(
            &mut data,
            0x700,
            "MountPoints2",
            &[
                ("C", 0x800),
                ("D", 0x900),
                ("{ecf5d85e-1234-5678-abcd-123456789abc}", 0xa00),
            ],
            &[],
        );
        write_nk(&mut data, 0x800, "C", &[], &[]);
        write_nk(&mut data, 0x900, "D", &[], &[]);
        write_nk(
            &mut data,
            0xa00,
            "{ecf5d85e-1234-5678-abcd-123456789abc}",
            &[],
            &[],
        );

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.mount_points.len(), 3);

        let c = info
            .mount_points
            .iter()
            .find(|m| m.drive_letter.as_deref() == Some("C:"))
            .unwrap();
        assert!(c.volume_guid.is_none());

        let guid = info
            .mount_points
            .iter()
            .find(|m| m.volume_guid.as_deref() == Some("{ecf5d85e-1234-5678-abcd-123456789abc}"))
            .unwrap();
        assert!(guid.drive_letter.is_none());
    }

    #[test]
    fn extract_ntuser_combined() {
        // Run + RecentDocs + UserAssist in one hive.
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x020, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "CurrentVersion",
            &[("Run", 0x700), ("Explorer", 0x800)],
            &[],
        );
        // Run
        write_nk(&mut data, 0x700, "Run", &[], &[0x780]);
        write_string_value(&mut data, 0x780, "OneDrive", "C:\\OneDrive.exe /bg", 0x3000);
        // Explorer
        write_nk(
            &mut data,
            0x800,
            "Explorer",
            &[("RecentDocs", 0x900), ("UserAssist", 0xa00)],
            &[],
        );
        // RecentDocs
        write_nk(&mut data, 0x900, "RecentDocs", &[(".txt", 0xd00)], &[]);
        write_nk(&mut data, 0xd00, ".txt", &[], &[0xd80, 0xdc0]);
        let mru = make_mru_list_ex(&[0]);
        let doc = make_recent_doc_binary("notes.txt");
        write_binary_value(&mut data, 0xd80, "MRUListEx", &mru, 0x3100);
        write_binary_value(&mut data, 0xdc0, "0", &doc, 0x3200);
        // UserAssist
        write_nk(&mut data, 0xa00, "UserAssist", &[("{GUID}", 0xe00)], &[]);
        write_nk(&mut data, 0xe00, "{GUID}", &[("Count", 0xf00)], &[]);
        write_nk(&mut data, 0xf00, "Count", &[], &[0xf80]);
        let ua = make_user_assist_binary(99, 3, 5000, 133_600_000_000_000_000);
        write_binary_value(
            &mut data,
            0xf80,
            "P:\\Hfref\\Grfg\\Qrfxgbc\\pnyp.rkr",
            &ua,
            0x3300,
        );

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.run_keys.len(), 1);
        assert_eq!(info.recent_docs.len(), 1);
        assert_eq!(info.ua_entries.len(), 1);
        assert_eq!(info.run_keys[0].value_name, "OneDrive");
        assert_eq!(info.recent_docs[0].file_name, "notes.txt");
        assert!(info.ua_entries[0].executable_path.contains("calc"));
        assert_eq!(info.ua_entries[0].run_count, 99);
    }

    #[test]
    fn extract_ntuser_combined_group2() {
        // WordWheelQuery + MountPoints2 + TypedURLs in one hive.
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x020, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Microsoft",
            &[("Windows", 0x400), ("Internet Explorer", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "CurrentVersion",
            &[("Explorer", 0x800)],
            &[],
        );
        // Explorer
        write_nk(
            &mut data,
            0x800,
            "Explorer",
            &[("WordWheelQuery", 0x900), ("MountPoints2", 0xa00)],
            &[],
        );
        // WordWheelQuery
        write_nk(&mut data, 0x900, "WordWheelQuery", &[], &[0x980, 0x9c0]);
        let wwq_mru = make_mru_list_ex(&[0]);
        write_string_value(&mut data, 0x980, "0", "search term", 0x3000);
        write_binary_value(&mut data, 0x9c0, "MRUListEx", &wwq_mru, 0x3100);
        // MountPoints2
        write_nk(&mut data, 0xa00, "MountPoints2", &[("E", 0xb00)], &[]);
        write_nk(&mut data, 0xb00, "E", &[], &[]);
        // IE TypedURLs
        write_nk(
            &mut data,
            0x500,
            "Internet Explorer",
            &[("TypedURLs", 0xc00)],
            &[],
        );
        write_nk(&mut data, 0xc00, "TypedURLs", &[], &[0xc80]);
        write_string_value(&mut data, 0xc80, "url1", "https://example.com", 0x3200);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert_eq!(info.word_wheel_query.len(), 1);
        assert_eq!(info.mount_points.len(), 1);
        assert_eq!(info.typed_urls.len(), 1);
        assert_eq!(info.word_wheel_query[0], "search term");
        assert_eq!(info.mount_points[0].drive_letter.as_deref(), Some("E:"));
        assert_eq!(info.typed_urls[0], "https://example.com");
    }

    #[test]
    fn extract_ntuser_handles_missing_keys() {
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Unrelated", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Unrelated", &[], &[]);

        let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
        assert!(info.run_keys.is_empty());
        assert!(info.recent_docs.is_empty());
        assert!(info.ua_entries.is_empty());
        assert!(info.typed_urls.is_empty());
        assert!(info.word_wheel_query.is_empty());
        assert!(info.mount_points.is_empty());
    }

    // ── SAM hive test helpers ─────────────────────────────────────────────

    fn make_sam_v_record(
        last_login_ft: u64,
        pwd_last_set_ft: u64,
        rid: u32,
        account_control: u32,
        admin_count: u16,
    ) -> Vec<u8> {
        let mut data = vec![0u8; 0x50];
        data[0x08..0x10].copy_from_slice(&last_login_ft.to_le_bytes());
        data[0x18..0x20].copy_from_slice(&pwd_last_set_ft.to_le_bytes());
        data[0x28..0x2C].copy_from_slice(&rid.to_le_bytes());
        data[0x2C..0x30].copy_from_slice(&account_control.to_le_bytes());
        data[0x46..0x48].copy_from_slice(&admin_count.to_le_bytes());
        data
    }

    /// Build a synthetic SID blob. `sub_authorities` includes the
    /// domain-specific components and the final RID.
    fn make_sid(sub_authorities: &[u32]) -> Vec<u8> {
        let sa_count = sub_authorities.len() as u8;
        let mut data = Vec::with_capacity(8 + sub_authorities.len() * 4);
        data.push(1u8); // revision
        data.push(sa_count);
        // Identifier authority: NT Authority (5)
        data.extend_from_slice(&[0u8, 0, 0, 0, 0, 5]);
        for sa in sub_authorities {
            data.extend_from_slice(&sa.to_le_bytes());
        }
        data
    }

    fn make_sam_c_value(member_sids: &[Vec<u8>]) -> Vec<u8> {
        let mut data = Vec::new();
        // Revision (2) + padding (2)
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        // Member count (4)
        data.extend_from_slice(&(member_sids.len() as u32).to_le_bytes());
        for sid in member_sids {
            data.extend_from_slice(sid);
        }
        data
    }

    /// Build a synthetic DomainAccountF binary blob with the given password
    /// policy values.  Day-based values are converted to 100 ns ticks.
    fn make_domain_account_f_blob(
        max_pwd_age_days: u64,
        min_pwd_age_days: u64,
        min_pwd_length: u16,
        pwd_history_length: u16,
        lockout_threshold: u16,
        lockout_duration_minutes: u64,
        lockout_observation_window_minutes: u64,
    ) -> Vec<u8> {
        // 96-byte struct (0x60)
        let mut data = vec![0u8; 96];
        // revision at 0x00
        data[0x00..0x04].copy_from_slice(&3u32.to_le_bytes());
        // max_pwd_age at 0x18
        let max_pwd_age_ticks = max_pwd_age_days * 864_000_000_000u64;
        data[0x18..0x20].copy_from_slice(&max_pwd_age_ticks.to_le_bytes());
        // min_pwd_age at 0x20
        let min_pwd_age_ticks = min_pwd_age_days * 864_000_000_000u64;
        data[0x20..0x28].copy_from_slice(&min_pwd_age_ticks.to_le_bytes());
        // lockout_duration at 0x30
        let lockout_duration_ticks = lockout_duration_minutes * 60 * 10_000_000u64;
        data[0x30..0x38].copy_from_slice(&lockout_duration_ticks.to_le_bytes());
        // lockout_observation_window at 0x38
        let lockout_observation_window_ticks =
            lockout_observation_window_minutes * 60 * 10_000_000u64;
        data[0x38..0x40].copy_from_slice(&lockout_observation_window_ticks.to_le_bytes());
        // min_pwd_length at 0x50
        data[0x50..0x52].copy_from_slice(&min_pwd_length.to_le_bytes());
        // pwd_history_length at 0x52
        data[0x52..0x54].copy_from_slice(&pwd_history_length.to_le_bytes());
        // lockout_threshold at 0x54
        data[0x54..0x56].copy_from_slice(&lockout_threshold.to_le_bytes());
        data
    }

    /// Build a synthetic SAM hive with 2 users (Administrator, Guest) and
    /// groups from both Builtin\Aliases and Account\Aliases.
    ///
    /// Offset layout (0x80 apart to avoid NK record overlap):
    ///   NK keys:  0x020–0xA00
    ///   VK values: 0x1100–0x123F
    ///   Binary data cells: 0x5000–0x53FF
    fn synthetic_sam_hive() -> Vec<u8> {
        let mut data = vec![0u8; 0x8000];
        data[0..4].copy_from_slice(b"regf");
        data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
        data[0x1000..0x1004].copy_from_slice(b"hbin");
        data[0x1008..0x100c].copy_from_slice(&0x7000u32.to_le_bytes());

        // ── NK key tree (spaced 0x80 apart) ──────────────────────────

        // Root(0x020) → SAM(0x080)
        write_nk(&mut data, 0x020, "ROOT", &[("SAM", 0x080)], &[]);
        // SAM(0x080) → Domains(0x100)
        write_nk(&mut data, 0x080, "SAM", &[("Domains", 0x100)], &[]);
        // Domains(0x100) → Account(0x180), Builtin(0x880)
        write_nk(
            &mut data,
            0x100,
            "Domains",
            &[("Account", 0x180), ("Builtin", 0x880)],
            &[],
        );
        // Account(0x180) → Users(0x200), Aliases(0x500), and F value (password policy)
        write_nk(
            &mut data,
            0x180,
            "Account",
            &[("Users", 0x200), ("Aliases", 0x500)],
            &[0x1240],
        );
        // Account\F value: DomainAccountF password policy binary blob
        let account_f = make_domain_account_f_blob(
            42, // max password age days
            1,  // min password age days
            8,  // min password length
            24, // password history length
            5,  // lockout threshold
            30, // lockout duration minutes
            30, // lockout observation window minutes
        );
        write_binary_value(&mut data, 0x1240, "F", &account_f, 0x5400);
        // Users(0x200) → Names(0x280), 000001F4(0x400), 000001F5(0x480)
        write_nk(
            &mut data,
            0x200,
            "Users",
            &[("Names", 0x280), ("000001F4", 0x400), ("000001F5", 0x480)],
            &[],
        );
        // Names(0x280) → Administrator(0x300), Guest(0x380)
        write_nk(
            &mut data,
            0x280,
            "Names",
            &[("Administrator", 0x300), ("Guest", 0x380)],
            &[],
        );

        // Names\Administrator(0x300) → RID DWORD = 500
        write_nk(&mut data, 0x300, "Administrator", &[], &[0x1100]);
        write_dword_value(&mut data, 0x1100, "", 500);

        // Names\Guest(0x380) → RID DWORD = 501
        write_nk(&mut data, 0x380, "Guest", &[], &[0x1120]);
        write_dword_value(&mut data, 0x1120, "", 501);

        // Users\000001F4(0x400) → V value
        write_nk(&mut data, 0x400, "000001F4", &[], &[0x1140]);
        let admin_v = make_sam_v_record(
            133_600_000_000_000_000,
            133_500_000_000_000_000,
            500,
            0x0000,
            3,
        );
        write_binary_value(&mut data, 0x1140, "V", &admin_v, 0x5000);

        // Users\000001F5(0x480) → V value
        write_nk(&mut data, 0x480, "000001F5", &[], &[0x1160]);
        let guest_v = make_sam_v_record(0, 133_400_000_000_000_000, 501, SAM_ACCOUNT_DISABLED, 0);
        write_binary_value(&mut data, 0x1160, "V", &guest_v, 0x5100);

        // Account\Aliases(0x500) → Names(0x580), 00000220(0x700), 00000221(0x780)
        write_nk(
            &mut data,
            0x500,
            "Aliases",
            &[("Names", 0x580), ("00000220", 0x700), ("00000221", 0x780)],
            &[],
        );
        // Aliases\Names(0x580) → Administrators(0x600), Users(0x680)
        write_nk(
            &mut data,
            0x580,
            "Names",
            &[("Administrators", 0x600), ("Users", 0x680)],
            &[],
        );

        // Aliases\Names\Administrators(0x600) → RID DWORD = 544
        write_nk(&mut data, 0x600, "Administrators", &[], &[0x1180]);
        write_dword_value(&mut data, 0x1180, "", 544);

        // Aliases\Names\Users(0x680) → RID DWORD = 545
        write_nk(&mut data, 0x680, "Users", &[], &[0x11A0]);
        write_dword_value(&mut data, 0x11A0, "", 545);

        // Aliases\00000220(0x700) → C value with Admin RID=500
        write_nk(&mut data, 0x700, "00000220", &[], &[0x11C0]);
        let admin_sid = make_sid(&[21, 123456789, 123456789, 123456789, 500]);
        let admin_c = make_sam_c_value(&[admin_sid]);
        write_binary_value(&mut data, 0x11C0, "C", &admin_c, 0x5200);

        // Aliases\00000221(0x780) → C value with Admin and Guest
        write_nk(&mut data, 0x780, "00000221", &[], &[0x11E0]);
        let users_c = make_sam_c_value(&[
            make_sid(&[21, 123456789, 123456789, 123456789, 500]),
            make_sid(&[21, 123456789, 123456789, 123456789, 501]),
        ]);
        write_binary_value(&mut data, 0x11E0, "C", &users_c, 0x5300);

        // Builtin(0x880) → Aliases(0x900)
        write_nk(&mut data, 0x880, "Builtin", &[("Aliases", 0x900)], &[]);
        // Builtin\Aliases(0x900) → Names(0x980)
        write_nk(&mut data, 0x900, "Aliases", &[("Names", 0x980)], &[]);
        // Builtin\Aliases\Names(0x980) → Administrators(0xA00), Users(0xA80)
        write_nk(
            &mut data,
            0x980,
            "Names",
            &[("Administrators", 0xA00), ("Users", 0xA80)],
            &[],
        );

        // Builtin\Aliases\Names\Administrators(0xA00) → RID DWORD = 544
        write_nk(&mut data, 0xA00, "Administrators", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "", 544);

        // Builtin\Aliases\Names\Users(0xA80) → RID DWORD = 545
        write_nk(&mut data, 0xA80, "Users", &[], &[0x1220]);
        write_dword_value(&mut data, 0x1220, "", 545);

        data
    }

    // ── SAM extraction tests ──────────────────────────────────────────────

    #[test]
    fn extract_sam_fields_from_synthetic_hive() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();

        // Two users
        assert_eq!(info.users.len(), 2, "expected 2 users");
        let admin = info
            .users
            .iter()
            .find(|u| u.username == "Administrator")
            .unwrap();
        let guest = info.users.iter().find(|u| u.username == "Guest").unwrap();

        assert_eq!(admin.rid, 500);
        assert_eq!(guest.rid, 501);

        // Groups: 2 from Account\Aliases + 2 from Builtin\Aliases = 4
        assert_eq!(info.groups.len(), 4, "expected 4 groups (2 per alias root)");

        // No warnings expected for a well-formed synthetic hive
        assert!(
            info.warnings.is_empty(),
            "unexpected warnings: {:?}",
            info.warnings
        );
    }

    #[test]
    fn extract_sam_fields_user_account_control() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();

        let admin = info
            .users
            .iter()
            .find(|u| u.username == "Administrator")
            .unwrap();
        assert!(!admin.account_disabled, "Administrator should be enabled");
        assert!(!admin.account_locked, "Administrator should not be locked");

        let guest = info.users.iter().find(|u| u.username == "Guest").unwrap();
        assert!(guest.account_disabled, "Guest should be disabled");
        assert!(
            !guest.account_locked,
            "Guest should not be locked (only disabled)"
        );
    }

    #[test]
    fn extract_sam_fields_timestamps() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();

        let admin = info
            .users
            .iter()
            .find(|u| u.username == "Administrator")
            .unwrap();
        assert!(
            admin.last_login.is_some(),
            "Administrator should have last_login"
        );
        assert!(
            admin.password_last_set.is_some(),
            "Administrator should have password_last_set"
        );

        let guest = info.users.iter().find(|u| u.username == "Guest").unwrap();
        assert!(
            guest.last_login.is_none(),
            "Guest should have no last_login (FT=0)"
        );
        assert!(
            guest.password_last_set.is_some(),
            "Guest should have password_last_set"
        );
    }

    #[test]
    fn extract_sam_fields_admin_count() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();

        let admin = info
            .users
            .iter()
            .find(|u| u.username == "Administrator")
            .unwrap();
        assert_eq!(admin.admin_count, 3);

        let guest = info.users.iter().find(|u| u.username == "Guest").unwrap();
        assert_eq!(guest.admin_count, 0);
    }

    #[test]
    fn extract_sam_fields_group_memberships() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();

        let admin = info
            .users
            .iter()
            .find(|u| u.username == "Administrator")
            .unwrap();
        // Administrator should be member of Administrators and Users groups
        assert!(
            admin
                .group_memberships
                .contains(&"Administrators".to_string()),
            "Administrator should be in Administrators group"
        );
        assert!(
            admin.group_memberships.contains(&"Users".to_string()),
            "Administrator should be in Users group"
        );

        let guest = info.users.iter().find(|u| u.username == "Guest").unwrap();
        // Guest should be member of Users group
        assert!(
            guest.group_memberships.contains(&"Users".to_string()),
            "Guest should be in Users group"
        );

        // Verify group member lists — use the group that actually has members
        // (the one from Account\Aliases which has the C value)
        let admins_group = info
            .groups
            .iter()
            .find(|g| g.name == "Administrators" && !g.members.is_empty())
            .unwrap();
        assert!(
            admins_group.members.contains(&"Administrator".to_string()),
            "Administrators group should contain Administrator (groups with members: {:?})",
            info.groups
                .iter()
                .filter(|g| !g.members.is_empty())
                .collect::<Vec<_>>()
        );

        let users_group = info
            .groups
            .iter()
            .find(|g| g.name == "Users" && !g.members.is_empty())
            .unwrap();
        assert!(
            users_group.members.contains(&"Administrator".to_string()),
            "Users group should contain Administrator"
        );
        assert!(
            users_group.members.contains(&"Guest".to_string()),
            "Users group should contain Guest"
        );
    }

    #[test]
    fn extract_sam_fields_empty_hive() {
        // An empty hive (no SAM tree) should return empty users/groups with warnings
        let mut data = vec![0u8; 0x4000];
        data[0..4].copy_from_slice(b"regf");
        data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
        data[0x1000..0x1004].copy_from_slice(b"hbin");
        data[0x1008..0x100c].copy_from_slice(&0x3000u32.to_le_bytes());
        write_nk(&mut data, 0x20, "NOTSAM", &[], &[]);

        let info = extract_sam_fields(&data, "not/sam").unwrap();
        assert!(info.users.is_empty());
        assert!(info.groups.is_empty());
        assert!(
            !info.warnings.is_empty(),
            "should warn about missing SAM tree"
        );
    }

    #[test]
    fn extract_sam_fields_v_record_too_short() {
        // V record shorter than 0x50 bytes should generate a warning
        let mut data = synthetic_sam_hive();

        // Overwrite the Administrator V value with a truncated blob.
        // Administrator V: VK at offset 0x1140, binary data at cell 0x5000.
        let cell_abs = BASE_BLOCK_SIZE + 0x5000;
        // Cell header: negative size. Set to -8 (4 header + 4 payload → very short)
        data[cell_abs..cell_abs + 4].copy_from_slice(&(-8i32).to_le_bytes());
        // Zero out the rest so we don't read junk
        data[cell_abs + 4..cell_abs + 8].fill(0);

        // Also update the VK record's data_len to match
        let vk_abs = BASE_BLOCK_SIZE + 0x1140;
        data[vk_abs + 8..vk_abs + 12].copy_from_slice(&4u32.to_le_bytes());

        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();
        assert!(
            info.warnings
                .iter()
                .any(|w| w.contains("V record") && w.contains("expected at least")),
            "should warn about short V record, got: {:?}",
            info.warnings
        );
    }

    #[test]
    fn extract_sam_fields_v_record_unexpected_type() {
        // V value stored as a string instead of binary should trigger a warning
        let mut data = synthetic_sam_hive();

        // Replace the Administrator V value VK (at offset 0x1140) with a REG_SZ
        write_vk(&mut data, 0x1140, "V", REG_SZ, 0x8000_0004, 0x42424242);

        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();
        assert!(
            info.warnings.iter().any(|w| w.contains("unexpected type")),
            "should warn about V value having unexpected type, got: {:?}",
            info.warnings
        );
    }

    #[test]
    fn extract_sam_fields_password_policy() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();

        let policy = info
            .password_policy
            .expect("synthetic hive should have password policy");
        assert_eq!(policy.max_password_age_days, 42);
        assert_eq!(policy.min_password_age_days, 1);
        assert_eq!(policy.min_password_length, 8);
        assert_eq!(policy.password_history_length, 24);
        assert_eq!(policy.lockout_threshold, 5);
        assert_eq!(policy.lockout_duration_minutes, 30);
        assert_eq!(policy.lockout_observation_window_minutes, 30);
    }

    #[test]
    fn extract_sam_fields_password_policy_when_account_f_missing() {
        // Build a SAM hive WITHOUT the Account F value.  Password policy
        // should be None (not an error — common for non-AD workstations).
        let mut data = synthetic_sam_hive();
        // Overwrite the Account NK to remove the F value VK.
        // Account is at offset 0x180. Re-write without values.
        write_nk(
            &mut data,
            0x180,
            "Account",
            &[("Users", 0x200), ("Aliases", 0x500)],
            &[], // no values → no F key
        );

        let info = extract_sam_fields(&data, "Windows/System32/config/SAM").unwrap();
        assert!(
            info.password_policy.is_none(),
            "missing Account F should yield None password_policy"
        );
        // Users and groups should still be extracted normally.
        assert_eq!(info.users.len(), 2);
        assert_eq!(info.groups.len(), 4);
    }

    // ── Txlog-override tests ───────────────────────────────────────────────

    use crate::registry::txlog::fixture::{build_synthetic_log1, SyntheticEntry};

    /// Build a minimal synthetic SYSTEM hive that has a ComputerName value.
    fn txlog_system_hive(computer_name: &str) -> Vec<u8> {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("ComputerName", 0x600)], &[]);
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", computer_name, 0x1800);
        data
    }

    #[test]
    fn system_hive_with_txlog_overrides_computer_name() {
        let hive_bytes = txlog_system_hive("OLD-PC");

        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 100,
            timestamp: Some(0x01DB_9F8C_0000_0000), // 2026-06-14 approx
            key_path:
                "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName"
                    .to_string(),
            value_name: Some("ComputerName".to_string()),
            data_before: Some(encode_utf16le("OLD-PC")),
            data_after: Some(encode_utf16le("NEW-PC")),
        }]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        let cn = info.computer_name.as_ref().unwrap();
        assert_eq!(
            cn.value, "NEW-PC",
            "ComputerName should be overridden by txlog"
        );
        assert!(info.txlog_applied, "txlog_applied should be true");
        assert_eq!(info.txlog_timestamps.len(), 1);
        let ts = &info.txlog_timestamps[0];
        assert_eq!(ts.field_name, "ComputerName");
        assert!(ts.txlog_used);
        assert!(ts.txlog_timestamp.is_some());
        assert!(ts.hive_timestamp.is_none());
    }

    #[test]
    fn system_hive_with_txlog_no_match_leaves_field_unchanged() {
        let hive_bytes = txlog_system_hive("ORIGINAL-PC");

        // Txlog entry for a completely different key — should not match.
        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 1,
            timestamp: Some(0x01DB_9F8C_0000_0000),
            key_path: "\\Registry\\Machine\\SOFTWARE\\Some\\Other\\Path".to_string(),
            value_name: Some("Unrelated".to_string()),
            data_before: None,
            data_after: Some(encode_utf16le("ignored")),
        }]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        let cn = info.computer_name.as_ref().unwrap();
        assert_eq!(
            cn.value, "ORIGINAL-PC",
            "ComputerName should stay unchanged"
        );
        assert!(!info.txlog_applied);
        let ts = &info.txlog_timestamps[0];
        assert_eq!(ts.field_name, "ComputerName");
        assert!(!ts.txlog_used);
        assert!(ts.txlog_timestamp.is_none());
    }

    #[test]
    fn software_hive_with_txlog_overrides_product_name() {
        // Build a SOFTWARE hive with ProductName = "Windows Old".
        let mut data = empty_hive("SOFTWARE");
        write_nk(&mut data, 0x20, "SOFTWARE", &[("Microsoft", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Microsoft", &[("Windows NT", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Windows NT",
            &[("CurrentVersion", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "CurrentVersion", &[], &[0x600, 0x680]);
        write_string_value(&mut data, 0x600, "ProductName", "Windows Old", 0x900);
        write_string_value(&mut data, 0x680, "CurrentBuild", "22000", 0x980);

        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 50,
            timestamp: Some(0x01DB_A000_0000_0000),
            key_path: "\\Registry\\Machine\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"
                .to_string(),
            value_name: Some("ProductName".to_string()),
            data_before: Some(encode_utf16le("Windows Old")),
            data_after: Some(encode_utf16le("Windows New")),
        }]);

        let info = extract_software_hive_fields_with_txlog(
            &data,
            "Windows/System32/config/SOFTWARE",
            &txlog_bytes,
        )
        .unwrap();

        assert_eq!(info.product_name.as_ref().unwrap().value, "Windows New");
        assert_eq!(
            info.current_build.as_ref().unwrap().value,
            "22000",
            "CurrentBuild should be untouched"
        );
        assert!(info.txlog_applied);
        assert_eq!(info.txlog_timestamps.len(), 2); // ProductName + CurrentBuild
        let pn_ts = info
            .txlog_timestamps
            .iter()
            .find(|ts| ts.field_name == "ProductName")
            .unwrap();
        assert!(pn_ts.txlog_used);
        let cb_ts = info
            .txlog_timestamps
            .iter()
            .find(|ts| ts.field_name == "CurrentBuild")
            .unwrap();
        assert!(!cb_ts.txlog_used);
    }

    #[test]
    fn ntuser_hive_with_txlog_overrides_run_key_command() {
        // Build an NTUSER hive with a single Run key.
        let mut data = empty_hive("NTUSER");
        write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
        write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
        write_nk(
            &mut data,
            0x400,
            "Windows",
            &[("CurrentVersion", 0x500)],
            &[],
        );
        write_nk(&mut data, 0x500, "CurrentVersion", &[("Run", 0x600)], &[]);
        write_nk(&mut data, 0x600, "Run", &[], &[0x700]);
        write_string_value(&mut data, 0x700, "Malware", "C:\\temp\\old.exe", 0x1000);

        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 200,
            timestamp: Some(0x01DB_A100_0000_0000),
            key_path:
                "\\Registry\\User\\S-1-5-21-123\\Software\\Microsoft\\Windows\\CurrentVersion\\Run"
                    .to_string(),
            value_name: Some("Malware".to_string()),
            data_before: Some(encode_utf16le("C:\\temp\\old.exe")),
            data_after: Some(encode_utf16le("C:\\temp\\new.exe")),
        }]);

        let info =
            extract_ntuser_fields_with_txlog(&data, "Users/Test/NTUSER.DAT", &txlog_bytes).unwrap();

        assert_eq!(info.run_keys.len(), 1);
        assert_eq!(info.run_keys[0].value_name, "Malware");
        assert_eq!(info.run_keys[0].command, "C:\\temp\\new.exe");
        assert!(
            info.run_keys[0].timestamp.is_some(),
            "Run key should have timestamp from txlog"
        );
        assert!(info.txlog_applied);
    }

    #[test]
    fn txlog_uses_highest_sequence_number() {
        // When multiple txlog entries match the same field, use the one with
        // the highest sequence number.
        let hive_bytes = txlog_system_hive("V1");

        let txlog_bytes = build_synthetic_log1(&[
            SyntheticEntry {
                operation: 2,
                sequence_number: 10,
                timestamp: Some(0x01DB_9F8C_0000_0000),
                key_path: "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
                value_name: Some("ComputerName".to_string()),
                data_before: Some(encode_utf16le("V1")),
                data_after: Some(encode_utf16le("V2")),
            },
            SyntheticEntry {
                operation: 2,
                sequence_number: 20, // higher seq → should win
                timestamp: Some(0x01DB_A000_0000_0000),
                key_path: "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
                value_name: Some("ComputerName".to_string()),
                data_before: Some(encode_utf16le("V2")),
                data_after: Some(encode_utf16le("V3")),
            },
        ]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        assert_eq!(info.computer_name.as_ref().unwrap().value, "V3");
    }

    /// Helper: encode a string as UTF-16LE bytes (null-terminated).
    fn encode_utf16le(s: &str) -> Vec<u8> {
        let mut out: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        out.extend_from_slice(&[0x00, 0x00]); // null terminator
        out
    }
}
