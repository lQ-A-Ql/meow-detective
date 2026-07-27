use super::*;

use crate::metadata::FveMetadata;
use crate::method::EncryptionMethod;

/// Builds metadata carrying only the fields the layout reads.
///
/// Constructed directly rather than parsed: the layout is pure address
/// arithmetic, so each test varies exactly one input.
fn metadata(
    method_code: u16,
    metadata_offsets: [u64; 3],
    metadata_size: u32,
    volume_header_offset: u64,
    volume_header_size: u64,
    encrypted_volume_size: u64,
) -> FveMetadata {
    FveMetadata {
        encryption_method: EncryptionMethod::from_code(method_code),
        encryption_method_code: method_code,
        volume_guid: [0xAB; 16],
        creation_time: 0,
        entries: Vec::new(),
        encrypted_volume_size,
        volume_header_offset,
        volume_header_size,
        metadata_offsets,
        metadata_size,
    }
}

/// A whole-volume-encrypted 0x8000 layout: the first 0x2000 bytes relocated to
/// 0x9000, one metadata block at 0x4000 spanning 0x800 bytes.
///
/// The metadata block sits past the relocated-header region, as it does on a real
/// volume. `blanking_takes_precedence_over_relocation` covers the overlap case.
fn standard() -> VolumeLayout {
    VolumeLayout::from_metadata(&metadata(0x8000, [0x4000, 0, 0], 0x800, 0x9000, 0x2000, 0))
}

#[test]
fn a_normal_sector_maps_to_itself_and_is_encrypted() {
    assert_eq!(
        standard().resolve(0x5000),
        SectorSource::Encrypted {
            physical_offset: 0x5000
        }
    );
}

#[test]
fn the_relocated_header_reads_from_its_physical_home() {
    // Logical 0 lives at volume_header_offset, and the physical location is also
    // the cipher's address. Using the logical offset as the IV here decrypts to
    // garbage with no error, which is why the source carries the physical value.
    let layout = standard();
    assert_eq!(
        layout.resolve(0),
        SectorSource::Encrypted {
            physical_offset: 0x9000
        }
    );
    assert_eq!(
        layout.resolve(0x400),
        SectorSource::Encrypted {
            physical_offset: 0x9400
        }
    );
}

#[test]
fn the_relocation_stops_at_the_header_size() {
    // The last relocated sector and the first non-relocated one must land in
    // different places; an off-by-one here silently shifts a whole sector.
    let layout = standard();
    assert_eq!(
        layout.resolve(0x2000 - 512),
        SectorSource::Encrypted {
            physical_offset: 0x9000 + 0x2000 - 512
        }
    );
    assert_eq!(
        layout.resolve(0x2000),
        SectorSource::Encrypted {
            physical_offset: 0x2000
        }
    );
}

#[test]
fn a_zero_sized_header_region_relocates_nothing() {
    let layout =
        VolumeLayout::from_metadata(&metadata(0x8000, [0x1000, 0, 0], 0x800, 0x9000, 0, 0));
    assert_eq!(
        layout.resolve(0),
        SectorSource::Encrypted { physical_offset: 0 }
    );
    assert_eq!(layout.volume_header_size(), 0);
}

#[test]
fn metadata_regions_read_back_as_zeros() {
    // The FVE blocks are not filesystem content. Returning decrypted metadata
    // there would put key-protector bytes into the plaintext view.
    let layout = standard();
    assert_eq!(layout.resolve(0x4000), SectorSource::Blanked);
    assert_eq!(layout.resolve(0x4000 + 512), SectorSource::Blanked);
}

#[test]
fn the_blanked_region_ends_where_the_block_does() {
    let layout = standard();
    // 0x4000 + 0x800 = 0x4800 is the first sector past the block.
    assert_eq!(layout.resolve(0x4800 - 512), SectorSource::Blanked);
    assert_eq!(
        layout.resolve(0x4800),
        SectorSource::Encrypted {
            physical_offset: 0x4800
        }
    );
}

#[test]
fn blanking_takes_precedence_over_relocation() {
    // If a metadata block overlaps the relocated-header region, blanking wins:
    // the block is not filesystem content wherever it sits. Reversing the
    // precedence would hand FVE metadata bytes to the filesystem reader as if
    // they were the volume's boot sector.
    let layout =
        VolumeLayout::from_metadata(&metadata(0x8000, [0x400, 0, 0], 0x200, 0x9000, 0x2000, 0));
    assert_eq!(layout.resolve(0x400), SectorSource::Blanked);
    assert_eq!(
        layout.resolve(0x600),
        SectorSource::Encrypted {
            physical_offset: 0x9600
        },
        "sectors outside the block still relocate"
    );
}

