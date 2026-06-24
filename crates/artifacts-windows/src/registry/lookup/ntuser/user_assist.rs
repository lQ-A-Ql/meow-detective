use crate::registry::lookup::reader::RegistryHiveReader;
use crate::registry::lookup::*;

// ── UserAssist ───────────────────────────────────────────────────────────────

pub(super) fn extract_user_assist(
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
