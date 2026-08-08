use super::*;

fn make_user_f(rid: u32, logon_count: u16, account_control: u16) -> Vec<u8> {
    let mut data = vec![0u8; 80];
    data[0x30..0x34].copy_from_slice(&rid.to_le_bytes());
    data[0x38..0x3a].copy_from_slice(&account_control.to_le_bytes());
    data[0x42..0x44].copy_from_slice(&logon_count.to_le_bytes());
    data
}

#[test]
fn parses_user_f_and_rejects_short_data() {
    assert_eq!(
        parse_user_f(&make_user_f(500, 42, 0x300)),
        Some((500, 42, 0x300))
    );
    assert_eq!(parse_user_f(&make_user_f(1001, 15, 0)), Some((1001, 15, 0)));
    assert!(parse_user_f(&[0u8; 20]).is_none());
}

#[test]
fn parses_username_from_v_record() {
    let username = "Administrator";
    let encoded = username
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let mut data = vec![0u8; 0x200];
    data[0x0c..0x10].copy_from_slice(&0x50u32.to_le_bytes());
    data[0x10..0x14].copy_from_slice(&(encoded.len() as u32).to_le_bytes());
    data[0x50..0x50 + encoded.len()].copy_from_slice(&encoded);
    assert_eq!(
        parse_username_from_v_record(&data).as_deref(),
        Some(username)
    );
    assert!(parse_username_from_v_record(&[0u8; 4]).is_none());
}

#[test]
fn parse_username_from_v_record_zero_length() {
    let mut data = vec![0u8; 0x20];
    data[0x10..0x14].copy_from_slice(&0u32.to_le_bytes());
    assert!(parse_username_from_v_record(&data).is_none());
}

fn user_v_blob(fields: &[(&str, usize, usize)]) -> Vec<u8> {
    let mut data = vec![0u8; 0x300];
    for (value, header_offset, data_offset) in fields {
        let encoded = value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        data[*header_offset..*header_offset + 4]
            .copy_from_slice(&(*data_offset as u32).to_le_bytes());
        data[*header_offset + 4..*header_offset + 8]
            .copy_from_slice(&(encoded.len() as u32).to_le_bytes());
        data[*data_offset..*data_offset + encoded.len()].copy_from_slice(&encoded);
    }
    data
}

#[test]
fn parses_all_user_v_profile_fields() {
    let data = user_v_blob(&[
        ("jdoe", 0x0c, 0x80),
        ("Jane Doe", 0x18, 0xa0),
        ("Analyst", 0x24, 0xc0),
        ("C:\\Users\\jdoe", 0x30, 0xe0),
        ("C:\\Profiles\\jdoe", 0x3c, 0x120),
        ("login.cmd", 0x48, 0x160),
    ]);
    let profile = parse_user_v(&data).unwrap();
    assert_eq!(profile.username, "jdoe");
    assert_eq!(profile.full_name, "Jane Doe");
    assert_eq!(profile.comment, "Analyst");
    assert_eq!(profile.home_dir, "C:\\Users\\jdoe");
    assert_eq!(profile.profile_path, "C:\\Profiles\\jdoe");
    assert_eq!(profile.script_path, "login.cmd");
}

#[test]
fn parse_user_v_nul_terminated_string() {
    let username = "Guest";
    let encoded = username
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let mut data = vec![0u8; 0x200];
    data[0x0c..0x10].copy_from_slice(&0x50u32.to_le_bytes());
    data[0x10..0x14].copy_from_slice(&(encoded.len() as u32).to_le_bytes());
    data[0x50..0x50 + encoded.len()].copy_from_slice(&encoded);
    assert_eq!(parse_user_v(&data).unwrap().username, username);
}

#[test]
fn user_v_bounds_and_unicode_are_safe() {
    assert!(parse_user_v(&[0u8; 10]).is_none());
    let mut data = user_v_blob(&[("调查员", 0x0c, 0x80)]);
    data[0x18..0x1c].copy_from_slice(&0xffffu32.to_le_bytes());
    data[0x1c..0x20].copy_from_slice(&8u32.to_le_bytes());
    let profile = parse_user_v(&data).unwrap();
    assert_eq!(profile.username, "调查员");
    assert!(profile.full_name.is_empty());
}

