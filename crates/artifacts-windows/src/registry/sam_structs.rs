use binread::BinRead;

use crate::registry::lookup::RegistryHiveReader;

/// Boot key permutation table.
///
/// After concatenating the four LSA subkey class names (hex-decoded bytes),
/// apply this permutation to produce the final 16-byte System Key (SysKey /
/// BootKey). Indices are into the concatenated 16-byte array.
const BOOT_KEY_PERMUTATION: [usize; 16] = [8, 5, 4, 2, 11, 9, 13, 3, 0, 6, 1, 12, 14, 10, 15, 7];

/// Extract the SysKey (BootKey) from a SYSTEM registry hive.
///
/// The boot key is obfuscated across four class names stored in the NK
/// cells of the LSA subkeys (`JD`, `Skew1`, `GBG`, `Data`) under
/// `ControlSetXXX\Control\LSA`. Each class name is a UTF-16LE hex string.
/// Concatenating and hex-decoding the four strings yields the scrambled
/// 16-byte key; applying [`BOOT_KEY_PERMUTATION`] produces the final boot
/// key used to decrypt the SAM hive.
///
/// The function tries each discovered ControlSet (determined via
/// `Select\Current` plus fallback names) and returns the first
/// successfully extracted boot key.
pub fn extract_boot_key(system_hive: &[u8]) -> Option<[u8; 16]> {
    let hive = RegistryHiveReader::new(system_hive).ok()?;
    let control_sets = hive.control_set_candidates(&mut Vec::new());

    /// Hex-decode an ASCII/UTF-16LE hex string of the form `hex,hex,...`
    /// (the class names of the JD / Skew1 / GBG / Data subkeys use
    /// comma-separated hex bytes in some Windows versions) into a `Vec<u8>`.
    /// Falls back to a straight hex-decode when the string contains no
    /// commas.
    fn decode_hex_class_name(raw: &str) -> Option<Vec<u8>> {
        // Some Windows builds store the class name as comma-separated hex
        // bytes, e.g. "46,00,3c,db,...".  Remove commas and whitespace first.
        let cleaned: String = raw
            .chars()
            .filter(|c| !c.is_ascii_whitespace() && *c != ',')
            .collect();
        if !cleaned.len().is_multiple_of(2) {
            return None;
        }
        let mut bytes = Vec::with_capacity(cleaned.len() / 2);
        for chunk in cleaned.as_bytes().chunks(2) {
            let high = hex_char_to_nibble(chunk[0])?;
            let low = hex_char_to_nibble(*chunk.get(1)?)?;
            bytes.push((high << 4) | low);
        }
        Some(bytes)
    }

    fn hex_char_to_nibble(c: u8) -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    }

    for cs in control_sets {
        let lsa_path: &[&str] = &[cs.as_str(), "Control", "LSA"];
        let lsa_nk = hive.navigate_to(lsa_path).ok()??;
        let subkey_names = hive.read_subkey_names_from_nk(&lsa_nk).ok()?;

        let ordered_subkeys = ["JD", "Skew1", "GBG", "Data"];
        let mut hex_combined = String::new();

        for sk_name in ordered_subkeys {
            if !subkey_names.iter().any(|n| n.eq_ignore_ascii_case(sk_name)) {
                hex_combined.clear();
                break;
            }
            let mut sk_path: Vec<&str> = lsa_path.to_vec();
            sk_path.push(sk_name);
            let class_name = hive.read_class_name_at(&sk_path).ok()??;
            hex_combined.push_str(&class_name);
        }

        if hex_combined.is_empty() {
            continue;
        }

        let scrambled = decode_hex_class_name(&hex_combined)?;
        if scrambled.len() < 16 {
            continue;
        }

        let mut boot_key = [0u8; 16];
        for (i, &p) in BOOT_KEY_PERMUTATION.iter().enumerate() {
            boot_key[i] = *scrambled.get(p)?;
        }
        return Some(boot_key);
    }

    None
}

/// Raw SAM UserF binary structure (F value under Users\<RID>).
///
/// This is the binary blob stored as the `F` registry value under each
/// `SAM\Domains\Account\Users\<RID_HEX>` subkey.  It contains the user's
/// login timestamps, RID, account flags, and login counts.
///
/// Offsets are relative to the start of the `F` value data.
#[derive(BinRead, Debug)]
#[br(little)]
pub struct UserFRaw {
    /// Last login time as Windows FILETIME (100 ns since 1601-01-01).
    pub last_login_time: u64,
    _pad1: u64,
    /// Last password change time as Windows FILETIME.
    pub last_pwd_change_time: u64,
    _pad2: u64,
    /// Last failed login time as Windows FILETIME.
    pub last_failed_login_time: u64,
    /// Relative Identifier (RID) of the user account.
    pub rid: u32,
    _pad3: u32,
    /// User account attribute flags (e.g. user vs. machine account).
    pub user_attribute: u32,
    _pad4: u32,
    /// Number of successful logons.
    pub logon_count: u16,
    /// Number of failed logon attempts.
    pub invalid_login_count: u16,
    _pad5: [u8; 12],
}

/// Parse a SAM UserF binary blob and extract the RID, logon count, and user
/// attribute flags.
///
/// Returns `None` if the data is shorter than the expected struct size or
/// if the binary parse fails for any other reason.
pub fn parse_user_f(data: &[u8]) -> Option<(u32, u16, u32)> {
    let mut cursor = std::io::Cursor::new(data);
    let user_f = UserFRaw::read(&mut cursor).ok()?;
    Some((user_f.rid, user_f.logon_count, user_f.user_attribute))
}

