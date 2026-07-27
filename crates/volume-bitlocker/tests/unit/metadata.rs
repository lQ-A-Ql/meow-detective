use super::*;

/// Encodes one metadata entry with its 8-byte header.
fn entry(entry_type: u16, value_type: u16, data: &[u8]) -> Vec<u8> {
    let size = (8 + data.len()) as u16;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&entry_type.to_le_bytes());
    out.extend_from_slice(&value_type.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// A VMK entry carrying only a protector code, enough to appear in the inventory.
fn vmk_entry(code: u16) -> Vec<u8> {
    let mut data = vec![0u8; 28];
    data[26..28].copy_from_slice(&code.to_le_bytes());
    entry(ENTRY_TYPE_VMK, VALUE_TYPE_VMK, &data)
}

/// The relocated volume-header descriptor entry.
fn volume_header_entry(offset: u64, size: u64) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(&size.to_le_bytes());
    entry(ENTRY_TYPE_VOLUME_HEADER, ENTRY_TYPE_VOLUME_HEADER, &data)
}

/// How a synthetic metadata block should be built.
struct BlockSpec<'a> {
    method: u16,
    entries: &'a [u8],
    with_signature: bool,
    /// Overrides the metadata-size field; `None` uses the true size.
    metadata_size: Option<u32>,
    /// Volume-header sector count in the block header.
    volume_header_sectors: u32,
    /// Volume-header offset in the block header.
    block_volume_header_offset: u64,
}

impl Default for BlockSpec<'_> {
    fn default() -> Self {
        Self {
            method: 0x8000,
            entries: &[],
            with_signature: true,
            metadata_size: None,
            volume_header_sectors: 1,
            block_volume_header_offset: 0x9000,
        }
    }
}

/// Builds a bare FVE metadata block, with no surrounding volume.
///
/// This module tests the block parser, so its fixture is a block — not a whole
/// image. Whole-volume synthesis lives in the unlock tests, which need it.
fn build_block(spec: &BlockSpec<'_>) -> Vec<u8> {
    let metadata_size = spec
        .metadata_size
        .unwrap_or((48 + spec.entries.len()) as u32);
    let mut block = vec![0u8; 64 + 48 + spec.entries.len() + 64];
    if spec.with_signature {
        block[0..8].copy_from_slice(b"-FVE-FS-");
    }
    block[10..12].copy_from_slice(&2u16.to_le_bytes());
    block[28..32].copy_from_slice(&spec.volume_header_sectors.to_le_bytes());
    block[32..40].copy_from_slice(&0x1000u64.to_le_bytes());
    block[40..48].copy_from_slice(&0x2000u64.to_le_bytes());
    block[48..56].copy_from_slice(&0x3000u64.to_le_bytes());
    block[56..64].copy_from_slice(&spec.block_volume_header_offset.to_le_bytes());

    let header = 64usize;
    block[header..header + 4].copy_from_slice(&metadata_size.to_le_bytes());
    block[header + 16..header + 32].copy_from_slice(&[0xAB; 16]);
    block[header + 36..header + 38].copy_from_slice(&spec.method.to_le_bytes());
    block[header + 40..header + 48].copy_from_slice(&0x01D9_0000_0000_0000u64.to_le_bytes());
    block[header + 48..header + 48 + spec.entries.len()].copy_from_slice(spec.entries);
    block
}

#[test]
fn parse_sequence_reads_consecutive_entries() {
    let mut data = Vec::new();
    data.extend_from_slice(&entry(0x0002, 0x0008, b"first"));
    data.extend_from_slice(&entry(0x0003, 0x0005, b"second-value"));

    let entries = MetadataEntry::parse_sequence(&data);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].entry_type, 0x0002);
    assert_eq!(entries[0].value_type, 0x0008);
    assert_eq!(entries[0].data, b"first");
    assert_eq!(entries[1].data, b"second-value");
    assert_eq!(entries[1].version, 1);
}

#[test]
fn parse_sequence_stops_on_a_size_below_the_header() {
    // A zero or undersized size field would otherwise loop forever on untrusted
    // input. One valid entry then a zero size must yield exactly one entry.
    let mut data = entry(0x0002, 0x0008, b"ok");
    data.extend_from_slice(&[0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    let entries = MetadataEntry::parse_sequence(&data);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].data, b"ok");
}

#[test]
fn parse_sequence_stops_on_a_size_past_the_buffer() {
    let mut data = Vec::new();
    // Claims 4096 bytes but only 8 are present.
    data.extend_from_slice(&4096u16.to_le_bytes());
    data.extend_from_slice(&0x0002u16.to_le_bytes());
    data.extend_from_slice(&0x0008u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    assert!(MetadataEntry::parse_sequence(&data).is_empty());
}

#[test]
fn parse_sequence_tolerates_a_truncated_tail() {
    let mut data = entry(0x0002, 0x0008, b"complete");
    data.extend_from_slice(&[0x10, 0x00, 0x02]); // 3 bytes, below the 8-byte header
    assert_eq!(MetadataEntry::parse_sequence(&data).len(), 1);
}

#[test]
fn nested_parses_entries_after_an_offset() {
    let inner = entry(0x0000, 0x0003, b"salt-ish");
    let mut data = vec![0u8; 28];
    data.extend_from_slice(&inner);
    let outer = MetadataEntry {
        entry_type: 0x0002,
        value_type: 0x0008,
        version: 1,
        data,
    };
    let nested = outer.nested(28);
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].value_type, 0x0003);
}

