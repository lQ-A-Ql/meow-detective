use super::*;

const FILE_ATTRIBUTE_ENCRYPTED: u32 = 0x0000_4000;

#[test]
fn deleted_record_propagates_standard_information_efs_flag() {
    let record = deleted_file_record(FILE_ATTRIBUTE_ENCRYPTED, 0);
    let recovered = parse_deleted_record(&record);

    assert!(recovered.encrypted);
    assert_eq!(recovered.name, "deleted-secret.txt");
}

#[test]
fn deleted_record_propagates_file_name_efs_flag() {
    let record = deleted_file_record(0, FILE_ATTRIBUTE_ENCRYPTED);
    let recovered = parse_deleted_record(&record);

    assert!(recovered.encrypted);
}

#[test]
fn deleted_record_without_efs_flags_remains_clear() {
    let record = deleted_file_record(0, 0);
    let recovered = parse_deleted_record(&record);

    assert!(!recovered.encrypted);
}

fn parse_deleted_record(record: &[u8]) -> NtfsDeletedFileRecord {
    let mut parser = MftRecordParser::new(1024, 512);
    deleted_record(&mut parser, record, 42, 4096, 1024, 4096)
        .unwrap()
        .unwrap()
}

fn deleted_file_record(si_flags: u32, file_name_flags: u32) -> Vec<u8> {
    let mut record = vec![0u8; 1024];
    record[0..4].copy_from_slice(b"FILE");
    record[0x10..0x12].copy_from_slice(&7u16.to_le_bytes());
    record[0x14..0x16].copy_from_slice(&56u16.to_le_bytes());
    record[0x16..0x18].copy_from_slice(&0u16.to_le_bytes());

    let mut position = 56usize;
    let standard_information_length = 0x60usize;
    record[position..position + 4].copy_from_slice(&0x10u32.to_le_bytes());
    record[position + 4..position + 8]
        .copy_from_slice(&(standard_information_length as u32).to_le_bytes());
    record[position + 0x10..position + 0x14].copy_from_slice(&0x30u32.to_le_bytes());
    record[position + 0x14..position + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
    record[position + 0x18 + 0x20..position + 0x18 + 0x24].copy_from_slice(&si_flags.to_le_bytes());
    position += standard_information_length;

    let name = "deleted-secret.txt";
    let name_units = name.encode_utf16().collect::<Vec<_>>();
    let file_name_content_length = 0x42 + name_units.len() * 2;
    let file_name_attribute_length = 0x18 + file_name_content_length;
    record[position..position + 4].copy_from_slice(&0x30u32.to_le_bytes());
    record[position + 4..position + 8]
        .copy_from_slice(&(file_name_attribute_length as u32).to_le_bytes());
    record[position + 0x10..position + 0x14]
        .copy_from_slice(&(file_name_content_length as u32).to_le_bytes());
    record[position + 0x14..position + 0x16].copy_from_slice(&0x18u16.to_le_bytes());
    let content = position + 0x18;
    record[content..content + 8].copy_from_slice(&5u64.to_le_bytes());
    record[content + 0x30..content + 0x38].copy_from_slice(&16u64.to_le_bytes());
    record[content + 0x38..content + 0x3c].copy_from_slice(&file_name_flags.to_le_bytes());
    record[content + 0x40] = name_units.len() as u8;
    record[content + 0x41] = 1;
    for (index, unit) in name_units.into_iter().enumerate() {
        let offset = content + 0x42 + index * 2;
        record[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
    }
    position += file_name_attribute_length;
    record[position..position + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    record
}
