use super::reader::RegistryHiveReader;
use super::txlog_util::{
    find_best_txlog_match, find_best_txlog_match_user_assist, parse_user_assist_binary,
    txlog_data_to_string,
};
use super::*;
use crate::registry::RegistryError;

mod appcompat;
mod mount_points;
mod recent_files;
mod shell_folders;
mod user_assist;

#[cfg(test)]
#[path = "../../../../tests/unit/registry/lookup/ntuser.rs"]
mod tests;

pub use appcompat::extract_appcompat_layers_from_ntuser_hive;
pub(crate) use recent_files::decode_pidl_path;

// ── NTUSER.DAT field extraction ──────────────────────────────────────────────

pub fn extract_ntuser_fields(bytes: &[u8], hive_path: &str) -> Result<NtuserInfo, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = NtuserInfo::default();
    let parser = "registry.ntuser";

    info.run_keys = recent_files::extract_run_keys(&hive, hive_path, parser, &mut info.warnings);
    info.recent_docs =
        recent_files::extract_recent_docs(&hive, hive_path, parser, &mut info.warnings);
    info.ua_entries =
        user_assist::extract_user_assist(&hive, hive_path, parser, &mut info.warnings);
    info.typed_urls =
        shell_folders::extract_typed_urls(&hive, hive_path, parser, &mut info.warnings);
    info.word_wheel_query =
        shell_folders::extract_word_wheel_query(&hive, hive_path, parser, &mut info.warnings);
    info.mount_points =
        mount_points::extract_mount_points(&hive, hive_path, parser, &mut info.warnings);
    info.open_save_mru =
        recent_files::extract_open_save_mru(&hive, hive_path, parser, &mut info.warnings);
    info.last_visited_mru =
        recent_files::extract_last_visited_mru(&hive, hive_path, parser, &mut info.warnings);
    info.run_mru = recent_files::extract_run_mru(&hive, hive_path, parser, &mut info.warnings);
    info.default_browser = shell_folders::extract_default_browser(&hive);

    Ok(info)
}

/// Like [`extract_ntuser_fields`], but after standard extraction checks a
/// transaction log for more recent writes to Run / RunOnce keys and TypedURLs.
pub fn extract_ntuser_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<NtuserInfo, RegistryError> {
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
