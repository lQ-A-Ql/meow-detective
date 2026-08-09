use super::*;
use crate::registry::hash_decrypt;
use crate::registry::tests::*;
use zeroize::Zeroizing;

fn build_v_record(username: &str, with_hashes: bool) -> Vec<u8> {
    let mut data = vec![0u8; 204 + 0x200];
    let name: Vec<u8> = username.encode_utf16().flat_map(u16::to_le_bytes).collect();
    // The V record's string pointers are relative to the 0xCC data mark.
    data[0x0c..0x10].copy_from_slice(&0u32.to_le_bytes());
    data[0x10..0x14].copy_from_slice(&(name.len() as u32).to_le_bytes());
    data[204..204 + name.len()].copy_from_slice(&name);
    if with_hashes {
        // Revision-2 (AES) blob: pekid(2) + revision(2) + data offset(4) +
        // salt(16) + ciphertext(16). Offsets are relative to the 204 mark.
        let mut blob = vec![0u8; 40];
        blob[2..4].copy_from_slice(&2u16.to_le_bytes());
        blob[8..24].copy_from_slice(&[0x11; 16]);
        blob[24..40].copy_from_slice(&[0xab; 16]);
        data[0xa8..0xac].copy_from_slice(&0x40u32.to_le_bytes());
        data[0xac..0xb0].copy_from_slice(&(blob.len() as u32).to_le_bytes());
        data[204 + 0x40..204 + 0x40 + blob.len()].copy_from_slice(&blob);
    }
    data
}

fn build_f_record(disabled: bool, failed_logons: u16) -> Vec<u8> {
    let mut data = vec![0u8; 0x50];
    let acb: u16 = if disabled { 0x0211 } else { 0x0210 };
    data[0x38..0x3a].copy_from_slice(&acb.to_le_bytes());
    data[0x40..0x42].copy_from_slice(&failed_logons.to_le_bytes());
    data
}

/// The shared `write_binary_value` helper hardcodes a 128-byte data cell,
/// which cannot hold a full V record; write the cell with its real size.
fn write_large_binary_value(
    data: &mut [u8],
    vk_offset: u32,
    name: &str,
    value: &[u8],
    data_offset: u32,
) {
    let absolute = 0x1000 + data_offset as usize;
    let cell_size = (value.len() + 4).next_multiple_of(8) as i32;
    data[absolute..absolute + 4].copy_from_slice(&(-cell_size).to_le_bytes());
    data[absolute + 4..absolute + 4 + value.len()].copy_from_slice(value);
    write_vk(data, vk_offset, name, 3, value.len() as u32, data_offset);
}