/// Extract the username from a SAM V record binary blob.
///
/// The V record stores the username as a UTF-16LE string at a
/// variable offset recorded in the record header:
///   - offset 0x0C: username_offset (u32, relative to record start)
///   - offset 0x10: username_length (u32, in bytes)
///
/// Returns `None` if the data is too short or the offsets/lengths
/// are invalid.
pub fn parse_username_from_v_record(data: &[u8]) -> Option<String> {
    if data.len() < 0x14 {
        return None;
    }
    let username_offset = u32::from_le_bytes(data.get(0x0C..0x10)?.try_into().ok()?) as usize;
    let username_length = u32::from_le_bytes(data.get(0x10..0x14)?.try_into().ok()?) as usize;

    if username_length == 0 || username_length > 256 {
        return None;
    }
    let name_bytes = data.get(username_offset..username_offset + username_length)?;
    if !name_bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = name_bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&units);
    let trimmed = s.trim_end_matches('\0');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

/// Raw SAM UserV binary structure (V value under Users\<RID>).
///
/// This structure maps the string-offset portion of the V record.  Each
/// string field is stored as a UTF-16LE blob at a variable offset from the
/// start of the record, with a length in bytes.
#[derive(BinRead, Debug)]
#[br(little)]
pub struct UserVRaw {
    _pad1: [u8; 12],
    name_offset: u32,
    name_length: u32,
    _pad2: u32,
    full_name_offset: u32,
    full_name_length: u32,
    _pad3: u32,
    comment_offset: u32,
    comment_length: u32,
    _pad4: u32,
    home_dir_offset: u32,
    home_dir_length: u32,
    _pad5: u32,
    profile_path_offset: u32,
    profile_path_length: u32,
    _pad6: u32,
    script_path_offset: u32,
    script_path_length: u32,
}

/// String fields extracted from a SAM UserV record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SamUserProfile {
    pub username: String,
    pub full_name: String,
    pub comment: String,
    pub home_dir: String,
    pub profile_path: String,
    pub script_path: String,
}

/// Extract a UTF-16LE string from binary data at a given byte offset and
/// length.  Returns `None` when the offset/length are out of bounds, the
/// length is zero, the length is implausibly large (>512), or the decoded
/// string is empty after trimming trailing NULs.
fn extract_utf16le_at(data: &[u8], offset: u32, length: u32) -> Option<String> {
    if length == 0 || length > 512 {
        return None;
    }
    let offset = offset as usize;
    let length = length as usize;
    let bytes = data.get(offset..offset.wrapping_add(length))?;
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&units);
    let trimmed = s.trim_end_matches('\0');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse a SAM UserV binary blob and extract all string fields.
///
/// Returns `None` if the data is shorter than the expected struct size or
/// if the binary parse fails for any other reason.
pub fn parse_user_v(data: &[u8]) -> Option<SamUserProfile> {
    let mut cursor = std::io::Cursor::new(data);
    let raw = UserVRaw::read(&mut cursor).ok()?;

    let username = extract_utf16le_at(data, raw.name_offset, raw.name_length).unwrap_or_default();
    let full_name =
        extract_utf16le_at(data, raw.full_name_offset, raw.full_name_length).unwrap_or_default();
    let comment =
        extract_utf16le_at(data, raw.comment_offset, raw.comment_length).unwrap_or_default();
    let home_dir =
        extract_utf16le_at(data, raw.home_dir_offset, raw.home_dir_length).unwrap_or_default();
    let profile_path = extract_utf16le_at(data, raw.profile_path_offset, raw.profile_path_length)
        .unwrap_or_default();
    let script_path = extract_utf16le_at(data, raw.script_path_offset, raw.script_path_length)
        .unwrap_or_default();

    Some(SamUserProfile {
        username,
        full_name,
        comment,
        home_dir,
        profile_path,
        script_path,
    })
}

/// Raw SAM DomainAccountF binary structure (F value under `SAM\Domains\Account`).
///
/// This binary blob stores the domain-wide account policy including
/// password age limits, password complexity/history requirements, and
/// lockout thresholds.  It is stored as the `F` registry value directly
/// under `SAM\Domains\Account` (not under a user subkey — user-level `F`
/// values are [`UserFRaw`]).
///
/// Offsets are relative to the start of the `F` value data.
#[derive(BinRead, Debug)]
#[br(little)]
pub struct DomainAccountFRaw {
    /// SAM revision.
    pub revision: u32,
    _pad1: u32,
    /// Account creation time as Windows FILETIME (100 ns since 1601-01-01).
    pub creation_time: u64,
    /// Domain-modified count (incremented on policy changes).
    pub domain_modified_count: u64,
    /// Maximum password age in 100 ns units.
    pub max_pwd_age: u64,
    /// Minimum password age in 100 ns units.
    pub min_pwd_age: u64,
    /// Force logoff interval in 100 ns units.
    pub force_logoff: u64,
    /// Account lockout duration in 100 ns units.
    pub lockout_duration: u64,
    /// Lockout observation window in 100 ns units.
    pub lockout_observation_window: u64,
    _pad2: u64,
    /// Next available Relative Identifier (RID) for user accounts.
    pub next_rid: u32,
    /// Password properties bitmask (e.g., password complexity, reversible encryption).
    pub pwd_properties: u32,
    /// Minimum password length in characters.
    pub min_pwd_length: u16,
    /// Password history length (number of passwords remembered).
    pub pwd_history_length: u16,
    /// Lockout threshold (number of invalid attempts before lockout).
    pub lockout_threshold: u16,
    _pad3: u16,
    /// Server state (domain controller operational state).
    pub server_state: u32,
    /// Server role (primary/backup/standalone).
    pub server_role: u16,
    /// UAS compatibility requirement.
    pub uas_compatibility_req: u16,
}

/// Password policy extracted from DomainAccountF.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SamPasswordPolicy {
    /// Maximum password age in days (0 = never expires).
    pub max_password_age_days: u64,
    /// Minimum password age in days (0 = can change immediately).
    pub min_password_age_days: u64,
    /// Minimum password length in characters.
    pub min_password_length: u16,
    /// Number of passwords remembered in history.
    pub password_history_length: u16,
    /// Number of invalid attempts before account lockout (0 = never lock).
    pub lockout_threshold: u16,
    /// Account lockout duration in minutes (0 = locked until administrator resets).
    pub lockout_duration_minutes: u64,
    /// Lockout observation window in minutes.
    pub lockout_observation_window_minutes: u64,
}

