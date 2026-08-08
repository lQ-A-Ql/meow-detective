use std::collections::HashMap;

use super::super::{
    filetime_to_utc, RegistryHiveReader, RegistryValue, SamUser, REG_DWORD, SAM_ACCOUNT_DISABLED,
    SAM_ACCOUNT_LOCKED, VK_SIGNATURE,
};
use super::records::parse_sam_f_record;

pub(super) fn build_sam_name_to_rid(
    hive: &RegistryHiveReader<'_>,
    names_path: &[&str],
    warnings: &mut Vec<String>,
) -> Vec<(String, u32)> {
    let names_node = match hive.navigate_to(names_path) {
        Ok(Some(node)) => node,
        Ok(None) => return Vec::new(),
        Err(error) => {
            warnings.push(format!("SAM Users\\Names parse error: {error}"));
            return Vec::new();
        }
    };
    let names = match hive.read_subkey_names_from_nk(&names_node) {
        Ok(names) => names,
        Err(error) => {
            warnings.push(format!("SAM Users\\Names subkeys error: {error}"));
            return Vec::new();
        }
    };
    names
        .into_iter()
        .filter_map(|username| {
            let mut path = names_path.to_vec();
            path.push(username.as_str());
            match find_rid_in_sam_key(hive, &path, warnings) {
                Some(rid) => Some((username, rid)),
                None => {
                    warnings.push(format!("SAM user '{}' has no readable RID value", username));
                    None
                }
            }
        })
        .collect()
}

pub(super) fn find_rid_in_sam_key(
    hive: &RegistryHiveReader<'_>,
    key_path: &[&str],
    warnings: &mut Vec<String>,
) -> Option<u32> {
    let node = match hive.navigate_to(key_path) {
        Ok(Some(node)) => node,
        Err(error) => {
            warnings.push(format!("{} parse error: {error}", key_path.join("\\")));
            return None;
        }
        _ => return None,
    };
    if let Ok(values) = hive.read_all_values_from_nk(&node) {
        for (_, value) in values {
            match value {
                RegistryValue::Dword(value) => return Some(value),
                RegistryValue::Binary(data) if data.len() >= 4 => {
                    if let Some(rid) = data
                        .get(..4)
                        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                        .map(u32::from_le_bytes)
                    {
                        return Some(rid);
                    }
                }
                _ => {}
            }
        }
    }
    if let Ok(offsets) = hive.read_raw_vk_data_offsets(&node) {
        for offset in offsets {
            if let Some(rid) = read_inline_rid(hive, offset) {
                return Some(rid);
            }
        }
    }
    warnings.push(format!(
        "SAM key {} has no readable RID value (raw scan also failed)",
        key_path.join("\\"),
    ));
    None
}

fn read_inline_rid(hive: &RegistryHiveReader<'_>, value_offset: u32) -> Option<u32> {
    let absolute = hive.abs(value_offset).ok()?;
    if absolute + 0x14 > hive.bytes.len() || &hive.bytes[absolute + 4..absolute + 6] != VK_SIGNATURE
    {
        return None;
    }
    let data_type = u32::from_le_bytes(
        hive.bytes[absolute + 0x10..absolute + 0x14]
            .try_into()
            .ok()?,
    );
    if (500..2000).contains(&data_type) {
        return Some(data_type);
    }
    let data_len_raw = u32::from_le_bytes(
        hive.bytes[absolute + 0x08..absolute + 0x0c]
            .try_into()
            .ok()?,
    );
    let raw_data_offset = u32::from_le_bytes(
        hive.bytes[absolute + 0x0c..absolute + 0x10]
            .try_into()
            .ok()?,
    );
    ((data_type == 0 || data_type == REG_DWORD)
        && (data_len_raw & 0x7fff_ffff) <= 4
        && raw_data_offset > 0
        && raw_data_offset < 0xffff)
        .then_some(raw_data_offset)
}

