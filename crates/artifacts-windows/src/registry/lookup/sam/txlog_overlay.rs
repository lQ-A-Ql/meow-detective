use super::super::txlog_util::find_best_txlog_match;
use super::super::{filetime_to_utc, TxlogTimestampInfo};
use super::extraction::extract_sam_fields;
use super::records::parse_sam_f_record;
use super::sid::machine_sid_from_v_data;
use crate::registry::txlog::parse_and_merge_txlogs;
use crate::registry::RegistryError;

pub fn extract_sam_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    boot_key: Option<[u8; 16]>,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
) -> Result<super::super::SamInfo, RegistryError> {
    let mut info = extract_sam_fields(bytes, hive_path, boot_key)?;
    let (transactions, warnings) = parse_and_merge_txlogs(txlog1, txlog2);
    info.warnings.extend(warnings);
    if transactions.is_empty() {
        return Ok(info);
    }
    let mut applied = false;
    let mut timestamps = Vec::new();
    let mut machine_sid = info
        .users
        .iter()
        .find(|user| !user.sid.is_empty())
        .and_then(|user| {
            user.sid
                .rfind('-')
                .map(|index| user.sid[..index].to_string())
        })
        .unwrap_or_default();

    if let Some(transaction) = find_best_txlog_match(&transactions, r"SAM\Domains\Account", "F") {
        if let Some(data) = transaction.data_after.as_deref() {
            if let Some(policy) = crate::registry::sam_structs::parse_domain_account_f(data) {
                info.password_policy = Some(policy);
                applied = true;
            }
        }
        timestamps.push(timestamp_info("passwordPolicy".to_string(), transaction));
    }
    if let Some(transaction) = find_best_txlog_match(&transactions, r"SAM\Domains\Account", "V") {
        if let Some(data) = transaction.data_after.as_deref() {
            let updated = machine_sid_from_v_data(data, &mut info.warnings);
            if !updated.is_empty() && updated != machine_sid {
                machine_sid = updated;
                applied = true;
            }
        }
        timestamps.push(timestamp_info("machineSid".to_string(), transaction));
    }
    for user in &mut info.users {
        let rid_hex = format!("{:08X}", user.rid);
        let path = format!(r"SAM\Domains\Account\Users\{rid_hex}");
        if let Some(transaction) = find_best_txlog_match(&transactions, &path, "V") {
            if let Some(data) = transaction.data_after.as_deref() {
                // Only profile fields are applied from V: its header words do
                // not carry timestamps, account-control flags or admin counts.
                if let Some(profile) = crate::registry::sam_structs::parse_user_v(data) {
                    user.full_name = profile.full_name;
                    user.comment = profile.comment;
                    user.home_dir = profile.home_dir;
                    user.profile_path = profile.profile_path;
                    applied = true;
                }
            }
            timestamps.push(timestamp_info(format!("userV:{rid_hex}"), transaction));
        }
        if let Some(transaction) = find_best_txlog_match(&transactions, &path, "F") {
            if let Some(data) = transaction.data_after.as_deref() {
                if let Some((last_login, password_set, _, login_count)) =
                    parse_sam_f_record(data, &mut info.warnings)
                {
                    user.last_login = filetime_to_utc(last_login);
                    user.password_last_set = filetime_to_utc(password_set);
                    user.login_count = login_count;
                    applied = true;
                }
            }
            timestamps.push(timestamp_info(format!("userF:{rid_hex}"), transaction));
        }
    }
    if !machine_sid.is_empty() {
        for user in &mut info.users {
            user.sid = format!("{}-{}", machine_sid, user.rid);
        }
    }
    info.txlog_applied = applied;
    info.txlog_timestamps = timestamps;
    Ok(info)
}

fn timestamp_info(
    field_name: String,
    transaction: &crate::registry::txlog::RegistryTransaction,
) -> TxlogTimestampInfo {
    TxlogTimestampInfo {
        field_name,
        hive_timestamp: None,
        txlog_timestamp: transaction.timestamp,
        txlog_used: transaction.data_after.is_some(),
    }
}
