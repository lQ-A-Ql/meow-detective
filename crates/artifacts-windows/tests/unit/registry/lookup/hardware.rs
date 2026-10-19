use super::*;
use crate::registry::lookup::types::REG_QWORD;
use crate::registry::tests::*;

fn structure(kind: u8, length: usize, handle: u16, strings: &[&str]) -> Vec<u8> {
    let mut data = vec![0; length];
    data[0] = kind;
    data[1] = length as u8;
    data[2..4].copy_from_slice(&handle.to_le_bytes());
    for value in strings {
        data.extend_from_slice(value.as_bytes());
        data.push(0);
    }
    if strings.is_empty() {
        data.push(0);
    }
    data.push(0);
    data
}

fn raw_table(records: &[Vec<u8>]) -> Vec<u8> {
    let mut table = records.concat();
    table.extend(structure(127, 4, 0xffff, &[]));
    let mut raw = vec![0, 3, 8, 0];
    raw.extend_from_slice(&(table.len() as u32).to_le_bytes());
    raw.extend(table);
    raw
}

fn cpu() -> Vec<u8> {
    let mut cpu = structure(4, 0x30, 0x40, &["CPU0", "GenuineIntel", "Fixture CPU"]);
    cpu[4] = 1;
    cpu[5] = 3;
    cpu[7] = 2;
    cpu[0x10] = 3;
    cpu[0x18] = 0x41;
    cpu[0x14..0x16].copy_from_slice(&4200u16.to_le_bytes());
    cpu[0x16..0x18].copy_from_slice(&3200u16.to_le_bytes());
    cpu[0x23] = 8;
    cpu[0x24] = 8;
    cpu[0x25] = 16;
    cpu
}

fn ram(size: u16, handle: u16) -> Vec<u8> {
    let mut ram = structure(
        17,
        0x5c,
        handle,
        &["DIMM_A1", "BANK 0", "Fixture Memory", "MEM-001", "PART-64G"],
    );
    ram[0x0c..0x0e].copy_from_slice(&size.to_le_bytes());
    ram[0x0e] = 9;
    ram[0x10] = 1;
    ram[0x11] = 2;
    ram[0x12] = 0x22;
    ram[0x17] = 3;
    ram[0x18] = 4;
    ram[0x1a] = 5;
    ram[0x1c..0x20].copy_from_slice(&65536u32.to_le_bytes());
    ram[0x15..0x17].copy_from_slice(&u16::MAX.to_le_bytes());
    ram[0x54..0x58].copy_from_slice(&6400u32.to_le_bytes());
    ram[0x20..0x22].copy_from_slice(&6000u16.to_le_bytes());
    ram
}

#[test]
fn smbios_hardware_preserves_cpu_memory_identity_and_source_offsets() {
    let mut board = structure(
        2,
        8,
        0x20,
        &["Fixture Vendor", "Fixture Board", "1.0", "BOARD-001"],
    );
    board[4] = 1;
    board[5] = 2;
    board[6] = 3;
    board[7] = 4;
    let raw = raw_table(&[cpu(), ram(0x7fff, 0x50), board]);
    let records = smbios::parse(&raw, "ControlSet002\\Services\\mssmbios\\Data").unwrap();
    let processor = records.iter().find(|r| r.kind == "cpu").unwrap();
    assert_eq!(processor.name, "Fixture CPU");
    assert_eq!(processor.properties["coreCount"], "8");
    assert_eq!(processor.properties["threadCount"], "16");
    assert_eq!(processor.source_offset, Some(8));
    assert_eq!(processor.value_name.as_deref(), Some("SMBiosData"));
    let memory = records.iter().find(|r| r.kind == "ram").unwrap();
    assert_eq!(memory.properties["capacityBytes"], "68719476736");
    assert_eq!(memory.properties["memoryType"], "DDR5");
    assert_eq!(memory.properties["speedMtS"], "6400");
    assert_eq!(memory.properties["configuredSpeedMtS"], "6000");
    assert_eq!(memory.properties["serialNumber"], "MEM-001");
    let board = records.iter().find(|r| r.kind == "baseboard").unwrap();
    assert_eq!(board.name, "Fixture Board");
    assert!(records
        .iter()
        .all(|r| r.key_path.starts_with("ControlSet002\\")));
}