pub(super) fn extract_sam_user(
    hive: &RegistryHiveReader<'_>,
    username: &str,
    rid: u32,
    machine_sid: &str,
    hashed_boot_key: Option<[u8; 32]>,
    warnings: &mut Vec<String>,
) -> Option<SamUser> {
    let rid_hex = format!("{rid:08X}");
    let mut path = vec!["SAM", "Domains", "Account", "Users"];
    path.push(rid_hex.as_str());
    let value_data = match hive.lookup_value(&path, "V") {
        Ok(Some(RegistryValue::Binary(data))) => data,
        Ok(Some(other)) => {
            warnings.push(format!(
                "SAM user {}\\V value has unexpected type: {:?}",
                path.join("\\"),
                other
            ));
            return None;
        }
        Ok(None) => {
            warnings.push(format!("SAM user {}\\V not found", path.join("\\")));
            return None;
        }
        Err(error) => {
            warnings.push(format!(
                "SAM user {}\\V parse error: {error}",
                path.join("\\")
            ));
            return None;
        }
    };
    let profile = crate::registry::sam_structs::parse_user_v(&value_data).unwrap_or_default();
    let f_data = match hive.lookup_value(&path, "F") {
        Ok(Some(RegistryValue::Binary(data))) => Some(data),
        Ok(Some(other)) => {
            warnings.push(format!(
                "SAM user {}\\F value has unexpected type: {:?}",
                path.join("\\"),
                other
            ));
            None
        }
        _ => None,
    };
    // The F record is the sole authoritative carrier of per-user timestamps,
    // logon counters and the ACB account-control flags. The V value holds no
    // such fields; its header words were formerly misread as timestamps,
    // flags and an admin count (removed as outdated semantics).
    let f_record = f_data
        .as_deref()
        .and_then(|data| parse_sam_f_record(data, warnings));
    let (last_login, password_last_set, login_count) = f_record
        .map(|record| (record.0, record.1, record.3))
        .unwrap_or((0, 0, 0));
    let account_control = f_record.map(|record| record.2).unwrap_or(0);
    let effective_rid = f_data
        .as_deref()
        .and_then(crate::registry::sam_structs::parse_user_f)
        .map(|record| record.0)
        .unwrap_or(rid);
    if effective_rid != rid {
        warnings.push(format!(
            "SAM user {} F record RID {} does not match expected RID {}",
            username, effective_rid, rid
        ));
    }
    let (password_hash, password_hash_type) = decrypt_hashes(
        username,
        effective_rid,
        hashed_boot_key,
        f_data.is_some(),
        &value_data,
        warnings,
    );
    Some(SamUser {
        username: username.to_string(),
        rid: effective_rid,
        sid: if machine_sid.is_empty() {
            String::new()
        } else {
            format!("{machine_sid}-{effective_rid}")
        },
        full_name: profile.full_name,
        comment: profile.comment,
        home_dir: profile.home_dir,
        profile_path: profile.profile_path,
        last_login: filetime_to_utc(last_login),
        password_last_set: filetime_to_utc(password_last_set),
        account_disabled: account_control & SAM_ACCOUNT_DISABLED != 0,
        account_locked: account_control & SAM_ACCOUNT_LOCKED != 0,
        // No reliable on-disk source in the SAM hive; kept for DTO stability.
        admin_count: 0,
        login_count,
        group_memberships: Vec::new(),
        password_hash,
        password_hash_type,
    })
}

fn decrypt_hashes(
    username: &str,
    rid: u32,
    hashed_boot_key: Option<[u8; 32]>,
    f_record_present: bool,
    value_data: &[u8],
    warnings: &mut Vec<String>,
) -> (Option<String>, Option<String>) {
    let mut reason = None;
    let result = if let Some(key) = hashed_boot_key {
        match crate::registry::hash_decrypt::decrypt_user_hashes(key, rid, value_data) {
            Some(hashes) => {
                let has_lm = hashes.lm != crate::registry::hash_decrypt::LM_HASH_EMPTY;
                let has_nt = hashes.nt != crate::registry::hash_decrypt::NT_HASH_EMPTY;
                let hash_type = if has_lm && has_nt {
                    "Both"
                } else if has_lm {
                    "LM"
                } else {
                    "NTLM"
                };
                (
                    Some(format!("{}:{}", hashes.lm, hashes.nt)),
                    Some(hash_type.to_string()),
                )
            }
            None => {
                reason = Some("failed to parse or decrypt V record hashes");
                (None, None)
            }
        }
    } else {
        reason = Some("no SYSTEM BootKey available");
        (None, None)
    };
    if let Some(reason) = reason {
        warnings.push(format!(
            "SAM hash decryption diagnostic: \"{}\": \"{}\", \"{}\": {}, \"{}\": {}, \"{}\": {}, \"{}\": {}, \"{}\": \"{}\"",
            "username", username,
            "rid", rid,
            "boot_key_present", hashed_boot_key.is_some(),
            "f_record_present", f_record_present,
            "v_record_length", value_data.len(),
            "reason", reason,
        ));
    }
    result
}

pub(super) fn recover_users_from_rid_keys(
    hive: &RegistryHiveReader<'_>,
    machine_sid: &str,
    hashed_boot_key: Option<[u8; 32]>,
    rid_to_username: &mut HashMap<u32, String>,
    users: &mut Vec<SamUser>,
    warnings: &mut Vec<String>,
) {
    let users_path = ["SAM", "Domains", "Account", "Users"];
    let Some(users_node) = hive.navigate_to(&users_path).ok().flatten() else {
        return;
    };
    let Ok(names) = hive.read_subkey_names_from_nk(&users_node) else {
        return;
    };
    for subkey_name in names {
        let Some(hex_rid) = (!subkey_name.eq_ignore_ascii_case("Names"))
            .then(|| u32::from_str_radix(&subkey_name, 16).ok())
            .flatten()
        else {
            continue;
        };
        if rid_to_username.contains_key(&hex_rid) {
            continue;
        }
        let mut path = users_path.to_vec();
        path.push(subkey_name.as_str());
        let Some(node) = hive.navigate_to(&path).ok().flatten() else {
            continue;
        };
        let username = hive
            .read_raw_value_bytes(&node, "V")
            .ok()
            .flatten()
            .and_then(|data| crate::registry::sam_structs::parse_username_from_v_record(&data));
        let f_rid = hive
            .read_raw_value_bytes(&node, "F")
            .ok()
            .flatten()
            .and_then(|data| crate::registry::sam_structs::parse_user_f(&data))
            .map(|record| record.0);
        if let (Some(username), Some(f_rid)) = (username, f_rid) {
            if f_rid == hex_rid {
                rid_to_username.insert(hex_rid, username.clone());
                if let Some(user) = extract_sam_user(
                    hive,
                    &username,
                    hex_rid,
                    machine_sid,
                    hashed_boot_key,
                    warnings,
                ) {
                    users.push(user);
                }
                warnings.push(format!(
                    "SAM: recovered user '{}' (RID={}) from Users\\{}\\F value (Names key REG_NONE fallback)",
                    username, hex_rid, subkey_name
                ));
            }
        }
    }
}
