#[path = "txlog_fixture.rs"]
pub(crate) mod txlog_fixture;

const BASE_BLOCK_SIZE: usize = 0x1000;
const INVALID_OFFSET: u32 = 0xffff_ffff;
const REG_SZ: u32 = 1;
const REG_BINARY: u32 = 3;
const REG_DWORD: u32 = 4;
const REG_MULTI_SZ: u32 = 7;
const REG_QWORD: u32 = 11;
const USER_ASSIST_ENTRY_SIZE: usize = 72;
const SAM_ACCOUNT_DISABLED: u32 = 0x0001;

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
    let absolute = BASE_BLOCK_SIZE + offset as usize;
    let name_bytes = name.as_bytes();
    data[absolute..absolute + 4].copy_from_slice(&(-256i32).to_le_bytes());
    data[absolute + 4..absolute + 6].copy_from_slice(b"nk");
    data[absolute + 6..absolute + 8].copy_from_slice(&0x20u16.to_le_bytes());
    data[absolute + 0x18..absolute + 0x1c].copy_from_slice(&(subkeys.len() as u32).to_le_bytes());
    let subkey_list_offset = 0x2000 + offset;
    let value_list_offset = 0x4000 + offset;
    data[absolute + 0x20..absolute + 0x24].copy_from_slice(
        &if subkeys.is_empty() {
            INVALID_OFFSET
        } else {
            subkey_list_offset
        }
        .to_le_bytes(),
    );
    data[absolute + 0x28..absolute + 0x2c].copy_from_slice(&(values.len() as u32).to_le_bytes());
    data[absolute + 0x2c..absolute + 0x30].copy_from_slice(
        &if values.is_empty() {
            INVALID_OFFSET
        } else {
            value_list_offset
        }
        .to_le_bytes(),
    );
    data[absolute + 0x4c..absolute + 0x4e]
        .copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    data[absolute + 0x50..absolute + 0x50 + name_bytes.len()].copy_from_slice(name_bytes);

    if !values.is_empty() {
        let list_absolute = BASE_BLOCK_SIZE + value_list_offset as usize;
        data[list_absolute..list_absolute + 4]
            .copy_from_slice(&(-((values.len() as i32 * 4) + 4)).to_le_bytes());
        for (index, value_offset) in values.iter().enumerate() {
            let entry = list_absolute + 4 + index * 4;
            data[entry..entry + 4].copy_from_slice(&value_offset.to_le_bytes());
        }
    }
    if !subkeys.is_empty() {
        write_hashed_subkey_list(data, subkey_list_offset, b"lf", subkeys);
    }
}

pub(crate) fn write_nk_utf16_name(data: &mut [u8], offset: u32, name: &str) {
    let absolute = BASE_BLOCK_SIZE + offset as usize;
    let name_bytes = name
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    data[absolute..absolute + 4].copy_from_slice(&(-256i32).to_le_bytes());
    data[absolute + 4..absolute + 6].copy_from_slice(b"nk");
    data[absolute + 6..absolute + 8].copy_from_slice(&0u16.to_le_bytes());
    data[absolute + 0x20..absolute + 0x24].copy_from_slice(&INVALID_OFFSET.to_le_bytes());
    data[absolute + 0x2c..absolute + 0x30].copy_from_slice(&INVALID_OFFSET.to_le_bytes());
    data[absolute + 0x4c..absolute + 0x4e]
        .copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    data[absolute + 0x50..absolute + 0x50 + name_bytes.len()].copy_from_slice(&name_bytes);
}

