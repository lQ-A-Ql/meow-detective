use super::{
    keyring::parse_matching_vmk,
    profile::{FveVolumeContextLayout, KeyringLayout},
    volume_context::parse_profiled_vmk_datum,
};
use crate::MemoryWindowsError;

const CAPACITY: usize = 0x4000;
const GUID: [u8; 16] = [0xAB; 16];

fn layout() -> KeyringLayout {
    KeyringLayout {
        client_keyring_offset: 0x278,
        capacity: CAPACITY as u32,
        header_size: 0x20,
        dataset_minimum_size: 0x30,
        dataset_volume_guid_offset: 0x10,
    }
}

fn exact_keyring() -> Vec<u8> {
    let mut bytes = vec![0u8; CAPACITY];
    bytes[0..8].copy_from_slice(b"-FVE-FS-");
    write_u32(&mut bytes, 8, CAPACITY as u32);
    write_u32(&mut bytes, 12, 1);
    write_u32(&mut bytes, 16, 0x20);
    write_u32(&mut bytes, 20, 0x80);

    let dataset = 0x20;
    write_u32(&mut bytes, dataset, 0x60);
    write_u32(&mut bytes, dataset + 4, 1);
    write_u32(&mut bytes, dataset + 8, 0x30);
    write_u32(&mut bytes, dataset + 12, 0x5C);
    bytes[dataset + 0x10..dataset + 0x20].copy_from_slice(&GUID);

    let datum = dataset + 0x30;
    write_u16(&mut bytes, datum, 44);
    write_u16(&mut bytes, datum + 2, 5);
    write_u16(&mut bytes, datum + 4, 1);
    write_u16(&mut bytes, datum + 6, 1);
    write_u16(&mut bytes, datum + 8, 0x2003);
    bytes[datum + 12..datum + 44].fill(0x44);
    bytes
}

fn volume_context_layout() -> FveVolumeContextLayout {
    FveVolumeContextLayout {
        vmk_datum_pointer_offset: 0x3D0,
        vmk_datum_size: 44,
        vmk_datum_entry_type: 0,
        vmk_datum_value_type: 1,
        vmk_datum_version: 0,
        vmk_datum_algorithm: 0x2003,
        vmk_offset: 12,
    }
}

#[test]
fn exact_volume_dataset_yields_one_opaque_vmk() {
    let parsed = parse_matching_vmk(&exact_keyring(), GUID, layout())
        .expect("exact keyring must yield an opaque VMK");
    assert_eq!(parsed.datasets_examined, 1);
    drop(parsed.vmk);
}

#[test]
fn wrong_volume_and_near_match_datum_fail_closed() {
    let missing = recovery_error(parse_matching_vmk(&exact_keyring(), [0xCD; 16], layout()));
    assert!(matches!(
        missing,
        MemoryWindowsError::BitLockerVolumeDatasetNotFound
    ));

    let mut wrong_algorithm = exact_keyring();
    write_u16(&mut wrong_algorithm, 0x20 + 0x30 + 8, 0x2002);
    let error = recovery_error(parse_matching_vmk(&wrong_algorithm, GUID, layout()));
    assert!(matches!(
        error,
        MemoryWindowsError::BitLockerVmkDatumNotFound
    ));
}

#[test]
fn duplicate_volume_dataset_is_rejected() {
    let mut bytes = exact_keyring();
    bytes.copy_within(0x20..0x80, 0x80);
    write_u32(&mut bytes, 20, 0xE0);
    let error = recovery_error(parse_matching_vmk(&bytes, GUID, layout()));
    assert!(matches!(
        error,
        MemoryWindowsError::AmbiguousBitLockerVolumeDataset
    ));
}

#[test]
fn exact_profiled_volume_context_datum_is_accepted() {
    let mut datum = [0u8; 44];
    write_u16(&mut datum, 0, 44);
    write_u16(&mut datum, 4, 1);
    write_u16(&mut datum, 8, 0x2003);
    datum[12..].fill(0xA5);

    let vmk = parse_profiled_vmk_datum(&datum, volume_context_layout())
        .expect("the reviewed DCB datum must match exactly");
    assert_eq!(vmk, [0xA5; 32]);
}

#[test]
fn keyring_or_near_match_datum_is_not_a_volume_context_datum() {
    let mut datum = [0u8; 44];
    write_u16(&mut datum, 0, 44);
    write_u16(&mut datum, 2, 5);
    write_u16(&mut datum, 4, 1);
    write_u16(&mut datum, 6, 1);
    write_u16(&mut datum, 8, 0x2003);
    assert!(parse_profiled_vmk_datum(&datum, volume_context_layout()).is_none());

    write_u16(&mut datum, 2, 0);
    write_u16(&mut datum, 6, 0);
    write_u16(&mut datum, 8, 0x2002);
    assert!(parse_profiled_vmk_datum(&datum, volume_context_layout()).is_none());
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn recovery_error(result: crate::Result<super::keyring::ParsedKeyringVmk>) -> MemoryWindowsError {
    match result {
        Ok(_) => panic!("keyring recovery unexpectedly succeeded"),
        Err(error) => error,
    }
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn symbol_registry_resolves_the_26100_layouts_from_pdb_facts() {
    let layouts =
        super::symbol_table::resolve_ntoskrnl_layouts("953A8DE8-80B0-818C-32DA-2DEC1D79C2D9")
            .expect("26100 build must be in the embedded registry");

    assert_eq!(layouts.objects.root_directory_object_rva, 0x00F0_DFF0);
    assert_eq!(layouts.objects.info_mask_to_offset_rva, 0x00F0_E100);
    assert_eq!(layouts.objects.object_header_body_offset, 0x30);
    assert_eq!(layouts.objects.object_header_info_mask_offset, 0x1A);
    assert_eq!(layouts.objects.unicode_buffer_offset, 8);
    let driver = super::symbol_table::default_driver_layout();
    assert_eq!(driver.device_object_offset, 0x08);
    assert_eq!(driver.driver_start_offset, 0x18);
    assert_eq!(driver.driver_extension_offset, 0x30);
    assert_eq!(driver.driver_name_offset, 0x38);
    assert_eq!(driver.extension_client_list_offset, 0x28);
    let devices = super::symbol_table::default_device_layout();
    assert_eq!(devices.device_extension_offset, 0x40);
    assert_eq!(devices.driver_object_offset, 0x08);
    assert_eq!(devices.next_device_offset, 0x10);
    let module = super::symbol_table::default_module_layout();
    assert_eq!(module.dll_base_offset, 0x30);
    assert_eq!(module.size_of_image_offset, 0x40);
    assert_eq!(module.name_length_offset, 0x58);
    assert_eq!(module.name_buffer_offset, 0x60);
}

#[test]
fn symbol_registry_fails_closed_for_unknown_builds() {
    assert!(
        super::symbol_table::resolve_ntoskrnl_layouts("00000000-0000-0000-0000-000000000000")
            .is_none()
    );
}