fn domain_policy(
    max_days: u64,
    min_days: u64,
    min_length: u16,
    history: u16,
    threshold: u16,
    duration_minutes: u64,
    observation_minutes: u64,
) -> Vec<u8> {
    let mut data = vec![0u8; 96];
    data[0..4].copy_from_slice(&3u32.to_le_bytes());
    data[0x18..0x20].copy_from_slice(&(max_days * 864_000_000_000).to_le_bytes());
    data[0x20..0x28].copy_from_slice(&(min_days * 864_000_000_000).to_le_bytes());
    data[0x30..0x38].copy_from_slice(&(duration_minutes * 600_000_000).to_le_bytes());
    data[0x38..0x40].copy_from_slice(&(observation_minutes * 600_000_000).to_le_bytes());
    data[0x50..0x52].copy_from_slice(&min_length.to_le_bytes());
    data[0x52..0x54].copy_from_slice(&history.to_le_bytes());
    data[0x54..0x56].copy_from_slice(&threshold.to_le_bytes());
    data
}

#[test]
fn parses_domain_password_policy() {
    let policy = parse_domain_account_f(&domain_policy(42, 1, 8, 24, 5, 30, 30)).unwrap();
    assert_eq!(policy.max_password_age_days, 42);
    assert_eq!(policy.min_password_age_days, 1);
    assert_eq!(policy.min_password_length, 8);
    assert_eq!(policy.password_history_length, 24);
    assert_eq!(policy.lockout_threshold, 5);
    assert_eq!(policy.lockout_duration_minutes, 30);
    assert_eq!(policy.lockout_observation_window_minutes, 30);
}

#[test]
fn domain_policy_zero_intervals_are_preserved() {
    let policy = parse_domain_account_f(&domain_policy(0, 0, 0, 0, 0, 0, 0)).unwrap();
    assert_eq!(policy, SamPasswordPolicy::default());
    assert!(parse_domain_account_f(&[0u8; 20]).is_none());
}

const BASE: usize = 0x1000;
const INVALID: u32 = 0xffff_ffff;