pub(crate) fn write_hashed_subkey_list(
    data: &mut [u8],
    offset: u32,
    signature: &[u8; 2],
    subkeys: &[(&str, u32)],
) {
    let absolute = BASE_BLOCK_SIZE + offset as usize;
    data[absolute..absolute + 4].copy_from_slice(&(-256i32).to_le_bytes());
    data[absolute + 4..absolute + 6].copy_from_slice(signature);
    data[absolute + 6..absolute + 8].copy_from_slice(&(subkeys.len() as u16).to_le_bytes());
    for (index, (name, child_offset)) in subkeys.iter().enumerate() {
        let entry = absolute + 8 + index * 8;
        let mut hash = [0u8; 4];
        for (hash_index, byte) in name.as_bytes().iter().take(4).enumerate() {
            hash[hash_index] = *byte;
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
    let absolute = BASE_BLOCK_SIZE + offset as usize;
    data[absolute..absolute + 4].copy_from_slice(&(-256i32).to_le_bytes());
    data[absolute + 4..absolute + 6].copy_from_slice(signature);
    data[absolute + 6..absolute + 8].copy_from_slice(&(subkeys.len() as u16).to_le_bytes());
    for (index, child_offset) in subkeys.iter().enumerate() {
        let entry = absolute + 8 + index * 4;
        data[entry..entry + 4].copy_from_slice(&child_offset.to_le_bytes());
    }
}

pub(crate) fn set_nk_subkey_list(data: &mut [u8], node_offset: u32, list_offset: u32, count: u32) {
    let absolute = BASE_BLOCK_SIZE + node_offset as usize;
    data[absolute + 0x18..absolute + 0x1c].copy_from_slice(&count.to_le_bytes());
    data[absolute + 0x20..absolute + 0x24].copy_from_slice(&list_offset.to_le_bytes());
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
    let encoded = value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    write_external_value_data(data, data_offset, &encoded);
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
    write_external_value_data(data, data_offset, &encoded);
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
    write_external_value_data(data, data_offset, &value.to_le_bytes());
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
    let absolute = BASE_BLOCK_SIZE + offset as usize;
    let name_bytes = name.as_bytes();
    data[absolute..absolute + 4].copy_from_slice(&(-128i32).to_le_bytes());
    data[absolute + 4..absolute + 6].copy_from_slice(b"vk");
    data[absolute + 6..absolute + 8].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    data[absolute + 8..absolute + 12].copy_from_slice(&data_len.to_le_bytes());
    data[absolute + 12..absolute + 16].copy_from_slice(&data_offset.to_le_bytes());
    data[absolute + 16..absolute + 20].copy_from_slice(&value_type.to_le_bytes());
    data[absolute + 20..absolute + 22].copy_from_slice(&1u16.to_le_bytes());
    data[absolute + 0x18..absolute + 0x18 + name_bytes.len()].copy_from_slice(name_bytes);
}

pub(crate) fn write_binary_value(
    data: &mut [u8],
    offset: u32,
    name: &str,
    value_data: &[u8],
    data_offset: u32,
) {
    write_external_value_data(data, data_offset, value_data);
    write_vk(
        data,
        offset,
        name,
        REG_BINARY,
        value_data.len() as u32,
        data_offset,
    );
}

fn write_external_value_data(data: &mut [u8], data_offset: u32, value: &[u8]) {
    let absolute = BASE_BLOCK_SIZE + data_offset as usize;
    data[absolute..absolute + 4].copy_from_slice(&(-128i32).to_le_bytes());
    data[absolute + 4..absolute + 4 + value.len()].copy_from_slice(value);
}

pub(crate) fn make_recent_doc_binary(file_name: &str) -> Vec<u8> {
    let utf16 = file_name
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let mut result = ((utf16.len() + 6) as u32).to_le_bytes().to_vec();
    result.extend_from_slice(&utf16);
    result.extend_from_slice(&[0, 0]);
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
    for index in indices {
        data.extend_from_slice(&index.to_le_bytes());
    }
    data.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
    data
}

pub(crate) fn make_sam_v_record(
    last_login: u64,
    password_last_set: u64,
    rid: u32,
    account_control: u32,
    admin_count: u16,
) -> Vec<u8> {
    let mut data = vec![0u8; 0x50];
    data[0x08..0x10].copy_from_slice(&last_login.to_le_bytes());
    data[0x18..0x20].copy_from_slice(&password_last_set.to_le_bytes());
    data[0x28..0x2c].copy_from_slice(&rid.to_le_bytes());
    data[0x2c..0x30].copy_from_slice(&account_control.to_le_bytes());
    data[0x46..0x48].copy_from_slice(&admin_count.to_le_bytes());
    data
}

/// SAM per-user `F` value in the validated on-disk layout: last logon at
/// `0x08`, last password change at `0x18`, RID at `0x30`, ACB flags at
/// `0x38`, total logon count at `0x42`.
pub(crate) fn make_sam_f_record(
    last_login: u64,
    password_last_set: u64,
    rid: u32,
    account_control: u16,
    logon_count: u16,
) -> Vec<u8> {
    let mut data = vec![0u8; 80];
    data[0x08..0x10].copy_from_slice(&last_login.to_le_bytes());
    data[0x18..0x20].copy_from_slice(&password_last_set.to_le_bytes());
    data[0x30..0x34].copy_from_slice(&rid.to_le_bytes());
    data[0x38..0x3a].copy_from_slice(&account_control.to_le_bytes());
    data[0x42..0x44].copy_from_slice(&logon_count.to_le_bytes());
    data
}

pub(crate) fn make_sid(sub_authorities: &[u32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(8 + sub_authorities.len() * 4);
    data.push(1);
    data.push(sub_authorities.len() as u8);
    data.extend_from_slice(&[0, 0, 0, 0, 0, 5]);
    for authority in sub_authorities {
        data.extend_from_slice(&authority.to_le_bytes());
    }
    data
}

pub(crate) fn make_sam_c_value(member_sids: &[Vec<u8>]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&(member_sids.len() as u32).to_le_bytes());
    for sid in member_sids {
        data.extend_from_slice(sid);
    }
    data
}

pub(crate) fn make_domain_account_f_blob(
    max_age_days: u64,
    min_age_days: u64,
    min_length: u16,
    history_length: u16,
    lockout_threshold: u16,
    lockout_duration_minutes: u64,
    observation_minutes: u64,
) -> Vec<u8> {
    let mut data = vec![0u8; 96];
    data[0..4].copy_from_slice(&3u32.to_le_bytes());
    data[0x18..0x20].copy_from_slice(&(max_age_days * 864_000_000_000).to_le_bytes());
    data[0x20..0x28].copy_from_slice(&(min_age_days * 864_000_000_000).to_le_bytes());
    data[0x30..0x38].copy_from_slice(&(lockout_duration_minutes * 600_000_000).to_le_bytes());
    data[0x38..0x40].copy_from_slice(&(observation_minutes * 600_000_000).to_le_bytes());
    data[0x50..0x52].copy_from_slice(&min_length.to_le_bytes());
    data[0x52..0x54].copy_from_slice(&history_length.to_le_bytes());
    data[0x54..0x56].copy_from_slice(&lockout_threshold.to_le_bytes());
    data
}

pub(crate) fn encode_utf16le(value: &str) -> Vec<u8> {
    let mut data = value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    data.extend_from_slice(&[0, 0]);
    data
}

pub(crate) fn synthetic_sam_hive() -> Vec<u8> {
    let mut data = vec![0u8; 0x8000];
    data[0..4].copy_from_slice(b"regf");
    data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
    data[0x1000..0x1004].copy_from_slice(b"hbin");
    data[0x1008..0x100c].copy_from_slice(&0x7000u32.to_le_bytes());
    write_nk(&mut data, 0x020, "ROOT", &[("SAM", 0x080)], &[]);
    write_nk(&mut data, 0x080, "SAM", &[("Domains", 0x100)], &[]);
    write_nk(
        &mut data,
        0x100,
        "Domains",
        &[("Account", 0x180), ("Builtin", 0x880)],
        &[],
    );
    write_nk(
        &mut data,
        0x180,
        "Account",
        &[("Users", 0x200), ("Aliases", 0x500)],
        &[0x1240, 0x1260],
    );
    let account_f = make_domain_account_f_blob(42, 1, 8, 24, 5, 30, 30);
    write_binary_value(&mut data, 0x1240, "F", &account_f, 0x5400);
    let mut account_v = vec![0u8; 0x1c0];
    account_v[0x198..0x19c].copy_from_slice(&21u32.to_le_bytes());
    account_v[0x19c..0x1a0].copy_from_slice(&123_456_789u32.to_le_bytes());
    account_v[0x1a0..0x1a4].copy_from_slice(&123_456_789u32.to_le_bytes());
    account_v[0x1a4..0x1a8].copy_from_slice(&123_456_789u32.to_le_bytes());
    let value_absolute = BASE_BLOCK_SIZE + 0x5500;
    data[value_absolute..value_absolute + 4].copy_from_slice(&(-0x200i32).to_le_bytes());
    data[value_absolute + 4..value_absolute + 4 + account_v.len()].copy_from_slice(&account_v);
    write_vk(
        &mut data,
        0x1260,
        "V",
        REG_BINARY,
        account_v.len() as u32,
        0x5500,
    );
    write_nk(
        &mut data,
        0x200,
        "Users",
        &[("Names", 0x280), ("000001F4", 0x400), ("000001F5", 0x480)],
        &[],
    );
    write_nk(
        &mut data,
        0x280,
        "Names",
        &[("Administrator", 0x300), ("Guest", 0x380)],
        &[],
    );
    write_nk(&mut data, 0x300, "Administrator", &[], &[0x1100]);
    write_dword_value(&mut data, 0x1100, "", 500);
    write_nk(&mut data, 0x380, "Guest", &[], &[0x1120]);
    write_dword_value(&mut data, 0x1120, "", 501);
    write_nk(&mut data, 0x400, "000001F4", &[], &[0x1140, 0x1280]);
    write_binary_value(
        &mut data,
        0x1140,
        "V",
        &make_sam_v_record(133_600_000_000_000_000, 133_500_000_000_000_000, 500, 0, 3),
        0x5000,
    );
    // Administrator: enabled normal account (ACB 0x0214), 7 logons.
    write_binary_value(
        &mut data,
        0x1280,
        "F",
        &make_sam_f_record(
            133_600_000_000_000_000,
            133_500_000_000_000_000,
            500,
            0x0214,
            7,
        ),
        0x5700,
    );
    write_nk(&mut data, 0x480, "000001F5", &[], &[0x1160, 0x12a0]);
    write_binary_value(
        &mut data,
        0x1160,
        "V",
        &make_sam_v_record(0, 133_400_000_000_000_000, 501, SAM_ACCOUNT_DISABLED, 0),
        0x5100,
    );
    // Guest: disabled account (ACB disabled bit set), 0 logons.
    write_binary_value(
        &mut data,
        0x12a0,
        "F",
        &make_sam_f_record(0, 133_400_000_000_000_000, 501, 0x0211, 0),
        0x5800,
    );
    write_nk(
        &mut data,
        0x500,
        "Aliases",
        &[("Names", 0x580), ("00000220", 0x700), ("00000221", 0x780)],
        &[],
    );
    write_nk(
        &mut data,
        0x580,
        "Names",
        &[("Administrators", 0x600), ("Users", 0x680)],
        &[],
    );
    write_nk(&mut data, 0x600, "Administrators", &[], &[0x1180]);
    write_dword_value(&mut data, 0x1180, "", 544);
    write_nk(&mut data, 0x680, "Users", &[], &[0x11a0]);
    write_dword_value(&mut data, 0x11a0, "", 545);
    write_nk(&mut data, 0x700, "00000220", &[], &[0x11c0]);
    write_binary_value(
        &mut data,
        0x11c0,
        "C",
        &make_sam_c_value(&[make_sid(&[21, 123_456_789, 123_456_789, 123_456_789, 500])]),
        0x5200,
    );
    write_nk(&mut data, 0x780, "00000221", &[], &[0x11e0]);
    write_binary_value(
        &mut data,
        0x11e0,
        "C",
        &make_sam_c_value(&[
            make_sid(&[21, 123_456_789, 123_456_789, 123_456_789, 500]),
            make_sid(&[21, 123_456_789, 123_456_789, 123_456_789, 501]),
        ]),
        0x5300,
    );
    write_nk(&mut data, 0x880, "Builtin", &[("Aliases", 0x900)], &[]);
    write_nk(&mut data, 0x900, "Aliases", &[("Names", 0x980)], &[]);
    write_nk(
        &mut data,
        0x980,
        "Names",
        &[("Administrators", 0xa00), ("Users", 0xa80)],
        &[],
    );
    write_nk(&mut data, 0xa00, "Administrators", &[], &[0x1200]);
    write_dword_value(&mut data, 0x1200, "", 544);
    write_nk(&mut data, 0xa80, "Users", &[], &[0x1220]);
    write_dword_value(&mut data, 0x1220, "", 545);
    data
}
