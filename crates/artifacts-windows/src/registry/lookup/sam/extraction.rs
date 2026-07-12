use std::collections::HashMap;

use super::super::{RegistryHiveReader, RegistryValue, SamInfo};
use super::aliases::extract_sam_aliases;
use super::sid::extract_machine_sid;
use super::users::{build_sam_name_to_rid, extract_sam_user, recover_users_from_rid_keys};
use crate::registry::RegistryError;

pub fn extract_sam_fields(
    bytes: &[u8],
    hive_path: &str,
    boot_key: Option<[u8; 16]>,
) -> Result<SamInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = SamInfo::default();
    let machine_sid = extract_machine_sid(&hive, &mut info.warnings);
    let hashed_boot_key =
        boot_key.and_then(
            |key| match hive.lookup_value(&["SAM", "Domains", "Account"], "F") {
                Ok(Some(RegistryValue::Binary(data))) => {
                    crate::registry::hash_decrypt::derive_hashed_boot_key(key, &data)
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
    let names_path = ["SAM", "Domains", "Account", "Users", "Names"];
    let username_rid_map = build_sam_name_to_rid(&hive, &names_path, &mut info.warnings);
    if username_rid_map.is_empty() {
        info.warnings.push(format!(
            "{}: no user names found (hive={})",
            names_path.join("\\"),
            hive_path
        ));
    }
    let mut rid_to_username: HashMap<u32, String> = username_rid_map
        .iter()
        .map(|(username, rid)| (*rid, username.clone()))
        .collect();
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
    recover_users_from_rid_keys(
        &hive,
        &machine_sid,
        hashed_boot_key,
        &mut rid_to_username,
        &mut info.users,
        &mut info.warnings,
    );
    for alias_root in [
        ["SAM", "Domains", "Builtin", "Aliases"].as_slice(),
        ["SAM", "Domains", "Account", "Aliases"].as_slice(),
    ] {
        extract_sam_aliases(&hive, alias_root, &machine_sid, &rid_to_username, &mut info);
    }
    match hive.lookup_value(&["SAM", "Domains", "Account"], "F") {
        Ok(Some(RegistryValue::Binary(data))) => {
            info.password_policy = crate::registry::sam_structs::parse_domain_account_f(&data);
        }
        Ok(Some(_)) => info
            .warnings
            .push("SAM\\Domains\\Account\\F: unexpected value type (expected binary)".into()),
        Ok(None) => {}
        Err(error) => info.warnings.push(format!(
            "SAM\\Domains\\Account\\F: failed to read value: {error}"
        )),
    }
    for user in &mut info.users {
        for group in &info.groups {
            if group.members.contains(&user.username) {
                user.group_memberships.push(group.name.clone());
            }
        }
    }
    Ok(info)
}
