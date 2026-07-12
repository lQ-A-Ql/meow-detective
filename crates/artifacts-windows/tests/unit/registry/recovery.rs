use super::constants::{BASE_BLOCK_SIZE, HBIN_HEADER_SIZE, HBIN_MAGIC, INVALID_OFFSET};
use super::*;

fn build_hive(payload: &[u8]) -> Vec<u8> {
    let hbin_size = (payload.len() + HBIN_HEADER_SIZE).div_ceil(0x1000) * 0x1000;
    let mut data = vec![0u8; BASE_BLOCK_SIZE + hbin_size];
    data[0..4].copy_from_slice(b"regf");
    data[BASE_BLOCK_SIZE..BASE_BLOCK_SIZE + 4].copy_from_slice(HBIN_MAGIC);
    data[BASE_BLOCK_SIZE + 8..BASE_BLOCK_SIZE + 12]
        .copy_from_slice(&(hbin_size as u32).to_le_bytes());
    data[BASE_BLOCK_SIZE + HBIN_HEADER_SIZE..BASE_BLOCK_SIZE + HBIN_HEADER_SIZE + payload.len()]
        .copy_from_slice(payload);
    data
}

fn push_cell(payload: &mut Vec<u8>, allocated: bool, body: &[u8]) {
    let size = (body.len() + 4).next_multiple_of(8);
    let signed_size = if allocated {
        -(size as i32)
    } else {
        size as i32
    };
    payload.extend_from_slice(&signed_size.to_le_bytes());
    payload.extend_from_slice(body);
    payload.resize(payload.len() + size - body.len() - 4, 0);
}

