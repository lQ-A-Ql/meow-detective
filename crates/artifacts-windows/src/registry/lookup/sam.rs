use super::*;

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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::test_common::*;
    use super::*;

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
}