#[test]
fn nested_clamps_an_offset_past_the_value() {
    let outer = MetadataEntry {
        entry_type: 0x0002,
        value_type: 0x0008,
        version: 1,
        data: vec![0u8; 4],
    };
    assert!(outer.nested(1024).is_empty());
}

#[test]
fn protection_code_is_only_reported_for_vmk_entries() {
    let mut data = vec![0u8; 28];
    data[26..28].copy_from_slice(&0x2000u16.to_le_bytes());

    let vmk = MetadataEntry {
        entry_type: ENTRY_TYPE_VMK,
        value_type: VALUE_TYPE_VMK,
        version: 1,
        data: data.clone(),
    };
    assert_eq!(vmk.protection_code(), Some(0x2000));

    let fvek = MetadataEntry {
        entry_type: ENTRY_TYPE_FVEK,
        value_type: VALUE_TYPE_AES_CCM,
        version: 1,
        data,
    };
    assert_eq!(
        fvek.protection_code(),
        None,
        "offset 26 is only a protector code inside a VMK entry"
    );
}

#[test]
fn parse_requires_the_block_signature() {
    // This is what separates a real BitLocker To Go volume from plain FAT: both
    // carry MSWIN4.1 in the boot sector, only the encrypted one has this block.
    let block = build_block(&BlockSpec {
        with_signature: false,
        ..BlockSpec::default()
    });
    assert!(FveMetadata::parse(&block, 512).is_none());
}

#[test]
fn parse_reads_the_metadata_header_fields() {
    let block = build_block(&BlockSpec {
        method: 0x8004,
        ..BlockSpec::default()
    });
    let metadata = FveMetadata::parse(&block, 512).expect("valid block");
    assert_eq!(metadata.encryption_method_code, 0x8004);
    assert_eq!(metadata.encryption_method, EncryptionMethod::XtsAes128);
    assert_eq!(metadata.volume_guid, [0xAB; 16]);
    assert_eq!(metadata.creation_time, 0x01D9_0000_0000_0000);
    assert_eq!(metadata.metadata_offsets, [0x1000, 0x2000, 0x3000]);
}

#[test]
fn parse_preserves_an_unknown_method_code() {
    let block = build_block(&BlockSpec {
        method: 0x8009,
        ..BlockSpec::default()
    });
    let metadata = FveMetadata::parse(&block, 512).expect("valid block");
    assert_eq!(metadata.encryption_method_code, 0x8009);
    assert_eq!(
        metadata.encryption_method,
        EncryptionMethod::Unknown(0x8009)
    );
}

#[test]
fn protector_inventory_lists_every_protector_in_order() {
    let mut entries = Vec::new();
    for code in [0x2000u16, 0x0100, 0x0200] {
        entries.extend_from_slice(&vmk_entry(code));
    }
    let block = build_block(&BlockSpec {
        entries: &entries,
        ..BlockSpec::default()
    });
    let metadata = FveMetadata::parse(&block, 512).expect("valid block");
    assert_eq!(
        metadata.protector_inventory().protectors(),
        &[
            ProtectorKind::Password,
            ProtectorKind::Tpm,
            ProtectorKind::StartupKey
        ]
    );
    assert_eq!(metadata.protector_codes(), vec![0x2000, 0x0100, 0x0200]);
}

#[test]
fn protector_inventory_reports_a_tpm_only_volume_as_unusable() {
    // The forensically important case: the inventory is still produced, and it
    // says plainly that nothing here can unlock the volume.
    let mut entries = Vec::new();
    for code in [0x0100u16, 0x0200] {
        entries.extend_from_slice(&vmk_entry(code));
    }
    let block = build_block(&BlockSpec {
        entries: &entries,
        ..BlockSpec::default()
    });
    let metadata = FveMetadata::parse(&block, 512).expect("valid block");
    let inventory = metadata.protector_inventory();
    assert!(!inventory.is_empty());
    assert!(!inventory.has_unlockable_protector());
}

#[test]
fn a_block_with_no_protectors_yields_an_empty_inventory() {
    // Distinguishable from "unprotected": an empty inventory means the metadata
    // parsed but carried no protector entries, a malformed-volume signal.
    let metadata = FveMetadata::parse(&build_block(&BlockSpec::default()), 512).expect("valid");
    assert!(metadata.protector_inventory().is_empty());
    assert!(metadata.fvek_entry().is_none());
}