fn write_u16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_node(data: &mut [u8], offset: u32, name: &str, subkeys: &[(&str, u32)], values: &[u32]) {
    let absolute = BASE + offset as usize;
    data[absolute..absolute + 4].copy_from_slice(&(-256i32).to_le_bytes());
    data[absolute + 4..absolute + 6].copy_from_slice(b"nk");
    write_u16(data, absolute + 6, 0x20);
    write_u32(data, absolute + 0x18, subkeys.len() as u32);
    write_u32(
        data,
        absolute + 0x20,
        if subkeys.is_empty() {
            INVALID
        } else {
            0x3000 + offset
        },
    );
    write_u32(data, absolute + 0x28, values.len() as u32);
    write_u32(
        data,
        absolute + 0x2c,
        if values.is_empty() {
            INVALID
        } else {
            0x4000 + offset
        },
    );
    write_u32(data, absolute + 0x34, INVALID);
    write_u16(data, absolute + 0x4c, name.len() as u16);
    data[absolute + 0x50..absolute + 0x50 + name.len()].copy_from_slice(name.as_bytes());
    if !subkeys.is_empty() {
        let list = BASE + 0x3000 + offset as usize;
        data[list..list + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[list + 4..list + 6].copy_from_slice(b"lf");
        write_u16(data, list + 6, subkeys.len() as u16);
        for (index, (child_name, child_offset)) in subkeys.iter().enumerate() {
            let entry = list + 8 + index * 8;
            let mut hash = [0u8; 4];
            for (hash_index, byte) in child_name.as_bytes().iter().take(4).enumerate() {
                hash[hash_index] = *byte;
            }
            data[entry..entry + 4].copy_from_slice(&hash);
            write_u32(data, entry + 4, *child_offset);
        }
    }
    if !values.is_empty() {
        let list = BASE + 0x4000 + offset as usize;
        data[list..list + 4].copy_from_slice(&(-((values.len() as i32 * 4) + 4)).to_le_bytes());
        for (index, value) in values.iter().enumerate() {
            write_u32(data, list + 4 + index * 4, *value);
        }
    }
}

fn set_class(data: &mut [u8], offset: u32, class_name: &str) {
    let absolute = BASE + offset as usize;
    let name_length =
        u16::from_le_bytes(data[absolute + 0x4c..absolute + 0x4e].try_into().unwrap()) as usize;
    let encoded = class_name
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    data[absolute + 0x50 + name_length..absolute + 0x50 + name_length + encoded.len()]
        .copy_from_slice(&encoded);
    write_u16(data, absolute + 0x4e, encoded.len() as u16);
}

fn write_dword(data: &mut [u8], offset: u32, name: &str, value: u32) {
    let absolute = BASE + offset as usize;
    data[absolute..absolute + 4].copy_from_slice(&(-128i32).to_le_bytes());
    data[absolute + 4..absolute + 6].copy_from_slice(b"vk");
    write_u16(data, absolute + 6, name.len() as u16);
    write_u32(data, absolute + 8, 0x8000_0004);
    write_u32(data, absolute + 12, value);
    write_u32(data, absolute + 16, 4);
    write_u16(data, absolute + 20, 1);
    data[absolute + 0x18..absolute + 0x18 + name.len()].copy_from_slice(name.as_bytes());
}

fn boot_key_hive(current: Option<u32>, class_name: &str) -> Vec<u8> {
    let mut data = vec![0u8; 0x10000];
    data[0..4].copy_from_slice(b"regf");
    data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
    data[0x1000..0x1004].copy_from_slice(b"hbin");
    data[0x1008..0x100c].copy_from_slice(&0xf000u32.to_le_bytes());
    write_node(
        &mut data,
        0x20,
        "SYSTEM",
        &[("Select", 0x200), ("ControlSet001", 0x300)],
        &[],
    );
    write_node(
        &mut data,
        0x200,
        "Select",
        &[],
        if current.is_some() { &[0x2000] } else { &[] },
    );
    if let Some(current) = current {
        write_dword(&mut data, 0x2000, "Current", current);
    }
    write_node(
        &mut data,
        0x300,
        "ControlSet001",
        &[("Control", 0x400)],
        &[],
    );
    write_node(&mut data, 0x400, "Control", &[("LSA", 0x500)], &[]);
    write_node(
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
    for (offset, name) in [
        (0x600, "JD"),
        (0x680, "Skew1"),
        (0x700, "GBG"),
        (0x780, "Data"),
    ] {
        write_node(&mut data, offset, name, &[], &[]);
        set_class(&mut data, offset, class_name);
    }
    data
}

#[test]
fn extracts_permuted_boot_key() {
    let mut data = boot_key_hive(Some(1), "aa,aa,aa,aa");
    assert_eq!(extract_boot_key(&data), Some([0xaa; 16]));
    data[BASE + 0x680 + 0x50 + "Skew1".len()] = b'z';
    assert!(extract_boot_key(&data).is_none());
}

#[test]
fn boot_key_falls_back_to_control_set_one() {
    assert_eq!(
        extract_boot_key(&boot_key_hive(None, "bb,bb,bb,bb")),
        Some([0xbb; 16])
    );
}

#[test]
fn extract_boot_key_select_current_dword() {
    let mut data = boot_key_hive(Some(2), "dd,dd,dd,dd");
    write_node(
        &mut data,
        0x20,
        "SYSTEM",
        &[("Select", 0x200), ("ControlSet002", 0x300)],
        &[],
    );
    write_node(
        &mut data,
        0x300,
        "ControlSet002",
        &[("Control", 0x400)],
        &[],
    );
    assert_eq!(extract_boot_key(&data), Some([0xdd; 16]));
}

#[test]
fn boot_key_requires_complete_lsa_tree() {
    let mut data = boot_key_hive(Some(1), "cc,cc,cc,cc");
    let lsa = BASE + 0x500;
    write_u16(&mut data, lsa + 6, 0x20);
    write_u32(&mut data, lsa + 0x18, 0);
    write_u32(&mut data, lsa + 0x20, INVALID);
    assert!(extract_boot_key(&data).is_none());
}