#[test]
fn every_non_zero_metadata_copy_is_blanked() {
    let layout = VolumeLayout::from_metadata(&metadata(
        0x8000,
        [0x1000, 0x4000, 0x7000],
        0x800,
        0x9000,
        0,
        0,
    ));
    for offset in [0x1000u64, 0x4000, 0x7000] {
        assert_eq!(
            layout.resolve(offset),
            SectorSource::Blanked,
            "at {offset:#x}"
        );
    }
}

#[test]
fn a_zero_metadata_offset_does_not_blank_sector_zero() {
    // A zero entry means "no such copy". Treating it as a real offset would blank
    // the start of the volume, which is where the relocated boot sector lives.
    let layout = VolumeLayout::from_metadata(&metadata(0x8000, [0, 0, 0], 0x800, 0x9000, 0, 0));
    assert_eq!(
        layout.resolve(0),
        SectorSource::Encrypted { physical_offset: 0 }
    );
}

#[test]
fn a_partially_encrypted_volume_returns_the_tail_as_plaintext() {
    // Conversion in progress: bytes at or past encrypted_volume_size are already
    // plaintext on disk and must not be run through the cipher.
    let layout = VolumeLayout::from_metadata(&metadata(0x8000, [0, 0, 0], 0x800, 0, 0, 0x8000));
    assert_eq!(
        layout.resolve(0x7E00),
        SectorSource::Encrypted {
            physical_offset: 0x7E00
        }
    );
    assert_eq!(
        layout.resolve(0x8000),
        SectorSource::Plaintext {
            physical_offset: 0x8000
        }
    );
}

#[test]
fn a_zero_encrypted_size_means_the_whole_volume_is_encrypted() {
    // Inverting this leaves a fully encrypted volume reading back as ciphertext,
    // which looks like a corrupt filesystem rather than a bug.
    let layout = VolumeLayout::from_metadata(&metadata(0x8000, [0, 0, 0], 0x800, 0, 0, 0));
    for offset in [0u64, 0x1000, 0xFFFF_0000] {
        assert_eq!(
            layout.resolve(offset),
            SectorSource::Encrypted {
                physical_offset: offset
            },
            "at {offset:#x}"
        );
    }
}

#[test]
fn the_encrypted_boundary_is_tested_against_the_physical_offset() {
    // A relocated sector's physical home can sit past the encrypted region even
    // though its logical offset does not. The classification has to follow the
    // physical address, since that is where the bytes actually are.
    let layout =
        VolumeLayout::from_metadata(&metadata(0x8000, [0, 0, 0], 0x800, 0x9000, 0x2000, 0x8000));
    assert_eq!(
        layout.resolve(0),
        SectorSource::Plaintext {
            physical_offset: 0x9000
        },
        "logical 0 lives at 0x9000, past the 0x8000 encrypted boundary"
    );
}

#[test]
fn an_unencrypted_volume_never_reports_encrypted_sectors() {
    let layout = VolumeLayout::from_metadata(&metadata(0x0000, [0x1000, 0, 0], 0x800, 0, 0, 0));
    assert!(!layout.is_encrypted());
    assert_eq!(
        layout.resolve(0x5000),
        SectorSource::Plaintext {
            physical_offset: 0x5000
        }
    );
    // Metadata blanking still applies: those blocks are not filesystem content
    // whether or not the volume is encrypted.
    assert_eq!(layout.resolve(0x1000), SectorSource::Blanked);
}

#[test]
fn an_encrypted_volume_reports_itself_as_encrypted() {
    for code in [0x8000u16, 0x8002, 0x8003, 0x8004, 0x8005] {
        let layout = VolumeLayout::from_metadata(&metadata(code, [0, 0, 0], 0x800, 0, 0, 0));
        assert!(layout.is_encrypted(), "method {code:#06X}");
    }
}

#[test]
fn resolution_saturates_instead_of_overflowing() {
    // A corrupt header can carry extreme offsets; the mapping must stay defined.
    let layout = VolumeLayout::from_metadata(&metadata(
        0x8000,
        [u64::MAX, 0, 0],
        u32::MAX,
        u64::MAX,
        u64::MAX,
        0,
    ));
    let _ = layout.resolve(0);
    let _ = layout.resolve(u64::MAX - 511);
}

#[test]
fn sector_start_rounds_down_to_the_cipher_boundary() {
    assert_eq!(sector_start(0), 0);
    assert_eq!(sector_start(511), 0);
    assert_eq!(sector_start(512), 512);
    assert_eq!(sector_start(513), 512);
    assert_eq!(sector_start(0x1234), 0x1200);
}
