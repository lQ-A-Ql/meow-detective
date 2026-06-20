use chrono::{DateTime, TimeZone, Utc};

use crate::registry::txlog::parse_transaction_log;

// ── Submodules ───────────────────────────────────────────────────────────────

pub mod ntuser;
pub(crate) mod reader;
pub mod sam;
pub mod software;
pub mod system;
pub(crate) mod txlog_util;
pub mod types;
pub(crate) mod utf16;

// ── Re-exports from submodules ───────────────────────────────────────────────

pub use ntuser::{extract_ntuser_fields, extract_ntuser_fields_with_txlog};
pub(crate) use reader::RegistryHiveReader;
pub use sam::extract_sam_fields;
pub use software::{extract_software_hive_fields, extract_software_hive_fields_with_txlog};
pub use system::{extract_system_hive_fields, extract_system_hive_fields_with_txlog};
pub use types::*;

// ── Low-level parse utilities ───────────────────────────────────────────────

pub(crate) fn parse_value_data(data_type: u32, data: &[u8]) -> Result<RegistryValue, String> {
    match data_type {
        REG_SZ | REG_EXPAND_SZ => Ok(RegistryValue::String(utf16::decode_utf16_until_nul(data)?)),
        REG_DWORD => Ok(RegistryValue::Dword(
            utf16::read_le_array::<4>(data)
                .map(u32::from_le_bytes)
                .ok_or_else(|| "REG_DWORD value shorter than 4 bytes".to_string())?,
        )),
        REG_QWORD => Ok(RegistryValue::Qword(
            utf16::read_le_array::<8>(data)
                .map(u64::from_le_bytes)
                .ok_or_else(|| "REG_QWORD value shorter than 8 bytes".to_string())?,
        )),
        REG_MULTI_SZ => Ok(RegistryValue::MultiString(
            utf16::decode_utf16_full(data)?
                .split('\0')
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect(),
        )),
        _ => Ok(RegistryValue::Binary(data.to_vec())),
    }
}

// ── Shared field lookup helpers ──────────────────────────────────────────────