/// 100 ns intervals per day, used to convert FILETIME-relative durations
/// back to calendar days.
const HUNDRED_NS_PER_DAY: u64 = 864_000_000_000;

/// Convert a 64-bit FILETIME-relative interval (100 ns units) to whole
/// days, rounding to the nearest day.  Returns 0 when the input is 0
/// (meaning "never" / unlimited).
fn filetime_ticks_to_days(ticks: u64) -> u64 {
    if ticks == 0 {
        return 0;
    }
    ticks / HUNDRED_NS_PER_DAY
}

/// Convert a 64-bit FILETIME-relative interval (100 ns units) to whole
/// minutes, rounding down.  Returns 0 when the input is 0.
fn filetime_ticks_to_minutes(ticks: u64) -> u64 {
    if ticks == 0 {
        return 0;
    }
    ticks / (10_000_000 * 60) // 100 ns → 1 s (/10M) → 1 min (/60)
}

/// Parse a SAM DomainAccountF binary blob.
///
/// Returns `None` if the data is shorter than the expected struct size or
/// if the binary parse fails for any other reason.
pub fn parse_domain_account_f(data: &[u8]) -> Option<SamPasswordPolicy> {
    let mut cursor = std::io::Cursor::new(data);
    let raw = DomainAccountFRaw::read(&mut cursor).ok()?;
    Some(SamPasswordPolicy {
        max_password_age_days: filetime_ticks_to_days(raw.max_pwd_age),
        min_password_age_days: filetime_ticks_to_days(raw.min_pwd_age),
        min_password_length: raw.min_pwd_length,
        password_history_length: raw.pwd_history_length,
        lockout_threshold: raw.lockout_threshold,
        lockout_duration_minutes: filetime_ticks_to_minutes(raw.lockout_duration),
        lockout_observation_window_minutes: filetime_ticks_to_minutes(
            raw.lockout_observation_window,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_f(rid: u32, logon_count: u16, user_attribute: u32) -> Vec<u8> {
        let mut data = vec![0u8; 80];
        // last_login_time at 0x00: leave as 0
        // _pad1 at 0x08: leave as 0
        // last_pwd_change_time at 0x10: leave as 0
        // _pad2 at 0x18: leave as 0
        // last_failed_login_time at 0x20: leave as 0
        data[0x28..0x2C].copy_from_slice(&rid.to_le_bytes());
        // _pad3 at 0x2C: leave as 0
        data[0x30..0x34].copy_from_slice(&user_attribute.to_le_bytes());
        // _pad4 at 0x34: leave as 0
        data[0x38..0x3A].copy_from_slice(&logon_count.to_le_bytes());
        // invalid_login_count at 0x3A: leave as 0
        data
    }

    #[test]
    fn parse_user_f_valid() {
        let f_data = make_user_f(500, 42, 0x0300);
        let result = parse_user_f(&f_data);
        assert!(result.is_some());
        let (rid, logon_count, user_attribute) = result.unwrap();
        assert_eq!(rid, 500);
        assert_eq!(logon_count, 42);
        assert_eq!(user_attribute, 0x0300);
    }

    #[test]
    fn parse_user_f_too_short() {
        let data = vec![0u8; 20];
        assert!(parse_user_f(&data).is_none());
    }

    #[test]
    fn parse_user_f_roundtrip() {
        let f_data = make_user_f(1001, 15, 0);
        let (rid, logon_count, user_attribute) = parse_user_f(&f_data).unwrap();
        assert_eq!(rid, 1001);
        assert_eq!(logon_count, 15);
        assert_eq!(user_attribute, 0);
    }

    #[test]
    fn parse_username_from_v_record_valid() {
        // Build a V record with username at offset 0x50
        let username_str = "Administrator";
        let username_utf16: Vec<u8> = username_str
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let username_offset = 0x50u32;
        let username_length = username_utf16.len() as u32;

        let mut data = vec![0u8; 0x200];
        // Write the username at the offset
        data[username_offset as usize..username_offset as usize + username_utf16.len()]
            .copy_from_slice(&username_utf16);
        // Write the offset/length fields
        data[0x0C..0x10].copy_from_slice(&username_offset.to_le_bytes());
        data[0x10..0x14].copy_from_slice(&username_length.to_le_bytes());

        let result = parse_username_from_v_record(&data);
        assert_eq!(result.as_deref(), Some("Administrator"));
    }

    #[test]
    fn parse_username_from_v_record_too_short() {
        let data = vec![0u8; 4];
        assert!(parse_username_from_v_record(&data).is_none());
    }

    #[test]
    fn parse_username_from_v_record_zero_length() {
        let mut data = vec![0u8; 0x20];
        data[0x10..0x14].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_username_from_v_record(&data).is_none());
    }

    // ── Boot key extraction test helpers ───────────────────────────────────

    const BASE_BLOCK_SIZE: usize = 0x1000;
    const INVALID_OFFSET: u32 = 0xFFFF_FFFF;
    const HBIN_MAGIC: &[u8; 4] = b"hbin";

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn boot_key_empty_hive(root_name: &str) -> Vec<u8> {
        let mut data = vec![0u8; 0x10000];
        data[0..4].copy_from_slice(b"regf");
        data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
        data[0x1000..0x1004].copy_from_slice(HBIN_MAGIC);
        data[0x1008..0x100c].copy_from_slice(&0xF000u32.to_le_bytes());
        boot_key_write_nk(&mut data, 0x20, root_name, &[], &[]);
        data
    }

    /// Write an NK cell at `offset` (hive-relative).  `subkeys` is a list of
    /// `(name, offset)` tuples; `values` is a list of VK offsets.
    fn boot_key_write_nk(
        data: &mut [u8],
        offset: u32,
        name: &str,
        subkeys: &[(&str, u32)],
        values: &[u32],
    ) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        let name_bytes = name.as_bytes();
        let cell_size = -(256i32);
        data[abs..abs + 4].copy_from_slice(&cell_size.to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"nk");
        // flags = 0x20 → ASCII (compressed) name
        write_u16(data, abs + 6, 0x20);

        // Timestamp, parent – leave as zero
        // num_subkeys at NK+0x14 → abs+0x18
        write_u32(data, abs + 0x18, subkeys.len() as u32);
        // subkeys_list_offset at NK+0x1C → abs+0x20
        let subkey_list_offset = 0x3000 + offset;
        write_u32(
            data,
            abs + 0x20,
            if subkeys.is_empty() {
                INVALID_OFFSET
            } else {
                subkey_list_offset
            },
        );
        // num_values at NK+0x24 → abs+0x28
        write_u32(data, abs + 0x28, values.len() as u32);
        // values_list_offset at NK+0x28 → abs+0x2C
        let value_list_offset = 0x4000 + offset;
        write_u32(
            data,
            abs + 0x2C,
            if values.is_empty() {
                INVALID_OFFSET
            } else {
                value_list_offset
            },
        );
        // classname_offset at NK+0x30 → abs+0x34 (inline: 0xFFFFFFFF)
        write_u32(data, abs + 0x34, INVALID_OFFSET);
        // class_name_length at NK+0x4A → abs+0x4E (0, overridden later)
        write_u16(data, abs + 0x4E, 0);
        // name_length at NK+0x48 → abs+0x4C
        write_u16(data, abs + 0x4C, name_bytes.len() as u16);
        // name at NK+0x4C → abs+0x50
        data[abs + 0x50..abs + 0x50 + name_bytes.len()].copy_from_slice(name_bytes);

        if !values.is_empty() {
            let list_abs = BASE_BLOCK_SIZE + value_list_offset as usize;
            let list_cell_size = -(((values.len() as i32) * 4) + 4);
            data[list_abs..list_abs + 4].copy_from_slice(&list_cell_size.to_le_bytes());
            for (idx, vk_offset) in values.iter().enumerate() {
                write_u32(data, list_abs + 4 + idx * 4, *vk_offset);
            }
        }

        if !subkeys.is_empty() {
            let list_abs = BASE_BLOCK_SIZE + subkey_list_offset as usize;
            // Use "lf" hashed list
            let list_cell_size = -(((subkeys.len() as i32) * 8) + 4);
            data[list_abs..list_abs + 4].copy_from_slice(&list_cell_size.to_le_bytes());
            data[list_abs + 4..list_abs + 6].copy_from_slice(b"lf");
            write_u16(data, list_abs + 6, subkeys.len() as u16);
            for (idx, (sk_name, sk_offset)) in subkeys.iter().enumerate() {
                let entry = list_abs + 8 + idx * 8;
                // First 4 bytes: name hash (first 4 chars of name)
                let mut hash = [0u8; 4];
                for (i, b) in sk_name.as_bytes().iter().take(4).enumerate() {
                    hash[i] = *b;
                }
                data[entry..entry + 4].copy_from_slice(&hash);
                // Next 4 bytes: child offset
                write_u32(data, entry + 4, *sk_offset);
            }
        }
    }

    /// Write a class name into an existing NK cell.  The class name is stored
    /// inline right after the key name.
    fn boot_key_set_class_name(data: &mut [u8], nk_offset: u32, class_name: &str) {
        let abs = BASE_BLOCK_SIZE + nk_offset as usize;
        let name_len =
            u16::from_le_bytes(data[abs + 0x4C..abs + 0x4E].try_into().unwrap()) as usize;
        let class_utf16: Vec<u8> = class_name
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let class_start = abs + 0x50 + name_len;
        data[class_start..class_start + class_utf16.len()].copy_from_slice(&class_utf16);
        // Set class_name_length at NK+0x4A → abs+0x4E
        write_u16(data, abs + 0x4E, class_utf16.len() as u16);
    }

    /// Write an inline DWORD value VK cell.
    fn boot_key_write_dword(data: &mut [u8], offset: u32, name: &str, value: u32) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        let name_bytes = name.as_bytes();
        data[abs..abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"vk");
        write_u16(data, abs + 6, name_bytes.len() as u16);
        // data_len with inline flag
        write_u32(data, abs + 8, 0x8000_0004);
        // data_offset = inline value
        write_u32(data, abs + 12, value);
        // data_type = REG_DWORD (4)
        write_u32(data, abs + 16, 4);
        // flags = 1 (ASCII name)
        write_u16(data, abs + 20, 1);
        // name
        data[abs + 0x18..abs + 0x18 + name_bytes.len()].copy_from_slice(name_bytes);
    }

    // ── Boot key extraction tests ──────────────────────────────────────────

    #[test]
    fn extract_boot_key_from_synthetic_hive() {
        let mut data = boot_key_empty_hive("SYSTEM");

        // Build key tree:
        // ROOT → Select, ControlSet001
        // Select → Current = 1
        // ControlSet001 → Control
        // Control → LSA
        // LSA → JD, Skew1, GBG, Data
        boot_key_write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x200, "Select", &[], &[0x2000]);
        boot_key_write_dword(&mut data, 0x2000, "Current", 1);
        boot_key_write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x400, "Control", &[("LSA", 0x500)], &[]);
        boot_key_write_nk(
            &mut data,
            0x500,
            "LSA",
            &[
                ("JD", 0x600),
                ("Skew1", 0x680),
                ("GBG", 0x700),
                ("Data", 0x780),
            ],
            &[],
        );

        // Write each LSA subkey with a known class name.
        // Each class name is 4 hex bytes (8 hex chars).
        // Concatenated: 01020304 05060708 090a0b0c 0d0e0f10
        // After permutation [8,5,4,2,11,9,13,3,0,6,1,12,14,10,15,7]:
        //   090605030c0a0e040107020d0f0b1008
        boot_key_write_nk(&mut data, 0x600, "JD", &[], &[]);
        boot_key_set_class_name(&mut data, 0x600, "01020304");

        boot_key_write_nk(&mut data, 0x680, "Skew1", &[], &[]);
        boot_key_set_class_name(&mut data, 0x680, "05060708");

        boot_key_write_nk(&mut data, 0x700, "GBG", &[], &[]);
        boot_key_set_class_name(&mut data, 0x700, "090a0b0c");

        boot_key_write_nk(&mut data, 0x780, "Data", &[], &[]);
        boot_key_set_class_name(&mut data, 0x780, "0d0e0f10");

        let result = extract_boot_key(&data);
        assert!(result.is_some(), "boot key extraction should succeed");
        let boot_key = result.unwrap();
        let expected: [u8; 16] = [
            0x09, 0x06, 0x05, 0x03, 0x0c, 0x0a, 0x0e, 0x04, 0x01, 0x07, 0x02, 0x0d, 0x0f, 0x0b,
            0x10, 0x08,
        ];
        assert_eq!(boot_key, expected, "boot key should match permuted result");
    }

    #[test]
    fn extract_boot_key_select_current_dword() {
        // Verify that Select\Current dword actually controls which
        // ControlSet is used.
        let mut data = boot_key_empty_hive("SYSTEM");

        // Put LSA under ControlSet002 (NOT ControlSet001)
        boot_key_write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet002", 0x300)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x200, "Select", &[], &[0x2000]);
        boot_key_write_dword(&mut data, 0x2000, "Current", 2);
        boot_key_write_nk(
            &mut data,
            0x300,
            "ControlSet002",
            &[("Control", 0x400)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x400, "Control", &[("LSA", 0x500)], &[]);
        boot_key_write_nk(
            &mut data,
            0x500,
            "LSA",
            &[
                ("JD", 0x600),
                ("Skew1", 0x680),
                ("GBG", 0x700),
                ("Data", 0x780),
            ],
            &[],
        );

        // Each subkey class is "aa" → single byte 0xAA for simplicity.
        // Four subkeys × "aa" = "aaaaaaaa" → [0xAA, 0xAA, 0xAA, 0xAA,
        // 0xAA, 0xAA, 0xAA, 0xAA].  With comma-separated hex, these
        // could be "aa,aa,aa,aa" per subkey → each subkey decodes to
        // 4 bytes of 0xAA.
        for &(nk_off, _name) in &[
            (0x600, "JD"),
            (0x680, "Skew1"),
            (0x700, "GBG"),
            (0x780, "Data"),
        ] {
            boot_key_write_nk(&mut data, nk_off, _name, &[], &[]);
            // Use comma-separated form ("aa,aa,aa,aa" → 8 hex digits after commas
            // stripped = "aaaaaaaa" → 4 bytes of 0xAA)
            boot_key_set_class_name(&mut data, nk_off, "aa,aa,aa,aa");
        }

        let result = extract_boot_key(&data);
        assert!(result.is_some(), "boot key extraction should succeed");
        let boot_key = result.unwrap();
        // All bytes are 0xAA → after permutation they're all still 0xAA
        assert_eq!(boot_key, [0xAAu8; 16]);
    }

    #[test]
    fn extract_boot_key_fallback_when_select_corrupt() {
        // When Select\Current is unreadable, control_set_candidates
        // falls back to ControlSet001 and ControlSet002.  Verify
        // fallback works.
        let mut data = boot_key_empty_hive("SYSTEM");

        boot_key_write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        // Corrupt Select: no values at all (Current missing)
        boot_key_write_nk(&mut data, 0x200, "Select", &[], &[]);
        boot_key_write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x400, "Control", &[("LSA", 0x500)], &[]);
        boot_key_write_nk(
            &mut data,
            0x500,
            "LSA",
            &[
                ("JD", 0x600),
                ("Skew1", 0x680),
                ("GBG", 0x700),
                ("Data", 0x780),
            ],
            &[],
        );

        for &(nk_off, _name) in &[
            (0x600, "JD"),
            (0x680, "Skew1"),
            (0x700, "GBG"),
            (0x780, "Data"),
        ] {
            boot_key_write_nk(&mut data, nk_off, _name, &[], &[]);
            boot_key_set_class_name(&mut data, nk_off, "bb,bb,bb,bb");
        }

        let result = extract_boot_key(&data);
        assert!(result.is_some(), "fallback should find ControlSet001");
        assert_eq!(result.unwrap(), [0xBBu8; 16]);
    }

    #[test]
    fn extract_boot_key_empty_hive_returns_none() {
        let data = boot_key_empty_hive("SYSTEM");
        assert!(extract_boot_key(&data).is_none());
    }

    #[test]
    fn extract_boot_key_missing_lsa_returns_none() {
        let mut data = boot_key_empty_hive("SYSTEM");
        boot_key_write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x200, "Select", &[], &[0x2000]);
        boot_key_write_dword(&mut data, 0x2000, "Current", 1);
        // ControlSet001 exists but has no Control\LSA
        boot_key_write_nk(&mut data, 0x300, "ControlSet001", &[], &[]);

        assert!(extract_boot_key(&data).is_none());
    }

    #[test]
    fn extract_boot_key_missing_subkey_returns_none() {
        let mut data = boot_key_empty_hive("SYSTEM");
        boot_key_write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x200, "Select", &[], &[0x2000]);
        boot_key_write_dword(&mut data, 0x2000, "Current", 1);
        boot_key_write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x400, "Control", &[("LSA", 0x500)], &[]);
        // Only 3 of 4 required subkeys
        boot_key_write_nk(
            &mut data,
            0x500,
            "LSA",
            &[("JD", 0x600), ("Skew1", 0x680), ("GBG", 0x700)],
            &[],
        );
        for &(nk_off, _name) in &[(0x600, "JD"), (0x680, "Skew1"), (0x700, "GBG")] {
            boot_key_write_nk(&mut data, nk_off, _name, &[], &[]);
            boot_key_set_class_name(&mut data, nk_off, "cc,cc,cc,cc");
        }

        assert!(extract_boot_key(&data).is_none());
    }

    #[test]
    fn extract_boot_key_invalid_hex_class_returns_none() {
        let mut data = boot_key_empty_hive("SYSTEM");
        boot_key_write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x200, "Select", &[], &[0x2000]);
        boot_key_write_dword(&mut data, 0x2000, "Current", 1);
        boot_key_write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        boot_key_write_nk(&mut data, 0x400, "Control", &[("LSA", 0x500)], &[]);
        boot_key_write_nk(
            &mut data,
            0x500,
            "LSA",
            &[
                ("JD", 0x600),
                ("Skew1", 0x680),
                ("GBG", 0x700),
                ("Data", 0x780),
            ],
            &[],
        );

        // JD has a valid class, but Skew1 has non-hex characters
        boot_key_write_nk(&mut data, 0x600, "JD", &[], &[]);
        boot_key_set_class_name(&mut data, 0x600, "deadbeef");

        boot_key_write_nk(&mut data, 0x680, "Skew1", &[], &[]);
        boot_key_set_class_name(&mut data, 0x680, "not-hex!");

        boot_key_write_nk(&mut data, 0x700, "GBG", &[], &[]);
        boot_key_set_class_name(&mut data, 0x700, "cafebabe");

        boot_key_write_nk(&mut data, 0x780, "Data", &[], &[]);
        boot_key_set_class_name(&mut data, 0x780, "dec0ded");

        assert!(extract_boot_key(&data).is_none());
    }

    // ── UserV record extraction tests ─────────────────────────────────────

    /// Build a synthetic UserV blob with string fields at known offsets.
    fn make_user_v_blob(entries: &[(&str, u32, u32)]) -> Vec<u8> {
        // entries: (text, offset, length)
        // length is the UTF-16LE byte length (without NUL terminator)
        let mut data = vec![0u8; 0x200];
        for (text, offset, length) in entries {
            // If length is 0, skip writing (zero-length / empty field)
            if *length == 0 {
                continue;
            }
            let utf16_bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let copy_len = (*length as usize).min(utf16_bytes.len());
            data[*offset as usize..*offset as usize + copy_len]
                .copy_from_slice(&utf16_bytes[..copy_len]);
        }
        data
    }

    /// Write `name_offset`, `name_length`, etc. into the UserVRaw header area.
    #[allow(clippy::too_many_arguments)]
    fn write_user_v_header(
        data: &mut [u8],
        name_offset: u32,
        name_length: u32,
        full_name_offset: u32,
        full_name_length: u32,
        comment_offset: u32,
        comment_length: u32,
        home_dir_offset: u32,
        home_dir_length: u32,
        profile_path_offset: u32,
        profile_path_length: u32,
        script_path_offset: u32,
        script_path_length: u32,
    ) {
        let u32_at = |d: &mut [u8], off: usize, v: u32| {
            d[off..off + 4].copy_from_slice(&v.to_le_bytes());
        };
        // name_offset at 0x0C
        u32_at(data, 0x0C, name_offset);
        // name_length at 0x10
        u32_at(data, 0x10, name_length);
        // full_name_offset at 0x18
        u32_at(data, 0x18, full_name_offset);
        // full_name_length at 0x1C
        u32_at(data, 0x1C, full_name_length);
        // comment_offset at 0x24
        u32_at(data, 0x24, comment_offset);
        // comment_length at 0x28
        u32_at(data, 0x28, comment_length);
        // home_dir_offset at 0x30
        u32_at(data, 0x30, home_dir_offset);
        // home_dir_length at 0x34
        u32_at(data, 0x34, home_dir_length);
        // profile_path_offset at 0x3C
        u32_at(data, 0x3C, profile_path_offset);
        // profile_path_length at 0x40
        u32_at(data, 0x40, profile_path_length);
        // script_path_offset at 0x48
        u32_at(data, 0x48, script_path_offset);
        // script_path_length at 0x4C
        u32_at(data, 0x4C, script_path_length);
    }

    fn utf16le_byte_len(s: &str) -> u32 {
        (s.encode_utf16().count() * 2) as u32
    }

    #[test]
    fn parse_user_v_all_fields_present() {
        let username = "Administrator";
        let _full_name = String::new(); // empty
        let comment = "Built-in account for administering the computer/domain";
        let _home_dir = String::new(); // empty
        let _profile_path = String::new(); // empty
        let _script_path = String::new(); // empty

        let name_off = 0x50u32;
        let full_name_off = 0x80u32;
        let comment_off = 0xB0u32;
        let home_dir_off = 0x120u32;
        let profile_path_off = 0x150u32;
        let script_path_off = 0x180u32;

        let name_len = utf16le_byte_len(username);
        let comment_len = utf16le_byte_len(comment);
        // All zero-length fields → offset and length both zero
        let full_name_len = 0u32;
        let home_dir_len = 0u32;
        let profile_path_len = 0u32;
        let script_path_len = 0u32;

        let mut data = make_user_v_blob(&[
            (username, name_off, name_len),
            (comment, comment_off, comment_len),
        ]);
        write_user_v_header(
            &mut data,
            name_off,
            name_len,
            full_name_off,
            full_name_len,
            comment_off,
            comment_len,
            home_dir_off,
            home_dir_len,
            profile_path_off,
            profile_path_len,
            script_path_off,
            script_path_len,
        );

        let profile = parse_user_v(&data);
        assert!(profile.is_some(), "should parse UserV blob");
        let p = profile.unwrap();
        assert_eq!(p.username, "Administrator");
        assert_eq!(p.full_name, "");
        assert_eq!(p.comment, comment);
        assert_eq!(p.home_dir, "");
        assert_eq!(p.profile_path, "");
        assert_eq!(p.script_path, "");
    }

    #[test]
    fn parse_user_v_with_full_name_and_profile() {
        let username = "jdoe";
        let full_name = "John Doe";
        let comment = "Engineering";
        let home_dir = "C:\\Users\\jdoe";
        let profile_path = "C:\\Users\\jdoe";
        let script_path = "logon.cmd";

        let name_off = 0x50u32;
        let full_name_off = 0x70u32;
        let comment_off = 0x90u32;
        let home_dir_off = 0xB0u32;
        let profile_path_off = 0xD0u32;
        let script_path_off = 0xF0u32;

        let entries = &[
            (username, name_off, utf16le_byte_len(username)),
            (full_name, full_name_off, utf16le_byte_len(full_name)),
            (comment, comment_off, utf16le_byte_len(comment)),
            (home_dir, home_dir_off, utf16le_byte_len(home_dir)),
            (
                profile_path,
                profile_path_off,
                utf16le_byte_len(profile_path),
            ),
            (script_path, script_path_off, utf16le_byte_len(script_path)),
        ];

        let mut data = make_user_v_blob(entries);
        write_user_v_header(
            &mut data,
            name_off,
            utf16le_byte_len(username),
            full_name_off,
            utf16le_byte_len(full_name),
            comment_off,
            utf16le_byte_len(comment),
            home_dir_off,
            utf16le_byte_len(home_dir),
            profile_path_off,
            utf16le_byte_len(profile_path),
            script_path_off,
            utf16le_byte_len(script_path),
        );

        let profile = parse_user_v(&data).expect("parse_user_v should succeed");
        assert_eq!(profile.username, "jdoe");
        assert_eq!(profile.full_name, "John Doe");
        assert_eq!(profile.comment, "Engineering");
        assert_eq!(profile.home_dir, "C:\\Users\\jdoe");
        assert_eq!(profile.profile_path, "C:\\Users\\jdoe");
        assert_eq!(profile.script_path, "logon.cmd");
    }

    #[test]
    fn parse_user_v_too_short() {
        let data = vec![0u8; 0x20];
        assert!(parse_user_v(&data).is_none());
    }

    #[test]
    fn parse_user_v_nul_terminated_string() {
        let username = "Guest";
        // Write username with explicit NUL terminator bytes
        let mut data = vec![0u8; 0x200];
        let name_off = 0x50u32;
        let utf16_with_nul: Vec<u8> = username
            .encode_utf16()
            .chain(std::iter::once(0x0000))
            .flat_map(u16::to_le_bytes)
            .collect();
        let name_len = utf16_with_nul.len() as u32; // length includes NUL
        data[name_off as usize..name_off as usize + utf16_with_nul.len()]
            .copy_from_slice(&utf16_with_nul);

        write_user_v_header(&mut data, name_off, name_len, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);

        let profile = parse_user_v(&data).expect("parse_user_v should succeed");
        assert_eq!(profile.username, "Guest");
    }

    #[test]
    fn parse_user_v_out_of_bounds_offset_survives() {
        // Offset pointing past the end of the data should be caught by
        // extract_utf16le_at and return an empty string.
        let mut data = vec![0u8; 0x200];
        let name_off = 0x50u32;
        let username = "test";
        let name_len = utf16le_byte_len(username);
        let utf16_bytes: Vec<u8> = username.encode_utf16().flat_map(u16::to_le_bytes).collect();
        data[name_off as usize..name_off as usize + name_len as usize]
            .copy_from_slice(&utf16_bytes);

        write_user_v_header(
            &mut data, name_off, name_len, 0xFFFF, 32, // full_name_offset out of bounds
            0, 0, 0, 0, 0, 0, 0, 0,
        );

        let profile = parse_user_v(&data).expect("parse_user_v should succeed");
        assert_eq!(profile.username, "test");
        assert_eq!(profile.full_name, ""); // out-of-bounds → empty
    }

    #[test]
    fn parse_user_v_unicode_characters() {
        let full_name = "José María 中文";
        let mut data = vec![0u8; 0x200];
        let full_name_off = 0x50u32;
        let utf16_bytes: Vec<u8> = full_name
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let full_name_len = utf16_bytes.len() as u32;
        data[full_name_off as usize..full_name_off as usize + full_name_len as usize]
            .copy_from_slice(&utf16_bytes);

        write_user_v_header(
            &mut data,
            0,
            0,
            full_name_off,
            full_name_len,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        );

        let profile = parse_user_v(&data).expect("parse_user_v should succeed");
        assert_eq!(profile.full_name, full_name);
    }

    #[test]
    fn parse_user_v_rid_500_administrator_template() {
        // Synthetic V blob for the Administrator account (RID 500).
        // Includes logon timestamps + string fields for full coverage.
        let username = "Administrator";
        let comment = "Built-in account for administering the computer/domain";

        let name_off = 0xCCu32;
        let _name_len = utf16le_byte_len(username);
        let comment_off = 0x100u32;

        let mut data = vec![0u8; 0x400];

        // Write last_login at 0x08 (FILETIME): 2024-06-15 12:00:00 UTC
        let filetime_val: u64 = 133607088000000000; // example
        data[0x08..0x10].copy_from_slice(&filetime_val.to_le_bytes());
        // Write password_last_set at 0x18
        data[0x18..0x20].copy_from_slice(&filetime_val.to_le_bytes());
        // Write RID 500 at 0x28
        data[0x28..0x2C].copy_from_slice(&500u32.to_le_bytes());
        // Write account_control at 0x2C
        data[0x2C..0x30].copy_from_slice(&0x0210u32.to_le_bytes());

        // Write string data
        let username_utf16: Vec<u8> = username.encode_utf16().flat_map(u16::to_le_bytes).collect();
        data[name_off as usize..name_off as usize + username_utf16.len()]
            .copy_from_slice(&username_utf16);

        let comment_utf16: Vec<u8> = comment.encode_utf16().flat_map(u16::to_le_bytes).collect();
        data[comment_off as usize..comment_off as usize + comment_utf16.len()]
            .copy_from_slice(&comment_utf16);

        write_user_v_header(
            &mut data,
            name_off,
            username_utf16.len() as u32,
            0,
            0, // full_name empty
            comment_off,
            comment_utf16.len() as u32,
            0,
            0, // home_dir empty
            0,
            0, // profile_path empty
            0,
            0, // script_path empty
        );

        let profile = parse_user_v(&data).expect("parse_user_v should succeed");
        assert_eq!(profile.username, "Administrator");
        assert_eq!(profile.comment, comment);
        assert_eq!(profile.full_name, "");
        assert_eq!(profile.home_dir, "");
        assert_eq!(profile.profile_path, "");
        assert_eq!(profile.script_path, "");
    }

    // ── DomainAccountF password policy tests ───────────────────────────

    /// Build a synthetic DomainAccountF blob with known values.
    fn make_domain_account_f(
        max_pwd_age: u64,
        min_pwd_age: u64,
        min_pwd_length: u16,
        pwd_history_length: u16,
        lockout_threshold: u16,
        lockout_duration: u64,
        lockout_observation_window: u64,
    ) -> Vec<u8> {
        // Struct size is 96 bytes (0x60)
        let mut data = vec![0u8; 96];
        // revision at 0x00 = 3
        data[0x00..0x04].copy_from_slice(&3u32.to_le_bytes());
        // _pad1 at 0x04 = 0
        // creation_time at 0x08 = 0
        // domain_modified_count at 0x10 = 0
        data[0x18..0x20].copy_from_slice(&max_pwd_age.to_le_bytes());
        data[0x20..0x28].copy_from_slice(&min_pwd_age.to_le_bytes());
        // force_logoff at 0x28 = 0
        data[0x30..0x38].copy_from_slice(&lockout_duration.to_le_bytes());
        data[0x38..0x40].copy_from_slice(&lockout_observation_window.to_le_bytes());
        // _pad2 at 0x40 = 0
        // next_rid at 0x48 = 0
        // pwd_properties at 0x4C — leave as 0
        data[0x50..0x52].copy_from_slice(&min_pwd_length.to_le_bytes());
        data[0x52..0x54].copy_from_slice(&pwd_history_length.to_le_bytes());
        data[0x54..0x56].copy_from_slice(&lockout_threshold.to_le_bytes());
        // _pad3 at 0x56 = 0
        // server_state at 0x58 = 0
        // server_role at 0x5C = 0
        // uas_compatibility_req at 0x5E = 0
        data
    }

    #[test]
    fn parse_domain_account_f_typical_policy() {
        // 42 days max password age, 1 day min, 8 chars min length,
        // 24 passwords remembered, lockout after 5 attempts for 30 min,
        // lockout observation window 30 min.
        let max_pwd_age = 42 * 864_000_000_000u64;
        let min_pwd_age = 864_000_000_000u64;
        let min_pwd_length = 8u16;
        let pwd_history_length = 24u16;
        let lockout_threshold = 5u16;
        let lockout_duration = 30u64 * 60 * 10_000_000; // 30 min in 100ns
        let lockout_observation_window = 30u64 * 60 * 10_000_000;

        let data = make_domain_account_f(
            max_pwd_age,
            min_pwd_age,
            min_pwd_length,
            pwd_history_length,
            lockout_threshold,
            lockout_duration,
            lockout_observation_window,
        );

        let policy = parse_domain_account_f(&data);
        assert!(policy.is_some(), "should parse DomainAccountF");
        let p = policy.unwrap();
        assert_eq!(p.max_password_age_days, 42);
        assert_eq!(p.min_password_age_days, 1);
        assert_eq!(p.min_password_length, 8);
        assert_eq!(p.password_history_length, 24);
        assert_eq!(p.lockout_threshold, 5);
        assert_eq!(p.lockout_duration_minutes, 30);
        assert_eq!(p.lockout_observation_window_minutes, 30);
    }

    #[test]
    fn parse_domain_account_f_default_policy() {
        // Default Windows password policy: 42 days max, 0 days min,
        // 0 min length (but typically 7 enforced by complexity), 0 history,
        // 0 lockout threshold.
        let max_pwd_age = 42 * 864_000_000_000u64;
        let data = make_domain_account_f(max_pwd_age, 0, 0, 0, 0, 0, 0);

        let policy = parse_domain_account_f(&data).expect("should parse");
        assert_eq!(policy.max_password_age_days, 42);
        assert_eq!(policy.min_password_age_days, 0);
        assert_eq!(policy.min_password_length, 0);
        assert_eq!(policy.password_history_length, 0);
        assert_eq!(policy.lockout_threshold, 0);
        assert_eq!(policy.lockout_duration_minutes, 0);
        assert_eq!(policy.lockout_observation_window_minutes, 0);
    }

    #[test]
    fn parse_domain_account_f_never_expires() {
        // max_pwd_age = 0 means never expires
        let data = make_domain_account_f(
            0,
            0,
            7,
            10,
            3,
            15u64 * 60 * 10_000_000,
            15u64 * 60 * 10_000_000,
        );

        let policy = parse_domain_account_f(&data).expect("should parse");
        assert_eq!(policy.max_password_age_days, 0, "0 = never expires");
        assert_eq!(policy.min_password_age_days, 0);
        assert_eq!(policy.lockout_duration_minutes, 15);
    }

    #[test]
    fn parse_domain_account_f_too_short() {
        let data = vec![0u8; 40]; // shorter than 80-byte struct
        assert!(parse_domain_account_f(&data).is_none());
    }
}
