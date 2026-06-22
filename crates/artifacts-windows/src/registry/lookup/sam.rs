use super::reader::RegistryHiveReader;
use super::txlog_util::find_best_txlog_match;
use super::*;
use crate::registry::txlog::parse_and_merge_txlogs;
use crate::registry::RegistryError;

/// Compute the machine SID prefix from the raw `SAM\Domains\Account\V` value.
fn machine_sid_from_v_data(v_data: &[u8], warnings: &mut Vec<String>) -> String {
    const SID_OFFSET: usize = 408;
    if v_data.len() < SID_OFFSET + 16 {
        warnings.push(format!(
            "SAM Domain Account V value is too short to contain machine SID ({} bytes)",
            v_data.len()
        ));
        return String::new();
    }

    let read_u32 = |off: usize| {
        u32::from_le_bytes(v_data[off..off + 4].try_into().expect("slice length is 4"))
    };

    let domain_id = read_u32(SID_OFFSET);
    let sid1 = read_u32(SID_OFFSET + 4);
    let sid2 = read_u32(SID_OFFSET + 8);
    let sid3 = read_u32(SID_OFFSET + 12);

    format!("S-1-5-{}-{}-{}-{}", domain_id, sid1, sid2, sid3)
}

/// Extract the machine SID from the SAM `Domains\Account\V` value.
///
/// The value stores the domain SID prefix as four little-endian DWORDs
/// starting at offset 408 (0x198): the authority/sub-authority count and
/// three random sub-authorities.  This matches the layout used by
/// impacket/secretsdump and ForensicsTool.
fn extract_machine_sid(hive: &RegistryHiveReader<'_>, warnings: &mut Vec<String>) -> String {
    let v_data = match hive.lookup_value(&["SAM", "Domains", "Account"], "V") {
        Ok(Some(RegistryValue::Binary(data))) => data,
        Ok(Some(other)) => {
            warnings.push(format!(
                "SAM Domain Account V value has unexpected type: {:?}",
                other
            ));
            return String::new();
        }
        Ok(None) => {
            warnings.push("SAM Domain Account V value not found".to_string());
            return String::new();
        }
        Err(err) => {
            warnings.push(format!("SAM Domain Account V parse error: {err}"));
            return String::new();
        }
    };

    machine_sid_from_v_data(&v_data, warnings)
}

