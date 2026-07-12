use super::*;
pub(crate) use crate::registry::tests::*;

#[test]
fn reject_non_regf() {
    assert!(RegistryHiveReader::new(b"not-registry").is_err());
}

#[test]
fn reject_invalid_hbin_headers() {
    let mut missing_magic = empty_hive("ROOT");
    missing_magic[0x1000..0x1004].copy_from_slice(b"NOPE");
    assert!(RegistryHiveReader::new(&missing_magic).is_err());

    let mut zero_size = empty_hive("ROOT");
    zero_size[0x1008..0x100c].copy_from_slice(&0u32.to_le_bytes());
    assert!(RegistryHiveReader::new(&zero_size).is_err());

    let mut unaligned = empty_hive("ROOT");
    unaligned[0x1008..0x100c].copy_from_slice(&0x1234u32.to_le_bytes());
    assert!(RegistryHiveReader::new(&unaligned).is_err());
}

#[test]
fn reject_truncated_and_out_of_range_roots() {
    let mut truncated = vec![0u8; 0x1010];
    truncated[0..4].copy_from_slice(b"regf");
    truncated[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
    assert!(RegistryHiveReader::new(&truncated).is_err());

    let mut out_of_range = empty_hive("ROOT");
    out_of_range[0x24..0x28].copy_from_slice(&0x3000u32.to_le_bytes());
    assert!(RegistryHiveReader::new(&out_of_range).is_err());
}

#[test]
fn key_path_depth_is_bounded() {
    let data = empty_hive("ROOT");
    let hive = RegistryHiveReader::new(&data).unwrap();
    let too_deep = (0..65).map(|_| "x").collect::<Vec<_>>();
    assert!(hive
        .lookup_value(&too_deep, "val")
        .unwrap_err()
        .contains("depth"));
    let allowed = (0..64).map(|_| "x").collect::<Vec<_>>();
    assert!(hive.lookup_value(&allowed, "val").is_ok());
}

#[test]
fn parses_base_block_and_node_names() {
    let data = empty_hive("SYSTEM");
    let hive = RegistryHiveReader::new(&data).unwrap();
    assert_eq!(hive.root_cell_offset, 0x20);
    assert_eq!(hive.parse_nk(0x20).unwrap().name, "SYSTEM");

    let mut utf16 = empty_hive("ROOT");
    write_nk_utf16_name(&mut utf16, 0x20, "SYST\u{00c8}M");
    assert_eq!(
        RegistryHiveReader::new(&utf16)
            .unwrap()
            .parse_nk(0x20)
            .unwrap()
            .name,
        "SYST\u{00c8}M"
    );
}

fn assert_child_lookup(data: &[u8]) {
    let hive = RegistryHiveReader::new(data).unwrap();
    assert_eq!(
        hive.lookup_value(&["Child"], "Name").unwrap(),
        Some(RegistryValue::String("Value".to_string()))
    );
}

fn child_hive() -> Vec<u8> {
    let mut data = empty_hive("ROOT");
    write_nk(&mut data, 0x20, "ROOT", &[], &[]);
    write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
    write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
    data
}

#[test]
fn reads_lf_and_lh_subkey_lists() {
    let mut lf = child_hive();
    set_nk_subkey_list(&mut lf, 0x20, 0x2020, 1);
    write_hashed_subkey_list(&mut lf, 0x2020, b"lf", &[("Child", 0x200)]);
    assert_child_lookup(&lf);

    let mut lh = child_hive();
    set_nk_subkey_list(&mut lh, 0x20, 0x2020, 1);
    write_hashed_subkey_list(&mut lh, 0x2020, b"lh", &[("Child", 0x200)]);
    assert_child_lookup(&lh);
}

#[test]
fn reads_offset_first_lf_layout() {
    let mut data = child_hive();
    set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
    let absolute = BASE_BLOCK_SIZE + 0x2020;
    data[absolute..absolute + 4].copy_from_slice(&(-256i32).to_le_bytes());
    data[absolute + 4..absolute + 6].copy_from_slice(b"lf");
    data[absolute + 6..absolute + 8].copy_from_slice(&1u16.to_le_bytes());
    data[absolute + 8..absolute + 12].copy_from_slice(&0x200u32.to_le_bytes());
    data[absolute + 12..absolute + 16].copy_from_slice(b"Chil");
    assert_child_lookup(&data);
}

#[test]
fn reads_li_and_ri_subkey_lists() {
    let mut li = child_hive();
    set_nk_subkey_list(&mut li, 0x20, 0x2020, 1);
    write_flat_subkey_list(&mut li, 0x2020, b"li", &[0x200]);
    assert_child_lookup(&li);

    let mut ri = child_hive();
    set_nk_subkey_list(&mut ri, 0x20, 0x2020, 1);
    write_flat_subkey_list(&mut ri, 0x2020, b"ri", &[0x2080]);
    write_flat_subkey_list(&mut ri, 0x2080, b"li", &[0x200]);
    assert_child_lookup(&ri);
}

#[test]
fn reads_supported_value_types() {
    let mut dword = empty_hive("ROOT");
    write_nk(&mut dword, 0x20, "ROOT", &[], &[0x400]);
    write_dword_value(&mut dword, 0x400, "Current", 1);
    assert_eq!(
        RegistryHiveReader::new(&dword)
            .unwrap()
            .lookup_value(&[], "Current")
            .unwrap(),
        Some(RegistryValue::Dword(1))
    );

    let mut expand = empty_hive("ROOT");
    write_nk(&mut expand, 0x20, "ROOT", &[], &[0x400]);
    write_typed_string_value(
        &mut expand,
        0x400,
        "Path",
        REG_EXPAND_SZ,
        "%SystemRoot%\\System32",
        0x700,
    );
    assert_eq!(
        RegistryHiveReader::new(&expand)
            .unwrap()
            .lookup_value(&[], "Path")
            .unwrap(),
        Some(RegistryValue::String("%SystemRoot%\\System32".to_string()))
    );

    let mut multi = empty_hive("ROOT");
    write_nk(&mut multi, 0x20, "ROOT", &[], &[0x400]);
    write_multi_string_value(&mut multi, 0x400, "Services", &["Tcpip", "Dnscache"], 0x700);
    assert_eq!(
        RegistryHiveReader::new(&multi)
            .unwrap()
            .lookup_value(&[], "Services")
            .unwrap(),
        Some(RegistryValue::MultiString(vec![
            "Tcpip".to_string(),
            "Dnscache".to_string()
        ]))
    );

    let mut qword = empty_hive("ROOT");
    write_nk(&mut qword, 0x20, "ROOT", &[], &[0x400]);
    write_qword_value(&mut qword, 0x400, "Counter", 0x1122_3344_5566_7788, 0x700);
    assert_eq!(
        RegistryHiveReader::new(&qword)
            .unwrap()
            .lookup_value(&[], "Counter")
            .unwrap(),
        Some(RegistryValue::Qword(0x1122_3344_5566_7788))
    );
}

#[test]
fn rejects_malformed_value_storage() {
    let mut odd_utf16 = empty_hive("ROOT");
    write_nk(&mut odd_utf16, 0x20, "ROOT", &[], &[0x400]);
    let data_absolute = BASE_BLOCK_SIZE + 0x700;
    odd_utf16[data_absolute..data_absolute + 4].copy_from_slice(&(-8i32).to_le_bytes());
    odd_utf16[data_absolute + 4..data_absolute + 7].copy_from_slice(b"A\0B");
    write_vk(&mut odd_utf16, 0x400, "Odd", REG_SZ, 3, 0x700);
    assert!(RegistryHiveReader::new(&odd_utf16)
        .unwrap()
        .lookup_value(&[], "Odd")
        .unwrap_err()
        .contains("UTF-16 data has odd byte length"));

    let mut inline = empty_hive("ROOT");
    write_nk(&mut inline, 0x20, "ROOT", &[], &[0x400]);
    write_vk(&mut inline, 0x400, "TooLong", REG_DWORD, 0x8000_0005, 1);
    assert!(RegistryHiveReader::new(&inline)
        .unwrap()
        .lookup_value(&[], "TooLong")
        .unwrap_err()
        .contains("exceeds 4 bytes"));

    let mut short = empty_hive("ROOT");
    write_nk(&mut short, 0x20, "ROOT", &[], &[0x400]);
    short[data_absolute..data_absolute + 4].copy_from_slice(&(-8i32).to_le_bytes());
    short[data_absolute + 4..data_absolute + 6].copy_from_slice(&1u16.to_le_bytes());
    write_vk(&mut short, 0x400, "Short", REG_DWORD, 2, 0x700);
    assert!(RegistryHiveReader::new(&short)
        .unwrap()
        .lookup_value(&[], "Short")
        .unwrap_err()
        .contains("REG_DWORD value shorter than 4 bytes"));
}

#[test]
fn value_list_and_cell_bounds_are_enforced() {
    let mut data = empty_hive("ROOT");
    write_nk(&mut data, 0x20, "ROOT", &[], &[0x400, 0x500]);
    write_dword_value(&mut data, 0x400, "First", 1);
    write_dword_value(&mut data, 0x500, "Second", 2);
    assert_eq!(
        RegistryHiveReader::new(&data)
            .unwrap()
            .lookup_value(&[], "Second")
            .unwrap(),
        Some(RegistryValue::Dword(2))
    );

    let list_absolute = BASE_BLOCK_SIZE + 0x4020;
    data[list_absolute..list_absolute + 4].copy_from_slice(&(-4i32).to_le_bytes());
    assert!(RegistryHiveReader::new(&data)
        .unwrap()
        .lookup_value(&[], "Second")
        .unwrap_err()
        .contains("value list"));
}

#[test]
fn corrupt_cells_return_errors() {
    let data = empty_hive("ROOT");
    assert!(RegistryHiveReader::new(&data)
        .unwrap()
        .parse_nk(0xffff)
        .is_err());

    let mut corrupt = empty_hive("ROOT");
    corrupt[0x1020..0x1024].copy_from_slice(&(-999_999i32).to_le_bytes());
    assert!(RegistryHiveReader::new(&corrupt)
        .unwrap()
        .parse_nk(0x20)
        .is_err());
}
