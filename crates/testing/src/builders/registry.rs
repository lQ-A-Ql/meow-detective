//! Synthetic Windows Registry hive fixtures for targeted parser tests.
//!
//! These hives are deliberately tiny and only contain the keys required by
//! Analysis system-info tests. They are not full Windows registry samples.

const BASE_BLOCK_SIZE: usize = 0x1000;
const INVALID_OFFSET: u32 = 0xFFFF_FFFF;
const REG_SZ: u32 = 1;
const REG_DWORD: u32 = 4;

/// File name used for the synthetic SYSTEM hive fixture.
pub const SYSTEM_HIVE_NAME: &str = "SYSTEM";

/// File name used for the synthetic SOFTWARE hive fixture.
pub const SOFTWARE_HIVE_NAME: &str = "SOFTWARE";

/// Synthetic hostname stored in the tiny SYSTEM hive.
pub const SYSTEM_COMPUTER_NAME: &str = "BETA-LAB";

/// Synthetic timezone stored in the tiny SYSTEM hive.
pub const SYSTEM_TIMEZONE: &str = "China Standard Time";

/// Synthetic product name stored in the tiny SOFTWARE hive.
pub const SOFTWARE_PRODUCT_NAME: &str = "Forensics Fixture OS";

/// Synthetic Windows build value stored in the tiny SOFTWARE hive.
pub const SOFTWARE_CURRENT_BUILD: &str = "26000";

/// Synthetic display version stored in the tiny SOFTWARE hive.
pub const SOFTWARE_DISPLAY_VERSION: &str = "24H2";

/// Synthetic registered owner stored in the tiny SOFTWARE hive.
pub const SOFTWARE_REGISTERED_OWNER: &str = "DFIR Team";

/// Synthetic product id stored in the tiny SOFTWARE hive.
pub const SOFTWARE_PRODUCT_ID: &str = "00330-80000";

/// Synthetic install-date DWORD stored in the tiny SOFTWARE hive.
pub const SOFTWARE_INSTALL_DATE: u32 = 1_700_000_000;

/// Build a tiny synthetic SYSTEM hive with Analysis-relevant fields.
pub fn synthetic_system_hive() -> Vec<u8> {
    let mut data = empty_hive(SYSTEM_HIVE_NAME);
    write_nk(
        &mut data,
        0x20,
        SYSTEM_HIVE_NAME,
        &[("Select", 0x200), ("ControlSet001", 0x300)],
        &[],
    );
    write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
    write_dword_value(&mut data, 0x1200, "Current", 1);
    write_nk(
        &mut data,
        0x300,
        "ControlSet001",
        &[("Control", 0x400)],
        &[],
    );
    write_nk(
        &mut data,
        0x400,
        "Control",
        &[("ComputerName", 0x600), ("TimeZoneInformation", 0xa00)],
        &[],
    );
    write_nk(
        &mut data,
        0x600,
        "ComputerName",
        &[("ComputerName", 0x800)],
        &[],
    );
    write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
    write_string_value(
        &mut data,
        0xc00,
        "ComputerName",
        SYSTEM_COMPUTER_NAME,
        0x1800,
    );
    write_nk(&mut data, 0xa00, "TimeZoneInformation", &[], &[0xd00]);
    write_string_value(&mut data, 0xd00, "TimeZoneKeyName", SYSTEM_TIMEZONE, 0x1900);
    data
}

/// Build a tiny synthetic SOFTWARE hive with Analysis-relevant fields.
pub fn synthetic_software_hive() -> Vec<u8> {
    let mut data = empty_hive(SOFTWARE_HIVE_NAME);
    write_nk(
        &mut data,
        0x20,
        SOFTWARE_HIVE_NAME,
        &[("Microsoft", 0x200)],
        &[],
    );
    write_nk(&mut data, 0x200, "Microsoft", &[("Windows NT", 0x300)], &[]);
    write_nk(
        &mut data,
        0x300,
        "Windows NT",
        &[("CurrentVersion", 0x400)],
        &[],
    );
    write_nk(
        &mut data,
        0x400,
        "CurrentVersion",
        &[],
        &[0x600, 0x680, 0x700, 0x780, 0x800, 0x880],
    );
    write_string_value(
        &mut data,
        0x600,
        "ProductName",
        SOFTWARE_PRODUCT_NAME,
        0x1800,
    );
    write_string_value(
        &mut data,
        0x680,
        "CurrentBuild",
        SOFTWARE_CURRENT_BUILD,
        0x1900,
    );
    write_string_value(
        &mut data,
        0x700,
        "DisplayVersion",
        SOFTWARE_DISPLAY_VERSION,
        0x1a00,
    );
    write_string_value(
        &mut data,
        0x780,
        "RegisteredOwner",
        SOFTWARE_REGISTERED_OWNER,
        0x1b00,
    );
    write_string_value(&mut data, 0x800, "ProductId", SOFTWARE_PRODUCT_ID, 0x1c00);
    write_dword_value(&mut data, 0x880, "InstallDate", SOFTWARE_INSTALL_DATE);
    data
}

fn empty_hive(root_name: &str) -> Vec<u8> {
    let mut data = vec![0u8; 0x8000];
    data[0..4].copy_from_slice(b"regf");
    data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
    data[0x1000..0x1004].copy_from_slice(b"hbin");
    data[0x1008..0x100c].copy_from_slice(&0x7000u32.to_le_bytes());
    write_nk(&mut data, 0x20, root_name, &[], &[]);
    data
}

fn write_nk(data: &mut [u8], offset: u32, name: &str, subkeys: &[(&str, u32)], values: &[u32]) {
    let abs = BASE_BLOCK_SIZE + offset as usize;
    let name_bytes = name.as_bytes();
    let subkey_list_offset = 0x2000 + offset;
    let value_list_offset = 0x4000 + offset;
    data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
    data[abs + 4..abs + 6].copy_from_slice(b"nk");
    data[abs + 6..abs + 8].copy_from_slice(&0x20u16.to_le_bytes());
    data[abs + 0x18..abs + 0x1c].copy_from_slice(&(subkeys.len() as u32).to_le_bytes());
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

    if !subkeys.is_empty() {
        write_lf(data, subkey_list_offset, subkeys);
    }
    if !values.is_empty() {
        let list_abs = BASE_BLOCK_SIZE + value_list_offset as usize;
        data[list_abs..list_abs + 4]
            .copy_from_slice(&(-((values.len() as i32 * 4) + 4)).to_le_bytes());
        for (index, value_offset) in values.iter().enumerate() {
            let entry = list_abs + 4 + index * 4;
            data[entry..entry + 4].copy_from_slice(&value_offset.to_le_bytes());
        }
    }
}

fn write_lf(data: &mut [u8], offset: u32, subkeys: &[(&str, u32)]) {
    let abs = BASE_BLOCK_SIZE + offset as usize;
    data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
    data[abs + 4..abs + 6].copy_from_slice(b"lf");
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

fn write_string_value(data: &mut [u8], offset: u32, name: &str, value: &str, data_offset: u32) {
    let encoded: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
    data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
    data[data_abs + 4..data_abs + 4 + encoded.len()].copy_from_slice(&encoded);
    write_vk(
        data,
        offset,
        name,
        REG_SZ,
        encoded.len() as u32,
        data_offset,
    );
}

fn write_dword_value(data: &mut [u8], offset: u32, name: &str, value: u32) {
    write_vk(data, offset, name, REG_DWORD, 0x8000_0004, value);
}

fn write_vk(
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

#[cfg(test)]
#[path = "../../tests/unit/builders/registry.rs"]
mod tests;