#[test]
fn classify_protector_maps_both_tpm_codes() {
    assert_eq!(classify_protector(0x0100), ProtectorKind::Tpm);
    assert_eq!(
        classify_protector(0x0400),
        ProtectorKind::Tpm,
        "TPM+PIN is still a TPM protector for inventory purposes"
    );
    assert_eq!(classify_protector(0x0000), ProtectorKind::ClearKey);
    assert_eq!(classify_protector(0x0200), ProtectorKind::StartupKey);
    assert_eq!(classify_protector(0x0800), ProtectorKind::RecoveryPassword);
    assert_eq!(classify_protector(0x2000), ProtectorKind::Password);
    assert_eq!(classify_protector(0x1234), ProtectorKind::Unknown(0x1234));
}

#[test]
fn the_volume_header_descriptor_overrides_the_block_header_fields() {
    // When the block header and the descriptor entry disagree, the descriptor
    // wins. Getting this backwards would point the reader at the wrong sector for
    // the relocated original boot sector.
    let entries = volume_header_entry(0x3000, 512);
    let block = build_block(&BlockSpec {
        entries: &entries,
        volume_header_sectors: 8,
        block_volume_header_offset: 0x9000,
        ..BlockSpec::default()
    });
    let metadata = FveMetadata::parse(&block, 512).expect("valid block");
    assert_eq!(metadata.volume_header_offset, 0x3000);
    assert_eq!(
        metadata.volume_header_size, 512,
        "the descriptor's explicit size wins over sectors x bytes-per-sector"
    );
}

#[test]
fn block_header_fields_are_used_when_there_is_no_descriptor() {
    let block = build_block(&BlockSpec {
        volume_header_sectors: 8,
        block_volume_header_offset: 0x9000,
        ..BlockSpec::default()
    });
    let metadata = FveMetadata::parse(&block, 512).expect("valid block");
    assert_eq!(metadata.volume_header_offset, 0x9000);
    assert_eq!(metadata.volume_header_size, 8 * 512);
}

#[test]
fn a_zero_valued_descriptor_field_does_not_override() {
    // A descriptor present but zeroed must not blank the block-header fallback.
    let entries = volume_header_entry(0, 0);
    let block = build_block(&BlockSpec {
        entries: &entries,
        volume_header_sectors: 4,
        block_volume_header_offset: 0x9000,
        ..BlockSpec::default()
    });
    let metadata = FveMetadata::parse(&block, 512).expect("valid block");
    assert_eq!(metadata.volume_header_offset, 0x9000);
    assert_eq!(metadata.volume_header_size, 4 * 512);
}

#[test]
fn fvek_and_vmk_entries_are_located_by_type() {
    let mut entries = vmk_entry(0x2000);
    entries.extend_from_slice(&entry(ENTRY_TYPE_FVEK, VALUE_TYPE_AES_CCM, &[0u8; 40]));
    let block = build_block(&BlockSpec {
        entries: &entries,
        ..BlockSpec::default()
    });
    let metadata = FveMetadata::parse(&block, 512).expect("valid block");
    assert!(metadata.fvek_entry().is_some());
    assert_eq!(metadata.vmk_entries().count(), 1);
}

#[test]
fn a_truncated_block_parses_without_panicking() {
    let mut entries = vmk_entry(0x2000);
    entries.extend_from_slice(&entry(ENTRY_TYPE_FVEK, VALUE_TYPE_AES_CCM, &[0u8; 40]));
    let full = build_block(&BlockSpec {
        entries: &entries,
        ..BlockSpec::default()
    });
    // Every prefix must fail cleanly or yield a bounded parse. An evidence image
    // can be truncated anywhere, so no length may reach an index panic.
    for len in 0..full.len() {
        let _ = FveMetadata::parse(&full[..len], 512);
    }
}

#[test]
fn an_oversized_metadata_size_is_rejected() {
    let entries = vmk_entry(0x2000);
    let block = build_block(&BlockSpec {
        entries: &entries,
        metadata_size: Some(u32::MAX),
        ..BlockSpec::default()
    });
    assert!(FveMetadata::parse(&block, 512).is_none());
}

#[test]
fn an_undersized_metadata_size_is_rejected() {
    let entries = vmk_entry(0x2000);
    let block = build_block(&BlockSpec {
        entries: &entries,
        metadata_size: Some(8),
        ..BlockSpec::default()
    });
    assert!(FveMetadata::parse(&block, 512).is_none());
}

#[test]
fn a_corrupt_top_level_entry_tail_rejects_the_copy() {
    let mut entries = vmk_entry(0x2000);
    entries.extend_from_slice(&[0x10, 0x00, 0x02]);
    let block = build_block(&BlockSpec {
        entries: &entries,
        ..BlockSpec::default()
    });
    assert!(FveMetadata::parse(&block, 512).is_none());
}

#[test]
fn only_v2_metadata_blocks_are_accepted() {
    let mut block = build_block(&BlockSpec::default());
    block[10..12].copy_from_slice(&1u16.to_le_bytes());
    assert!(FveMetadata::parse(&block, 512).is_none());
}

#[test]
fn unreasonable_sector_sizes_are_rejected() {
    let block = build_block(&BlockSpec::default());
    assert!(FveMetadata::parse(&block, 256).is_none());
    assert!(FveMetadata::parse(&block, 8192).is_none());
}
