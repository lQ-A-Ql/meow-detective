use std::collections::HashMap;

use super::super::{RegistryHiveReader, RegistryValue, SamGroup, SamInfo};
use super::sid::parse_sid_at_end;
use super::users::find_rid_in_sam_key;

pub(super) fn extract_sam_aliases(
    hive: &RegistryHiveReader<'_>,
    alias_root: &[&str],
    machine_sid: &str,
    rid_to_username: &HashMap<u32, String>,
    info: &mut SamInfo,
) {
    let mut names_path = alias_root.to_vec();
    names_path.push("Names");
    let names_node = match hive.navigate_to(&names_path) {
        Ok(Some(node)) => node,
        Err(error) => {
            info.warnings
                .push(format!("{} parse error: {error}", names_path.join("\\")));
            return;
        }
        _ => return,
    };
    let names = match hive.read_subkey_names_from_nk(&names_node) {
        Ok(names) => names,
        Err(error) => {
            info.warnings
                .push(format!("{} subkeys error: {error}", names_path.join("\\")));
            return;
        }
    };
    for group_name in names {
        let mut name_path = names_path.clone();
        name_path.push(group_name.as_str());
        let Some(group_rid) = find_rid_in_sam_key(hive, &name_path, &mut info.warnings) else {
            continue;
        };
        let rid_hex = format!("{group_rid:08X}");
        let mut group_path = alias_root.to_vec();
        group_path.push(rid_hex.as_str());
        let members = match hive.lookup_value(&group_path, "C") {
            Ok(Some(RegistryValue::Binary(data))) => {
                parse_sam_c_members(&data, machine_sid, rid_to_username, &mut info.warnings)
            }
            Ok(Some(other)) => {
                info.warnings.push(format!(
                    "SAM group {}\\C value has unexpected type: {:?}",
                    group_path.join("\\"),
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
    rid_to_username: &HashMap<u32, String>,
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
    let mut members = Vec::new();
    let mut position = data.len();
    let mut misses = 0usize;
    while position >= 12 && misses <= 8 {
        match parse_sid_at_end(&data[..position]) {
            Some((sid, rid, sid_len)) => {
                let local = !machine_sid.is_empty()
                    && sid.starts_with(machine_sid)
                    && sid.as_bytes().get(machine_sid.len()) == Some(&b'-');
                let well_known = sid_len == 12;
                if !local && !well_known && !machine_sid.is_empty() {
                    misses += 1;
                    position = position.saturating_sub(1);
                    continue;
                }
                members.push(rid_to_username.get(&rid).cloned().unwrap_or_else(|| {
                    if local || well_known {
                        format!("rid-{rid}")
                    } else {
                        sid
                    }
                }));
                position = position.saturating_sub(sid_len);
                misses = 0;
            }
            None => {
                misses += 1;
                position = position.saturating_sub(1);
            }
        }
    }
    members.reverse();
    members
}