fn utf16(value: &str) -> Vec<u8> {
    value.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn nk_body(name: &str, compressed: bool, parent: u32, values: u32) -> Vec<u8> {
    let name_bytes = if compressed {
        name.as_bytes().to_vec()
    } else {
        utf16(name)
    };
    let mut body = vec![0u8; 0x4c + name_bytes.len()];
    body[0..2].copy_from_slice(b"nk");
    body[2..4].copy_from_slice(&(if compressed { 0x20u16 } else { 0 }).to_le_bytes());
    body[4..12].copy_from_slice(&133_600_000_000_000_000u64.to_le_bytes());
    body[12..16].copy_from_slice(&parent.to_le_bytes());
    body[0x24..0x28].copy_from_slice(&values.to_le_bytes());
    body[0x48..0x4a].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    body[0x4c..].copy_from_slice(&name_bytes);
    body
}

fn vk_body(name: &str, value_type: u32, inline_value: u32) -> Vec<u8> {
    let name_bytes = name.as_bytes();
    let mut body = vec![0u8; 0x14 + name_bytes.len()];
    body[0..2].copy_from_slice(b"vk");
    body[2..4].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    body[4..8].copy_from_slice(&0x8000_0004u32.to_le_bytes());
    body[8..12].copy_from_slice(&inline_value.to_le_bytes());
    body[12..16].copy_from_slice(&value_type.to_le_bytes());
    body[16..18].copy_from_slice(&1u16.to_le_bytes());
    body[0x14..].copy_from_slice(name_bytes);
    body
}

#[test]
fn detects_free_cells_only() {
    let mut payload = Vec::new();
    push_cell(
        &mut payload,
        false,
        &nk_body("Deleted", true, INVALID_OFFSET, 0),
    );
    push_cell(
        &mut payload,
        true,
        &nk_body("Live", true, INVALID_OFFSET, 0),
    );
    let cells = scan_free_cells(&build_hive(&payload));
    assert_eq!(cells.len(), 1);
    assert!(cells[0].offset >= BASE_BLOCK_SIZE + HBIN_HEADER_SIZE);
}

#[test]
fn recovers_deleted_key_metadata() {
    let mut payload = Vec::new();
    push_cell(
        &mut payload,
        false,
        &nk_body("DeletedKey", false, INVALID_OFFSET, 3),
    );
    let result = scan_deleted_registry_cells(&build_hive(&payload), "SYSTEM").unwrap();
    assert_eq!(result.free_cells_scanned, 1);
    assert_eq!(result.recovered_keys.len(), 1);
    let key = &result.recovered_keys[0];
    assert_eq!(key.key_name, "DeletedKey");
    assert_eq!(key.num_values, 3);
    assert_eq!(key.confidence, "high");
    assert!(key.last_written.is_some());
    assert_eq!(key.parent_path_hint, "(orphan)");
}

#[test]
fn recovers_deleted_value_preview() {
    let mut payload = Vec::new();
    push_cell(&mut payload, false, &vk_body("Flag", 4, 0x1122_3344));
    let result = scan_deleted_registry_cells(&build_hive(&payload), "SOFTWARE").unwrap();
    assert_eq!(result.recovered_values.len(), 1);
    let value = &result.recovered_values[0];
    assert_eq!(value.value_name, "Flag");
    assert_eq!(value.value_type, 4);
    assert!(value.value_data_preview.starts_with("44 33 22 11"));
    assert_eq!(value.confidence, "high");
}

#[test]
fn allocated_cells_and_unknown_signatures_are_skipped() {
    let mut payload = Vec::new();
    push_cell(
        &mut payload,
        true,
        &nk_body("Live", true, INVALID_OFFSET, 0),
    );
    push_cell(&mut payload, false, b"zz-not-a-record");
    let result = scan_deleted_registry_cells(&build_hive(&payload), "SYSTEM").unwrap();
    assert!(result.recovered_keys.is_empty());
    assert!(result.recovered_values.is_empty());
}

#[test]
fn malformed_names_lower_confidence() {
    let mut malformed = nk_body("", true, INVALID_OFFSET, 0);
    malformed[0x48..0x4a].copy_from_slice(&500u16.to_le_bytes());
    let mut payload = Vec::new();
    push_cell(&mut payload, false, &malformed);
    let result = scan_deleted_registry_cells(&build_hive(&payload), "SYSTEM").unwrap();
    assert_eq!(result.recovered_keys[0].confidence, "low");
}

#[test]
fn invalid_hives_are_rejected() {
    assert!(scan_deleted_registry_cells(b"short", "bad").is_err());
    let mut no_magic = vec![0u8; BASE_BLOCK_SIZE + 0x1000];
    no_magic[BASE_BLOCK_SIZE..BASE_BLOCK_SIZE + 4].copy_from_slice(HBIN_MAGIC);
    assert!(scan_deleted_registry_cells(&no_magic, "bad").is_err());
}

#[test]
fn recovers_multiple_deleted_records() {
    let mut payload = Vec::new();
    push_cell(
        &mut payload,
        false,
        &nk_body("Alpha", true, INVALID_OFFSET, 1),
    );
    push_cell(&mut payload, false, &vk_body("Value", 4, 7));
    push_cell(
        &mut payload,
        false,
        &nk_body("Beta", true, INVALID_OFFSET, 2),
    );
    let result = scan_deleted_registry_cells(&build_hive(&payload), "SYSTEM").unwrap();
    assert_eq!(result.recovered_keys.len(), 2);
    assert_eq!(result.recovered_values.len(), 1);
}

#[test]
fn walks_multiple_hbin_blocks() {
    let mut first_payload = Vec::new();
    push_cell(
        &mut first_payload,
        false,
        &nk_body("First", true, INVALID_OFFSET, 0),
    );
    let first = build_hive(&first_payload);
    let first_hbin_size = u32::from_le_bytes(
        first[BASE_BLOCK_SIZE + 8..BASE_BLOCK_SIZE + 12]
            .try_into()
            .unwrap(),
    ) as usize;
    let second_start = BASE_BLOCK_SIZE + first_hbin_size;
    let mut data = first;
    data.resize(second_start + 0x1000, 0);
    data[second_start..second_start + 4].copy_from_slice(HBIN_MAGIC);
    data[second_start + 8..second_start + 12].copy_from_slice(&0x1000u32.to_le_bytes());
    let second_body = nk_body("Second", true, INVALID_OFFSET, 0);
    let second_size = (second_body.len() + 4).next_multiple_of(8);
    data[second_start + HBIN_HEADER_SIZE..second_start + HBIN_HEADER_SIZE + 4]
        .copy_from_slice(&(second_size as i32).to_le_bytes());
    data[second_start + HBIN_HEADER_SIZE + 4
        ..second_start + HBIN_HEADER_SIZE + 4 + second_body.len()]
        .copy_from_slice(&second_body);
    let result = scan_deleted_registry_cells(&data, "SYSTEM").unwrap();
    assert_eq!(result.recovered_keys.len(), 2);
}

#[test]
fn resolves_allocated_parent_name() {
    let parent_body = nk_body("Parent", true, INVALID_OFFSET, 0);
    let parent_size = (parent_body.len() + 4).next_multiple_of(8);
    let child_parent_offset = HBIN_HEADER_SIZE as u32;
    let mut payload = Vec::new();
    push_cell(&mut payload, true, &parent_body);
    push_cell(
        &mut payload,
        false,
        &nk_body("Child", true, child_parent_offset, 0),
    );
    assert!(payload.len() >= parent_size);
    let result = scan_deleted_registry_cells(&build_hive(&payload), "SYSTEM").unwrap();
    assert_eq!(result.recovered_keys[0].parent_path_hint, "Parent");
}