fn build_hive(with_hashes: bool) -> Vec<u8> {
    let mut data = vec![0u8; 0x8000];
    data[0..4].copy_from_slice(b"regf");
    data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
    data[0x1000..0x1004].copy_from_slice(b"hbin");
    data[0x1008..0x100c].copy_from_slice(&0x7000u32.to_le_bytes());
    write_nk(&mut data, 0x20, "ROOT", &[("SAM", 0x80)], &[]);
    write_nk(&mut data, 0x80, "SAM", &[("Domains", 0x100)], &[]);
    write_nk(&mut data, 0x100, "Domains", &[("Account", 0x180)], &[]);
    write_nk(&mut data, 0x180, "Account", &[("Users", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Users", &[("000001F4", 0x280)], &[]);
    write_nk(&mut data, 0x280, "000001F4", &[], &[0x1140, 0x1180]);
    write_large_binary_value(
        &mut data,
        0x1140,
        "V",
        &build_v_record("TestUser", with_hashes),
        0x5000,
    );
    write_binary_value(&mut data, 0x1180, "F", &build_f_record(true, 7), 0x5600);
    data
}

#[test]
fn lists_accounts_with_flags_and_password_state() {
    let hive = build_hive(true);
    let accounts = list_accounts(&hive).unwrap();
    assert_eq!(accounts.len(), 1);
    let account = &accounts[0];
    assert_eq!(account.rid, 500);
    assert!(account.disabled);
    assert_eq!(account.username, "TestUser");
    // 7 failed logons but no ACB_AUTO_LOCKED: history alone is not a lockout.
    assert!(!account.locked_out);
    assert!(account.has_password);
}

#[test]
fn lockout_requires_the_auto_locked_flag() {
    let mut hive = build_hive(true);
    // Set ACB_AUTO_LOCKED in the F value (cell header is 4 bytes at 0x6600).
    let f_flags = 0x1000 + 0x5600 + 4 + F_ACB_OFFSET;
    hive[f_flags..f_flags + 2].copy_from_slice(&ACB_AUTO_LOCKED.to_le_bytes());
    assert!(list_accounts(&hive).unwrap()[0].locked_out);
}

#[test]
fn clear_password_reencrypts_the_empty_hashes() {
    let mut hive = build_hive(true);
    let hbootkey = Zeroizing::new([0x42; 32]);
    let outcome = apply_bypass(&mut hive, 500, SamBypassAction::ClearPassword, &hbootkey).unwrap();
    assert!(outcome.password_cleared);
    assert!(!outcome.already_passwordless);

    let reader = crate::registry::lookup::RegistryHiveReader::new(&hive).unwrap();
    let node = reader
        .navigate_to(&["SAM", "Domains", "Account", "Users", "000001F4"])
        .unwrap()
        .unwrap();
    let v = reader.read_raw_value_bytes(&node, "V").unwrap().unwrap();
    let hashes = hash_decrypt::decrypt_user_hashes(*hbootkey, 500, &v).unwrap();
    assert_eq!(hashes.nt, hash_decrypt::NT_HASH_EMPTY);
}

#[test]
fn enable_and_clear_unlocks_the_account_flags() {
    let mut hive = build_hive(true);
    let hbootkey = Zeroizing::new([0x7; 32]);
    let outcome = apply_bypass(
        &mut hive,
        500,
        SamBypassAction::EnableAndClearPassword,
        &hbootkey,
    )
    .unwrap();
    assert!(outcome.password_cleared);
    assert!(outcome.account_enabled);

    let accounts = list_accounts(&hive).unwrap();
    assert!(!accounts[0].disabled);
    assert!(!accounts[0].locked_out);
}

#[test]
fn a_dirty_hive_is_refused() {
    let mut hive = build_hive(true);
    hive[8..12].copy_from_slice(&1u32.to_le_bytes());
    let hbootkey = Zeroizing::new([0x42; 32]);
    assert!(apply_bypass(&mut hive, 500, SamBypassAction::ClearPassword, &hbootkey).is_err());
}

#[test]
fn a_passwordless_account_reports_already_clear() {
    let mut hive = build_hive(false);
    let hbootkey = Zeroizing::new([0x42; 32]);
    let outcome = apply_bypass(&mut hive, 500, SamBypassAction::ClearPassword, &hbootkey).unwrap();
    assert!(!outcome.password_cleared);
    assert!(outcome.already_passwordless);
}

#[test]
fn a_corrupt_v_hash_pointer_is_refused() {
    let mut hive = build_hive(true);
    // The NT blob pointer escapes the V value cell (204 + 0x200 bytes at
    // 0x6004) while staying inside the hive: the rewrite must not land
    // outside the cell.
    let v_nt_offset = 0x1000 + 0x5000 + 4 + V_NT_OFFSET_FIELD;
    hive[v_nt_offset..v_nt_offset + 4].copy_from_slice(&0x400u32.to_le_bytes());
    let hbootkey = Zeroizing::new([0x42; 32]);
    let error = apply_bypass(&mut hive, 500, SamBypassAction::ClearPassword, &hbootkey)
        .expect_err("corrupt V pointer must be refused");
    assert!(
        error.contains("escapes the V value cell"),
        "unexpected error: {error}"
    );
}

#[test]
fn a_cleared_password_verifies_as_blank() {
    let mut hive = build_hive(true);
    let hbootkey = Zeroizing::new([0x42; 32]);
    assert!(!account_password_is_blank(&hive, 500, &hbootkey).unwrap());
    apply_bypass(&mut hive, 500, SamBypassAction::ClearPassword, &hbootkey).unwrap();
    assert!(account_password_is_blank(&hive, 500, &hbootkey).unwrap());
    // An account with no stored hash is blank regardless of the key.
    let passwordless = build_hive(false);
    assert!(account_password_is_blank(&passwordless, 500, &hbootkey).unwrap());
}