/// Extract local user accounts, groups, and memberships from a SAM registry hive.
pub fn extract_sam_fields(
    bytes: &[u8],
    hive_path: &str,
    boot_key: Option<[u8; 16]>,
) -> Result<SamInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = SamInfo::default();
    let machine_sid = extract_machine_sid(&hive, &mut info.warnings);

    // Derive the SAM hashed boot key once if a SYSTEM BootKey was supplied.
    // This must happen before user extraction so password hashes can be
    // decrypted while the V records are read.
    let hashed_boot_key: Option<[u8; 32]> =
        boot_key.and_then(
            |bk| match hive.lookup_value(&["SAM", "Domains", "Account"], "F") {
                Ok(Some(RegistryValue::Binary(f_data))) => {
                    crate::registry::hash_decrypt::derive_hashed_boot_key(bk, &f_data)
                }
                _ => None,
            },
        );

    if boot_key.is_some() && hashed_boot_key.is_none() {
        info.warnings.push(
            "SAM hash decryption diagnostic: boot_key_present=true, \"reason\": \"SAM\\Domains\\Account\\F value missing or not binary\""
                .to_string(),
        );
    }

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
        if let Some(user) = extract_sam_user(
            &hive,
            username,
            *rid,
            &machine_sid,
            hashed_boot_key,
            &mut info.warnings,
        ) {
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
                            if let Some(user) = extract_sam_user(
                                &hive,
                                &username,
                                hex_rid,
                                &machine_sid,
                                hashed_boot_key,
                                &mut info.warnings,
                            ) {
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
        extract_sam_aliases(&hive, alias_root, &machine_sid, &rid_to_username, &mut info);
    }

    // ── Domain password policy ──────────────────────────────────────────────
    // The Account key's F value contains the domain-wide password policy.
    let account_path: &[&str] = &["SAM", "Domains", "Account"];
    match hive.lookup_value(account_path, "F") {
        Ok(Some(RegistryValue::Binary(f_data))) => {
            info.password_policy = crate::registry::sam_structs::parse_domain_account_f(&f_data);
        }
        Ok(Some(_)) => {
            info.warnings
                .push("SAM\\Domains\\Account\\F: unexpected value type (expected binary)".into());
        }
        // Missing F value is common for non-AD systems — not a warning.
        Ok(None) => {}
        Err(e) => {
            info.warnings.push(format!(
                "SAM\\Domains\\Account\\F: failed to read value: {e}"
            ));
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

/// Like [`extract_sam_fields`], but overlays newer values from .LOG1/.LOG2
/// transaction-log entries before returning the result.
///
/// Corrupt or missing transaction logs are treated as non-fatal warnings; the
/// base hive result is still returned.
pub fn extract_sam_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    boot_key: Option<[u8; 16]>,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
) -> Result<SamInfo, RegistryError> {
    let mut info = extract_sam_fields(bytes, hive_path, boot_key)?;
    let (transactions, txlog_warnings) = parse_and_merge_txlogs(txlog1, txlog2);
    info.warnings.extend(txlog_warnings);
    if transactions.is_empty() {
        return Ok(info);
    }

    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    // Current machine SID prefix derived from the first user SID, if any.
    let mut machine_sid = info
        .users
        .iter()
        .find(|u| !u.sid.is_empty())
        .and_then(|u| u.sid.rfind('-').map(|i| u.sid[..i].to_string()))
        .unwrap_or_default();

    // Domain password policy may be updated via SAM\Domains\Account\F.
    if let Some(txn) = find_best_txlog_match(&transactions, r"SAM\Domains\Account", "F") {
        if let Some(data) = txn.data_after.as_deref() {
            if let Some(policy) = crate::registry::sam_structs::parse_domain_account_f(data) {
                info.password_policy = Some(policy);
                txlog_applied = true;
            }
        }
        ts_infos.push(TxlogTimestampInfo {
            field_name: "passwordPolicy".to_string(),
            hive_timestamp: None,
            txlog_timestamp: txn.timestamp,
            txlog_used: txn.data_after.is_some(),
        });
    }

    // Machine SID prefix may be updated via SAM\Domains\Account\V.
    if let Some(txn) = find_best_txlog_match(&transactions, r"SAM\Domains\Account", "V") {
        if let Some(data) = txn.data_after.as_deref() {
            let new_sid = machine_sid_from_v_data(data, &mut info.warnings);
            if !new_sid.is_empty() && new_sid != machine_sid {
                machine_sid = new_sid;
                txlog_applied = true;
            }
        }
        ts_infos.push(TxlogTimestampInfo {
            field_name: "machineSid".to_string(),
            hive_timestamp: None,
            txlog_timestamp: txn.timestamp,
            txlog_used: txn.data_after.is_some(),
        });
    }

    // Per-user string fields / account control may be updated via the V value.
    for idx in 0..info.users.len() {
        let rid_hex = format!("{:08X}", info.users[idx].rid);
        let user_v_path = format!(r"SAM\Domains\Account\Users\{}", rid_hex);

        if let Some(txn) = find_best_txlog_match(&transactions, &user_v_path, "V") {
            if let Some(data) = txn.data_after.as_deref() {
                if let Some(profile) = crate::registry::sam_structs::parse_user_v(data) {
                    let user = &mut info.users[idx];
                    user.full_name = profile.full_name;
                    user.comment = profile.comment;
                    user.home_dir = profile.home_dir;
                    user.profile_path = profile.profile_path;
                    txlog_applied = true;
                }
                if let Some((ll, pls, _rid, account_control, admin_count)) =
                    parse_sam_v_record(data, &mut info.warnings)
                {
                    let user = &mut info.users[idx];
                    user.last_login = filetime_to_utc(ll);
                    user.password_last_set = filetime_to_utc(pls);
                    user.account_disabled = (account_control & SAM_ACCOUNT_DISABLED) != 0;
                    user.account_locked = (account_control & SAM_ACCOUNT_LOCKED) != 0;
                    user.admin_count = admin_count;
                    txlog_applied = true;
                }
            }
            ts_infos.push(TxlogTimestampInfo {
                field_name: format!("userV:{}", rid_hex),
                hive_timestamp: None,
                txlog_timestamp: txn.timestamp,
                txlog_used: txn.data_after.is_some(),
            });
        }

        // Timestamps/logon counts may also be updated via the F value.
        if let Some(txn) = find_best_txlog_match(&transactions, &user_v_path, "F") {
            if let Some(data) = txn.data_after.as_deref() {
                if let Some((ll, pls, _user_attr, lc)) =
                    parse_sam_f_record(data, &mut info.warnings)
                {
                    let user = &mut info.users[idx];
                    user.last_login = filetime_to_utc(ll);
                    user.password_last_set = filetime_to_utc(pls);
                    user.login_count = lc;
                    txlog_applied = true;
                }
            }
            ts_infos.push(TxlogTimestampInfo {
                field_name: format!("userF:{}", rid_hex),
                hive_timestamp: None,
                txlog_timestamp: txn.timestamp,
                txlog_used: txn.data_after.is_some(),
            });
        }
    }

    // If the machine SID prefix changed, recompute every user SID.
    if !machine_sid.is_empty() {
        for user in &mut info.users {
            user.sid = format!("{}-{}", machine_sid, user.rid);
        }
    }

    info.txlog_applied = txlog_applied;
    info.txlog_timestamps = ts_infos;
    Ok(info)
}

// ── SAM helpers ──────────────────────────────────────────────────────────────

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
            // SAM stores the RID as the default value's data_type field
            // (not in the value data). REG_NONE default value on Win10/11
            // has data_type = RID (e.g., 500 = 0x1F4 for Administrator).
            // This matches ForensicsTool Go implementation:
            //   _, valType, _ := userKey.GetValue("(default)", []byte{})
            //   rid := fmt.Sprintf("%08x", valType)
            if (500..2000).contains(&data_type) {
                return Some(data_type);
            }
            // Also check REG_NONE (0) or REG_DWORD (4) as fallback
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
    machine_sid: &str,
    hashed_boot_key: Option<[u8; 32]>,
    warnings: &mut Vec<String>,
) -> Option<SamUser> {
    let rid_hex = format!("{:08X}", rid);
    let user_key: &[&str] = &["SAM", "Domains", "Account", "Users"];

    // Build path: SAM\Domains\Account\Users\<RID_HEX>
    let mut user_path: Vec<&str> = user_key.to_vec();
    user_path.push(rid_hex.as_str());

    // Read the V value (string fields / profile).
    let v_data = match hive.lookup_value(&user_path, "V") {
        Ok(Some(RegistryValue::Binary(data))) => data,
        Ok(Some(other)) => {
            warnings.push(format!(
                "SAM user {}\\V value has unexpected type: {:?}",
                user_path.join("\\"),
                other
            ));
            return None;
        }
        Ok(None) => {
            warnings.push(format!("SAM user {}\\V not found", user_path.join("\\")));
            return None;
        }
        Err(err) => {
            warnings.push(format!(
                "SAM user {}\\V parse error: {err}",
                user_path.join("\\")
            ));
            return None;
        }
    };

    // Profile strings come from the V record.
    let profile = crate::registry::sam_structs::parse_user_v(&v_data).unwrap_or_default();

    // Parse the V record for string fields, account control flags and admin_count.
    let v_parsed = parse_sam_v_record(&v_data, warnings);

    // Timestamps and login count come from the F binary blob when present.
    // Account control flags, however, are unreliable in the F record on
    // Windows 10/11 (all local accounts may share the same user_attribute),
    // so we use the V record's account_control for status determination.
    let f_data = match hive.lookup_value(&user_path, "F") {
        Ok(Some(RegistryValue::Binary(data))) => Some(data),
        Ok(Some(other)) => {
            warnings.push(format!(
                "SAM user {}\\F value has unexpected type: {:?}",
                user_path.join("\\"),
                other
            ));
            None
        }
        _ => None,
    };

    let (last_login, password_last_set, login_count) = if let Some(f) = f_data.as_deref() {
        if let Some((ll, pls, _, lc)) = parse_sam_f_record(f, warnings) {
            (Some(ll), Some(pls), lc)
        } else {
            let (ll, pls, _, _, _) = v_parsed?;
            (Some(ll), Some(pls), 0)
        }
    } else {
        let (ll, pls, _, _, _) = v_parsed?;
        (Some(ll), Some(pls), 0)
    };

    // Account control flags come from the V record; fall back to the F
    // record's user_attribute if the V record is unusable.
    let account_control = v_parsed.map(|(_, _, _, ac, _)| ac).unwrap_or_else(|| {
        f_data
            .as_deref()
            .and_then(|f| parse_sam_f_record(f, warnings))
            .map(|(_, _, ac, _)| ac)
            .unwrap_or(0)
    });

    // The F record's rid is authoritative; fall back to the caller-supplied
    // rid if it cannot be parsed.
    let effective_rid = f_data
        .as_deref()
        .and_then(crate::registry::sam_structs::parse_user_f)
        .map(|(f_rid, _, _)| f_rid)
        .unwrap_or(rid);
    if effective_rid != rid {
        warnings.push(format!(
            "SAM user {} F record RID {} does not match expected RID {}",
            username, effective_rid, rid
        ));
    }

    // admin_count is only present in the V record header.
    let admin_count = v_parsed.map(|(_, _, _, _, ac)| ac).unwrap_or(0);

    let account_disabled = (account_control & SAM_ACCOUNT_DISABLED) != 0;
    let account_locked = (account_control & SAM_ACCOUNT_LOCKED) != 0;

    let sid = if machine_sid.is_empty() {
        String::new()
    } else {
        format!("{}-{}", machine_sid, effective_rid)
    };

    // Decrypt password hashes when the SYSTEM BootKey was available.
    let mut decrypt_reason: Option<&'static str> = None;
    let (password_hash, password_hash_type) = if let Some(hbk) = hashed_boot_key {
        match crate::registry::hash_decrypt::decrypt_user_hashes(hbk, effective_rid, &v_data) {
            Some(hashes) => {
                let has_lm = hashes.lm != crate::registry::hash_decrypt::LM_HASH_EMPTY;
                let has_nt = hashes.nt != crate::registry::hash_decrypt::NT_HASH_EMPTY;
                let hash_str = format!("{}:{}", hashes.lm, hashes.nt);
                let hash_type = if has_lm && has_nt {
                    "Both"
                } else if has_lm {
                    "LM"
                } else {
                    "NTLM"
                };
                (Some(hash_str), Some(hash_type.to_string()))
            }
            None => {
                decrypt_reason = Some("failed to parse or decrypt V record hashes");
                (None, None)
            }
        }
    } else {
        decrypt_reason = Some("no SYSTEM BootKey available");
        (None, None)
    };

    if let Some(reason) = decrypt_reason {
        warnings.push(format!(
            "SAM hash decryption diagnostic: \"{}\": \"{}\", \"{}\": {}, \"{}\": {}, \"{}\": {}, \"{}\": {}, \"{}\": \"{}\"",
            "username", username,
            "rid", effective_rid,
            "boot_key_present", hashed_boot_key.is_some(),
            "f_record_present", f_data.is_some(),
            "v_record_length", v_data.len(),
            "reason", reason,
        ));
    }

    Some(SamUser {
        username: username.to_string(),
        rid: effective_rid,
        sid,
        full_name: profile.full_name,
        comment: profile.comment,
        home_dir: profile.home_dir,
        profile_path: profile.profile_path,
        last_login: last_login.and_then(filetime_to_utc),
        password_last_set: password_last_set.and_then(filetime_to_utc),
        account_disabled,
        account_locked,
        admin_count,
        login_count,
        group_memberships: Vec::new(), // populated later via cross-reference
        password_hash,
        password_hash_type,
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

/// Parse the SAM UserF binary blob.
///
/// Returns `(last_login, password_last_set, user_attribute, login_count)`.
fn parse_sam_f_record(data: &[u8], _warnings: &mut Vec<String>) -> Option<(u64, u64, u32, u32)> {
    use binread::BinRead;
    use std::io::Cursor;

    let mut cursor = Cursor::new(data);
    let user_f = crate::registry::sam_structs::UserFRaw::read(&mut cursor).ok()?;
    Some((
        user_f.last_login_time,
        user_f.last_pwd_change_time,
        user_f.user_attribute,
        user_f.logon_count as u32,
    ))
}

fn extract_sam_aliases(
    hive: &RegistryHiveReader<'_>,
    alias_root: &[&str],
    machine_sid: &str,
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
                parse_sam_c_members(&data, machine_sid, rid_to_username, &mut info.warnings)
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
    machine_sid: &str,
    rid_to_username: &std::collections::HashMap<u32, String>,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    if data.len() < 12 {
        if !data.is_empty() {
            warnings.push(format!(
                "SAM group C value is {} bytes, expected at least 12 for a member SID",
                data.len()
            ));
        }
        return Vec::new();
    }

    // Member SIDs are packed at the very end of the C value.  We scan backward
    // from the end, allowing a small run of non-member bytes (padding or the
    // security descriptor) before stopping.  This handles modern Windows SAM
    // where SIDs can end with zero bytes (e.g. RID 500 = 0x000001F4).
    let mut members = Vec::new();
    let mut pos = data.len();
    let mut miss = 0usize;
    const MAX_MISS: usize = 8;

    while pos >= 12 && miss <= MAX_MISS {
        let slice = &data[..pos];
        match parse_sid_at_end(slice) {
            Some((sid, rid, sid_len)) => {
                let is_local = !machine_sid.is_empty()
                    && sid.starts_with(machine_sid)
                    && sid.as_bytes().get(machine_sid.len()) == Some(&b'-');
                let is_wellknown = sid_len == 12;
                if !is_local && !is_wellknown && !machine_sid.is_empty() {
                    // Security descriptor SID, not a member.
                    miss += 1;
                    pos = pos.saturating_sub(1);
                    continue;
                }

                let display = if is_local || is_wellknown {
                    rid_to_username
                        .get(&rid)
                        .cloned()
                        .unwrap_or_else(|| format!("rid-{rid}"))
                } else {
                    // No machine SID to validate against: still try to resolve local RIDs.
                    rid_to_username
                        .get(&rid)
                        .cloned()
                        .unwrap_or_else(|| sid.clone())
                };
                members.push(display);
                pos = pos.saturating_sub(sid_len);
                miss = 0;
            }
            None => {
                miss += 1;
                pos = pos.saturating_sub(1);
            }
        }
    }

    members.reverse();
    members
}

/// Parse a SID whose last byte is at the end of `data`.
///
/// Local group members are either 12-byte well-known SIDs or 28-byte domain/
/// machine-account SIDs. Returns `(SID string, last sub-authority/RID, length)`.
fn parse_sid_at_end(data: &[u8]) -> Option<(String, u32, usize)> {
    // Try the larger SID first so a 28-byte SID is not mistaken for a 12-byte
    // suffix.
    for &len in [28usize, 12usize].iter() {
        if data.len() < len {
            continue;
        }
        let bytes = &data[data.len() - len..];
        if let Some(sid) = sid_bytes_to_string(bytes) {
            let rid = u32::from_le_bytes(bytes[len - 4..len].try_into().ok()?);
            return Some((sid, rid, len));
        }
    }
    None
}

/// Convert a raw SID byte blob to its canonical string form (e.g. S-1-5-21-...).
fn sid_bytes_to_string(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }
    let revision = data[0];
    let sub_auth_count = data[1] as usize;
    if sub_auth_count == 0 || sub_auth_count > 15 {
        return None;
    }
    let sid_len = 8usize.checked_add(sub_auth_count.checked_mul(4)?)?;
    if data.len() < sid_len {
        return None;
    }
    // Identifier authority is big-endian and typically 5 (NT Authority).
    let id_auth = u64::from_be_bytes([0, 0, data[2], data[3], data[4], data[5], data[6], data[7]]);
    let subs: Vec<u32> = (0..sub_auth_count)
        .map(|i| {
            let off = 8 + i * 4;
            u32::from_le_bytes(data[off..off + 4].try_into().expect("4 bytes"))
        })
        .collect();
    Some(format!(
        "S-{}-{}-{}",
        revision,
        id_auth,
        subs.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("-")
    ))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::test_common::*;
    use super::*;
    use crate::registry::txlog::fixture::{build_synthetic_log1, SyntheticEntry};

    #[test]
    fn extract_sam_fields_from_synthetic_hive() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();

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

        // Without a SYSTEM BootKey we cannot decrypt hashes, so each user
        // records a structured diagnostic warning instead of failing silently.
        assert_eq!(info.warnings.len(), 2, "expected one diagnostic per user");
        assert!(
            info.warnings
                .iter()
                .any(|w| w.contains("\"username\": \"Administrator\"")),
            "missing Administrator diagnostic: {:?}",
            info.warnings
        );
        assert!(
            info.warnings
                .iter()
                .any(|w| w.contains("\"username\": \"Guest\"")),
            "missing Guest diagnostic: {:?}",
            info.warnings
        );
    }

    #[test]
    fn extract_sam_fields_user_account_control() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();

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
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();

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
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();

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
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();

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

        let info = extract_sam_fields(&data, "not/sam", None).unwrap();
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

        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();
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

        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();
        assert!(
            info.warnings.iter().any(|w| w.contains("unexpected type")),
            "should warn about V value having unexpected type, got: {:?}",
            info.warnings
        );
    }

    #[test]
    fn extract_sam_fields_password_policy() {
        let data = synthetic_sam_hive();
        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();

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

        let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();
        assert!(
            info.password_policy.is_none(),
            "missing Account F should yield None password_policy"
        );
        // Users and groups should still be extracted normally.
        assert_eq!(info.users.len(), 2);
        assert_eq!(info.groups.len(), 4);
    }

    #[test]
    fn extract_sam_fields_with_txlog_overrides_password_policy() {
        let data = synthetic_sam_hive();

        // Base hive password policy has max_password_age_days = 42.
        let base = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();
        let base_policy = base.password_policy.expect("base hive should have policy");
        assert_eq!(base_policy.max_password_age_days, 42);

        // Build a txlog that overrides Account\F with a new max password age.
        let txlog_f = make_domain_account_f_blob(
            7,  // max_password_age_days
            1,  // min_password_age_days
            8,  // min_password_length
            24, // password_history_length
            5,  // lockout_threshold
            30, // lockout_duration_minutes
            30, // lockout_observation_window_minutes
        );
        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 100,
            timestamp: Some(0x01DB_A100_0000_0000),
            key_path: r"\Registry\Machine\SAM\Domains\Account".to_string(),
            value_name: Some("F".to_string()),
            data_before: None,
            data_after: Some(txlog_f),
        }]);

        let info = extract_sam_fields_with_txlog(
            &data,
            "Windows/System32/config/SAM",
            None,
            Some(&txlog_bytes),
            None,
        )
        .unwrap();

        let policy = info
            .password_policy
            .expect("txlog override should keep policy");
        assert_eq!(
            policy.max_password_age_days, 7,
            "txlog F should override max password age"
        );
        assert!(info.txlog_applied, "txlog_applied should be true");
        assert!(
            info.txlog_timestamps
                .iter()
                .any(|ts| ts.field_name == "passwordPolicy"),
            "missing txlog timestamp for password policy: {:?}",
            info.txlog_timestamps
        );
    }
}
