//! Same-size, in-place SAM hive edits for the emulation bypass.
//!
//! Every edit keeps the hive byte length unchanged: hash blobs are rewritten
//! with the canonical empty LM/NT hashes (re-encrypted under the account's
//! RID keys), and account flags flip inside the existing V/F value data
//! cells. No cell is created, moved, or resized, so the hive stays valid
//! without touching hbin bookkeeping.

use zeroize::Zeroizing;

use super::hash_encrypt::{
    encrypt_hash_into_blob, EMPTY_LM_HASH, EMPTY_NT_HASH, LMPASSWORD_CONSTANT, NTPASSWORD_CONSTANT,
};
use super::lookup::RegistryHiveReader;

const USER_V_HEADER_LEN: usize = 204;
const V_LM_OFFSET_FIELD: usize = 0x9C;
const V_LM_LENGTH_FIELD: usize = 0xA0;
const V_NT_OFFSET_FIELD: usize = 0xA8;
const V_NT_LENGTH_FIELD: usize = 0xAC;
// User F layout per chntpw's sam.h (Petter Nordahl-Hagen): RID at 0x30,
// ACB flags u16 at 0x38, failed-login counter u16 at 0x40.
const F_ACB_OFFSET: usize = 0x38;
const F_FAILED_LOGON_OFFSET: usize = 0x40;
const F_MINIMUM_LENGTH: usize = 0x44;
const ACB_DISABLED: u16 = 0x0001;
const ACB_AUTO_LOCKED: u16 = 0x0400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamAccountInfo {
    pub rid: u32,
    pub username: String,
    pub disabled: bool,
    pub locked_out: bool,
    pub has_password: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamBypassAction {
    ClearPassword,
    EnableAndClearPassword,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SamEditOutcome {
    pub password_cleared: bool,
    pub account_enabled: bool,
    pub already_passwordless: bool,
}

/// A cleanly unmounted hive has matching primary/secondary sequence numbers.
/// Editing a dirty hive is refused: Windows would replay its transaction log
/// on boot and could clobber or corrupt the edit.
pub fn hive_is_clean(hive: &[u8]) -> bool {
    hive.len() >= 12 && hive[4..8] == hive[8..12]
}

/// Derive the SAM hashed boot key from the SYSTEM and SAM hive bytes. This is
/// the only secret the bypass editor needs; callers must keep it wrapped.
pub fn derive_hbootkey_from_hives(
    system_hive: &[u8],
    sam_hive: &[u8],
) -> Option<Zeroizing<[u8; 32]>> {
    let boot_key = super::sam_structs::extract_boot_key(system_hive)?;
    let reader = RegistryHiveReader::new(sam_hive).ok()?;
    let account = reader.navigate_to(&["SAM", "Domains", "Account"]).ok()??;
    let domain_f = reader.read_raw_value_bytes(&account, "F").ok()??;
    let key = super::hash_decrypt::derive_hashed_boot_key(boot_key, &domain_f)?;
    Some(Zeroizing::new(key))
}

pub fn list_accounts(hive: &[u8]) -> Result<Vec<SamAccountInfo>, String> {
    let reader = RegistryHiveReader::new(hive)?;
    let users_path = ["SAM", "Domains", "Account", "Users"];
    let node = reader
        .navigate_to(&users_path)?
        .ok_or_else(|| "SAM\\Domains\\Account\\Users key is missing".to_string())?;
    let mut accounts = Vec::new();
    for name in reader.read_subkey_names_from_nk(&node)? {
        let Ok(rid) = u32::from_str_radix(&name, 16) else {
            continue;
        };
        let path = ["SAM", "Domains", "Account", "Users", name.as_str()];
        let Some(user_node) = reader.navigate_to(&path)? else {
            continue;
        };
        let v = reader.read_raw_value_bytes(&user_node, "V")?;
        let f = reader.read_raw_value_bytes(&user_node, "F")?;
        let (flags, failed_count) = f.as_deref().and_then(read_f_flags).unwrap_or((0, 0));
        accounts.push(SamAccountInfo {
            rid,
            username: v.as_deref().and_then(username_from_v).unwrap_or_default(),
            disabled: flags & ACB_DISABLED != 0,
            locked_out: flags & ACB_AUTO_LOCKED != 0 || failed_count > 0,
            has_password: v.as_deref().map(nt_hash_present).unwrap_or(false),
        });
    }
    accounts.sort_by_key(|account| account.rid);
    Ok(accounts)
}

/// Rewrites the target account's LM/NT blobs to the canonical empty hashes
/// and optionally clears the disabled/locked flags. `hashed_boot_key` is the
/// derived SAM hbootkey; it stays wrapped in `Zeroizing`.
pub fn apply_bypass(
    hive: &mut [u8],
    rid: u32,
    action: SamBypassAction,
    hashed_boot_key: &Zeroizing<[u8; 32]>,
) -> Result<SamEditOutcome, String> {
    if !hive_is_clean(hive) {
        return Err("SAM hive is dirty (pending transaction log); refusing to edit".to_string());
    }
    let rid_hex = format!("{rid:08X}");
    let path = ["SAM", "Domains", "Account", "Users", rid_hex.as_str()];
    let (v_off, v_len) = locate_value(hive, &path, "V")?;
    let mut outcome = SamEditOutcome::default();

    let fields = {
        let v = hive
            .get(v_off..v_off + v_len)
            .ok_or_else(|| "V value is out of bounds".to_string())?;
        if v_len < USER_V_HEADER_LEN {
            return Err("V value is truncated".to_string());
        }
        if !nt_hash_present(v) {
            outcome.already_passwordless = true;
        }
        [
            (
                read_u32_at(v, V_NT_OFFSET_FIELD)? as usize,
                read_u32_at(v, V_NT_LENGTH_FIELD)? as usize,
                EMPTY_NT_HASH,
                NTPASSWORD_CONSTANT,
            ),
            (
                read_u32_at(v, V_LM_OFFSET_FIELD)? as usize,
                read_u32_at(v, V_LM_LENGTH_FIELD)? as usize,
                EMPTY_LM_HASH,
                LMPASSWORD_CONSTANT,
            ),
        ]
    };
    for (relative, length, empty, constant) in fields {
        if length == 0 {
            continue;
        }
        let blob_off = v_off
            .checked_add(USER_V_HEADER_LEN)
            .and_then(|base| base.checked_add(relative))
            .ok_or_else(|| "hash blob offset overflows".to_string())?;
        if length == 24 {
            // Salt-only blob: the system stores no hash at all here.
            continue;
        }
        let rewritten = {
            let blob = hive
                .get(blob_off..blob_off + length)
                .ok_or_else(|| "hash blob is out of bounds".to_string())?;
            encrypt_hash_into_blob(hashed_boot_key, rid, empty, constant, blob).ok_or_else(
                || {
                    let revision = blob
                        .get(2..4)
                        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
                        .map(u16::from_le_bytes)
                        .unwrap_or(0);
                    format!(
                        "hash blob format is unsupported (length={length}, revision={revision})"
                    )
                },
            )?
        };
        hive[blob_off..blob_off + length].copy_from_slice(&rewritten);
        outcome.password_cleared = true;
    }

    if matches!(action, SamBypassAction::EnableAndClearPassword) {
        let (f_off, f_len) = locate_value(hive, &path, "F")?;
        if f_len < F_MINIMUM_LENGTH {
            return Err("F value is truncated".to_string());
        }
        let flags = read_u16_at(&hive[f_off..f_off + f_len], F_ACB_OFFSET)?;
        let cleared = flags & !(ACB_DISABLED | ACB_AUTO_LOCKED);
        hive[f_off + F_ACB_OFFSET..f_off + F_ACB_OFFSET + 2]
            .copy_from_slice(&cleared.to_le_bytes());
        hive[f_off + F_FAILED_LOGON_OFFSET..f_off + F_FAILED_LOGON_OFFSET + 2]
            .copy_from_slice(&0u16.to_le_bytes());
        outcome.account_enabled = cleared != flags;
    }
    Ok(outcome)
}

fn locate_value(hive: &[u8], path: &[&str], value: &str) -> Result<(usize, usize), String> {
    let reader = RegistryHiveReader::new(hive)?;
    reader
        .value_data_location(path, value)?
        .ok_or_else(|| format!("{} value not found at {}", value, path.join("\\")))
}

fn nt_hash_present(v: &[u8]) -> bool {
    if v.len() < USER_V_HEADER_LEN {
        return false;
    }
    let length = u32::from_le_bytes(
        v[V_NT_LENGTH_FIELD..V_NT_LENGTH_FIELD + 4]
            .try_into()
            .unwrap_or([0; 4]),
    ) as usize;
    // 0 means "no field"; a 24-byte blob is the salt-only shell Windows
    // writes when no hash is stored.
    length > 0 && length != 24
}

/// The V record's string pointers are relative to the 0xCC data mark
/// (chntpw sam.h); `parse_username_from_v_record` in the shared profile
/// parser treats them as absolute, which yields garbage on real hives.
fn username_from_v(v: &[u8]) -> Option<String> {
    let relative = read_u32_at(v, 0x0c).ok()? as usize;
    let length = read_u32_at(v, 0x10).ok()? as usize;
    if length == 0 || length > 512 || !length.is_multiple_of(2) {
        return None;
    }
    let start = USER_V_HEADER_LEN.checked_add(relative)?;
    let bytes = v.get(start..start.checked_add(length)?)?;
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let value = String::from_utf16_lossy(&units);
    let trimmed = value.trim_end_matches('\0');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn read_f_flags(data: &[u8]) -> Option<(u16, u16)> {
    let flags = u16::from_le_bytes(data.get(F_ACB_OFFSET..F_ACB_OFFSET + 2)?.try_into().ok()?);
    let failed = u16::from_le_bytes(
        data.get(F_FAILED_LOGON_OFFSET..F_FAILED_LOGON_OFFSET + 2)?
            .try_into()
            .ok()?,
    );
    Some((flags, failed))
}

fn read_u16_at(data: &[u8], offset: usize) -> Result<u16, String> {
    data.get(offset..offset + 2)
        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| format!("hive field at {offset:#x} is out of bounds"))
}

fn read_u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    data.get(offset..offset + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| format!("hive field at {offset:#x} is out of bounds"))
}

#[cfg(test)]
#[path = "../../tests/unit/registry/sam_edit.rs"]
mod tests;