pub(crate) fn lookup_string_field(
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

pub(crate) fn lookup_optional_string_field(
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

pub(crate) fn lookup_install_date_field(
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

// ── NTUSER shared helpers ────────────────────────────────────────────────────

/// Apply ROT-13 substitution (UserAssist value-name decoding).
pub(crate) fn rot13_decode(encoded: &str) -> String {
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
pub(crate) fn windows_filetime_to_rfc3339(filetime: u64) -> Option<String> {
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
pub(crate) fn extract_utf16le_from_binary(data: &[u8]) -> Option<String> {
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

/// Convert a Windows FILETIME (100-ns intervals since 1601-01-01) to a
/// `DateTime<Utc>`. Returns `None` for a zero timestamp.
pub(crate) fn filetime_to_utc(filetime: u64) -> Option<DateTime<Utc>> {
    if filetime == 0 {
        return None;
    }
    let unix_seconds = (filetime / 10_000_000).saturating_sub(11_644_473_600);
    let nanos = ((filetime % 10_000_000) * 100) as u32;
    Utc.timestamp_opt(unix_seconds as i64, nanos).single()
}

// ── Tests: shared test fixtures ──────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod test_common {
    use super::*;

    pub(crate) fn empty_hive(root_name: &str) -> Vec<u8> {
        let mut data = vec![0u8; 0x8000];
        data[0..4].copy_from_slice(b"regf");
        data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
        data[0x1000..0x1004].copy_from_slice(b"hbin");
        data[0x1008..0x100c].copy_from_slice(&0x2000u32.to_le_bytes());
        write_nk(&mut data, 0x20, root_name, &[], &[]);
        data
    }

    pub(crate) fn write_nk(
        data: &mut [u8],
        offset: u32,
        name: &str,
        subkeys: &[(&str, u32)],
        values: &[u32],
    ) {
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

    pub(crate) fn write_nk_utf16_name(data: &mut [u8], offset: u32, name: &str) {
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

    pub(crate) fn write_hashed_subkey_list(
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

    pub(crate) fn write_flat_subkey_list(
        data: &mut [u8],
        offset: u32,
        signature: &[u8; 2],
        subkeys: &[u32],
    ) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(signature);
        data[abs + 6..abs + 8].copy_from_slice(&(subkeys.len() as u16).to_le_bytes());
        for (index, child_offset) in subkeys.iter().enumerate() {
            let entry = abs + 8 + index * 4;
            data[entry..entry + 4].copy_from_slice(&child_offset.to_le_bytes());
        }
    }

    pub(crate) fn set_nk_subkey_list(
        data: &mut [u8],
        nk_offset: u32,
        list_offset: u32,
        count: u32,
    ) {
        let abs = BASE_BLOCK_SIZE + nk_offset as usize;
        data[abs + 0x18..abs + 0x1c].copy_from_slice(&count.to_le_bytes());
        data[abs + 0x20..abs + 0x24].copy_from_slice(&list_offset.to_le_bytes());
    }

    pub(crate) fn write_string_value(
        data: &mut [u8],
        offset: u32,
        name: &str,
        value: &str,
        data_offset: u32,
    ) {
        write_typed_string_value(data, offset, name, REG_SZ, value, data_offset);
    }

    pub(crate) fn write_typed_string_value(
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

    pub(crate) fn write_multi_string_value(
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

    pub(crate) fn write_dword_value(data: &mut [u8], offset: u32, name: &str, value: u32) {
        write_vk(data, offset, name, REG_DWORD, 0x8000_0004, value);
    }

    pub(crate) fn write_qword_value(
        data: &mut [u8],
        offset: u32,
        name: &str,
        value: u64,
        data_offset: u32,
    ) {
        let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
        data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[data_abs + 4..data_abs + 12].copy_from_slice(&value.to_le_bytes());
        write_vk(data, offset, name, REG_QWORD, 8, data_offset);
    }

    pub(crate) fn write_vk(
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

    pub(crate) fn write_binary_value(
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

    pub(crate) fn make_recent_doc_binary(file_name: &str) -> Vec<u8> {
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

    pub(crate) fn make_user_assist_binary(
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

    pub(crate) fn make_mru_list_ex(indices: &[u32]) -> Vec<u8> {
        let mut data = Vec::with_capacity((indices.len() + 1) * 4);
        for idx in indices {
            data.extend_from_slice(&idx.to_le_bytes());
        }
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        data
    }

    pub(crate) fn make_sam_v_record(
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
    pub(crate) fn make_sid(sub_authorities: &[u32]) -> Vec<u8> {
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

    pub(crate) fn make_sam_c_value(member_sids: &[Vec<u8>]) -> Vec<u8> {
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
    pub(crate) fn make_domain_account_f_blob(
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

    /// Helper: encode a string as UTF-16LE bytes (null-terminated).
    pub(crate) fn encode_utf16le(s: &str) -> Vec<u8> {
        let mut out: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        out.extend_from_slice(&[0x00, 0x00]); // null terminator
        out
    }

    /// Build a synthetic SAM hive with 2 users (Administrator, Guest) and
    /// groups from both Builtin\Aliases and Account\Aliases.
    ///
    /// Offset layout (0x80 apart to avoid NK record overlap):
    ///   NK keys:  0x020–0xA00
    ///   VK values: 0x1100–0x123F
    ///   Binary data cells: 0x5000–0x53FF
    pub(crate) fn synthetic_sam_hive() -> Vec<u8> {
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
        let guest_v = make_sam_v_record(
            0,
            133_400_000_000_000_000,
            501,
            super::SAM_ACCOUNT_DISABLED,
            0,
        );
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
}

// ── General RegistryHiveReader tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use test_common::*;

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
}