#[test]
fn smbios_rejects_truncated_headers_lengths_and_unterminated_strings() {
    assert!(smbios::parse(&[0; 7], "Data").is_err());
    let raw = raw_table(&[cpu()]);
    let mut too_long = raw.clone();
    too_long[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(smbios::parse(&too_long, "Data").is_err());
    let mut short_record = raw.clone();
    short_record[9] = 3;
    assert!(smbios::parse(&short_record, "Data").is_err());
    let mut unterminated = vec![0, 3, 8, 0, 5, 0, 0, 0, 2, 4, 0, 0, b'X'];
    assert!(smbios::parse(&unterminated, "Data").is_err());
    unterminated[4] = 0;
    assert!(smbios::parse(&unterminated, "Data").unwrap().is_empty());
}

#[test]
fn memory_unknown_capacity_is_not_zero_and_empty_or_logical_slots_are_excluded() {
    let mut logical = ram(4096, 0x53);
    logical[0x12] = 0x1f;
    let raw = raw_table(&[ram(0xffff, 0x50), ram(0, 0x51), ram(0x8001, 0x52), logical]);
    let records = smbios::parse(&raw, "Data").unwrap();
    assert_eq!(records.len(), 2);
    assert!(!records[0].properties.contains_key("capacityBytes"));
    assert_eq!(records[1].properties["capacityBytes"], "1024");
    let mut extended = cpu();
    extended[0x23] = 255;
    extended[0x25] = 255;
    extended[0x2a..0x2c].copy_from_slice(&512u16.to_le_bytes());
    extended[0x2e..0x30].copy_from_slice(&1024u16.to_le_bytes());
    let records = smbios::parse(&raw_table(&[extended]), "Data").unwrap();
    assert_eq!(records[0].properties["coreCount"], "512");
    assert_eq!(records[0].properties["threadCount"], "1024");
}

#[test]
fn system_hardware_uses_selected_control_set_and_keeps_64_bit_gpu_capacity() {
    let mut data = empty_hive("SYSTEM");
    write_nk(
        &mut data,
        0x20,
        "SYSTEM",
        &[
            ("Select", 0x200),
            ("ControlSet001", 0x400),
            ("ControlSet002", 0x600),
        ],
        &[],
    );
    write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
    write_dword_value(&mut data, 0x1200, "Current", 2);
    write_nk(&mut data, 0x400, "ControlSet001", &[], &[]);
    write_nk(
        &mut data,
        0x600,
        "ControlSet002",
        &[("Control", 0x800)],
        &[],
    );
    write_nk(&mut data, 0x800, "Control", &[("Class", 0xa00)], &[]);
    write_nk(
        &mut data,
        0xa00,
        "Class",
        &[("{4D36E968-E325-11CE-BFC1-08002BE10318}", 0xc00)],
        &[],
    );
    write_nk(
        &mut data,
        0xc00,
        "{4D36E968-E325-11CE-BFC1-08002BE10318}",
        &[("0000", 0xe00)],
        &[],
    );
    write_nk(&mut data, 0xe00, "0000", &[], &[0x1300, 0x1400]);
    write_string_value(&mut data, 0x1300, "DriverDesc", "Fixture GPU", 0x1800);
    write_vk(
        &mut data,
        0x1400,
        "HardwareInformation.qwMemorySize",
        REG_QWORD,
        8,
        0x1900,
    );
    let absolute = 0x1000 + 0x1900;
    data[absolute..absolute + 4].copy_from_slice(&(-16i32).to_le_bytes());
    data[absolute + 4..absolute + 12].copy_from_slice(&(12u64 * 1024 * 1024 * 1024).to_le_bytes());
    let info = extract_system_hardware(&data, |_| Ok(None)).unwrap();
    assert_eq!(info.devices.len(), 1);
    assert_eq!(info.devices[0].name, "Fixture GPU");
    assert_eq!(info.devices[0].properties["capacityBytes"], "12884901888");
    assert!(info.devices[0].key_path.starts_with("ControlSet002\\"));
    assert!(info.warnings.is_empty());
}
