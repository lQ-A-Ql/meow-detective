use evidence_core::{probe, EvidenceReader, RawImageReader};
use std::io::{Read, Seek, SeekFrom, Write};

fn fixed_vhd(path: &std::path::Path, payload: &[u8], disk_type: u32) {
    let mut bytes = vec![0u8; payload.len().max(512)];
    bytes[..payload.len()].copy_from_slice(payload);
    bytes.extend_from_slice(&[0u8; 512]);
    let footer = bytes.len() - 512;
    bytes[footer..footer + 8].copy_from_slice(b"conectix");
    bytes[footer + 48..footer + 56].copy_from_slice(&(payload.len() as u64).to_be_bytes());
    bytes[footer + 60..footer + 64].copy_from_slice(&disk_type.to_be_bytes());
    std::fs::File::create(path)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
}

#[test]
fn opens_fixed_vhd_with_virtual_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("disk.vhd");
    fixed_vhd(&path, b"fixed payload", 2);
    let mut reader = RawImageReader::open(&path).unwrap();
    assert_eq!(reader.info().kind, "vhd-fixed");
    assert_eq!(reader.info().size, 13);
    let mut data = Vec::new();
    reader.read_to_end(&mut data).unwrap();
    assert_eq!(data, b"fixed payload");
    reader.seek(SeekFrom::Start(0)).unwrap();
}

#[test]
fn probes_dynamic_vhd_as_a_readable_virtual_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic.vhd");
    fixed_vhd(&path, b"dynamic", 3);
    let result = probe::probe(&path).unwrap();
    assert_eq!(result.candidates, vec!["vhd-dynamic"]);
    assert!(result.can_read_sectors);
    assert!(RawImageReader::open(&path).is_err());
}

#[test]
fn opens_dynamic_vhd_zero_fills_unallocated_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dynamic.vhd");
    let mut bytes = vec![0u8; 4096];
    bytes[512..520].copy_from_slice(b"cxsparse");
    bytes[512 + 16..512 + 24].copy_from_slice(&1536u64.to_be_bytes());
    bytes[512 + 28..512 + 32].copy_from_slice(&2u32.to_be_bytes());
    bytes[512 + 32..512 + 36].copy_from_slice(&4096u32.to_be_bytes());
    // BAT at 1536 sectors, both blocks unallocated.
    bytes[1536..1540].copy_from_slice(&0xffff_ffffu32.to_be_bytes());
    bytes[1540..1544].copy_from_slice(&0xffff_ffffu32.to_be_bytes());
    let footer = bytes.len() - 512;
    bytes[footer..footer + 8].copy_from_slice(b"conectix");
    bytes[footer + 16..footer + 24].copy_from_slice(&512u64.to_be_bytes());
    bytes[footer + 48..footer + 56].copy_from_slice(&8192u64.to_be_bytes());
    bytes[footer + 60..footer + 64].copy_from_slice(&3u32.to_be_bytes());
    std::fs::write(&path, bytes).unwrap();
    let mut reader = RawImageReader::open(&path).unwrap();
    assert_eq!(reader.info().kind, "vhd-dynamic");
    let mut output = vec![1u8; 8192];
    reader.read_exact(&mut output).unwrap();
    assert!(output.iter().all(|value| *value == 0));
}

#[test]
fn opens_differencing_vhd_from_verified_relative_parent() {
    let dir = tempfile::tempdir().unwrap();
    let parent_path = dir.path().join("parent.vhd");
    fixed_vhd(&parent_path, &vec![b'P'; 8192], 2);
    let child_path = dir.path().join("child.vhd");
    let mut bytes = vec![0u8; 4096];
    bytes[512..520].copy_from_slice(b"cxsparse");
    bytes[512 + 16..512 + 24].copy_from_slice(&1536u64.to_be_bytes());
    bytes[512 + 28..512 + 32].copy_from_slice(&2u32.to_be_bytes());
    bytes[512 + 32..512 + 36].copy_from_slice(&4096u32.to_be_bytes());
    bytes[1536..1540].copy_from_slice(&0xffff_ffffu32.to_be_bytes());
    bytes[1540..1544].copy_from_slice(&0xffff_ffffu32.to_be_bytes());
    let footer = bytes.len() - 512;
    bytes[footer..footer + 8].copy_from_slice(b"conectix");
    bytes[footer + 16..footer + 24].copy_from_slice(&512u64.to_be_bytes());
    bytes[footer + 48..footer + 56].copy_from_slice(&8192u64.to_be_bytes());
    bytes[footer + 60..footer + 64].copy_from_slice(&4u32.to_be_bytes());
    for (index, unit) in "parent.vhd".encode_utf16().enumerate() {
        bytes[footer + 64 + index * 2..footer + 66 + index * 2]
            .copy_from_slice(&unit.to_be_bytes());
    }
    std::fs::write(&child_path, bytes).unwrap();
    let mut reader = RawImageReader::open(&child_path).unwrap();
    assert_eq!(reader.info().kind, "vhd-differencing");
    let mut output = vec![0u8; 8192];
    reader.read_exact(&mut output).unwrap();
    assert!(output.iter().all(|value| *value == b'P'));
}

#[test]
fn opens_qcow2_sparse_image_with_zero_unallocated_cluster() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("disk.qcow2");
    let mut bytes = vec![0u8; 2048];
    bytes[0..4].copy_from_slice(b"QFI\xfb");
    bytes[4..8].copy_from_slice(&3u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&9u32.to_be_bytes());
    bytes[24..32].copy_from_slice(&1024u64.to_be_bytes());
    bytes[36..40].copy_from_slice(&1u32.to_be_bytes());
    bytes[40..48].copy_from_slice(&512u64.to_be_bytes());
    bytes[512..520].copy_from_slice(&1024u64.to_be_bytes());
    bytes[1024..1032].copy_from_slice(&1536u64.to_be_bytes());
    bytes[1536..1541].copy_from_slice(b"hello");
    std::fs::write(&path, bytes).unwrap();
    let mut reader = RawImageReader::open(&path).unwrap();
    assert_eq!(reader.info().kind, "qcow2");
    let mut output = vec![0u8; 1024];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(&output[..5], b"hello");
    assert!(output[512..].iter().all(|value| *value == 0));
}

#[test]
fn opens_qcow2_with_relative_backing_file() {
    let dir = tempfile::tempdir().unwrap();
    let parent_path = dir.path().join("parent.raw");
    std::fs::write(&parent_path, b"parent payload").unwrap();

    let path = dir.path().join("child.qcow2");
    let mut bytes = vec![0u8; 1024];
    bytes[0..4].copy_from_slice(b"QFI\xfb");
    bytes[4..8].copy_from_slice(&3u32.to_be_bytes());
    bytes[8..16].copy_from_slice(&72u64.to_be_bytes());
    bytes[16..20].copy_from_slice(&("parent.raw".len() as u32).to_be_bytes());
    bytes[20..24].copy_from_slice(&9u32.to_be_bytes());
    bytes[24..32].copy_from_slice(&14u64.to_be_bytes());
    bytes[36..40].copy_from_slice(&1u32.to_be_bytes());
    bytes[40..48].copy_from_slice(&512u64.to_be_bytes());
    bytes[72..82].copy_from_slice(b"parent.raw");
    // An empty L1 entry makes every child cluster fall back to the parent.
    std::fs::write(&path, bytes).unwrap();

    let mut reader = RawImageReader::open(&path).unwrap();
    assert_eq!(reader.info().kind, "qcow2");
    let mut output = vec![0u8; 14];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(&output, b"parent payload");
    assert_eq!(reader.backing_paths().len(), 2);
}

#[test]
fn opens_qcow_v1_with_mapped_cluster() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("disk.qcow");
    let mut bytes = vec![0u8; 2048];
    bytes[0..4].copy_from_slice(b"QFI\xfb");
    bytes[4..8].copy_from_slice(&1u32.to_be_bytes());
    bytes[24..32].copy_from_slice(&512u64.to_be_bytes());
    bytes[32] = 9;
    bytes[33] = 6;
    bytes[40..48].copy_from_slice(&512u64.to_be_bytes());
    bytes[512..520].copy_from_slice(&1024u64.to_be_bytes());
    bytes[1024..1032].copy_from_slice(&1536u64.to_be_bytes());
    bytes[1536..1558].copy_from_slice(b"qcow v1 mapped payload");
    std::fs::write(&path, bytes).unwrap();

    let mut reader = RawImageReader::open(&path).unwrap();
    let mut output = vec![0u8; 22];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(&output, b"qcow v1 mapped payload");
}

#[test]
fn opens_vhdx_with_bat_mapped_payload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("disk.vhdx");
    let mut bytes = vec![0u8; 3 * 1024 * 1024];
    bytes[..8].copy_from_slice(b"vhdxfile");
    let region = 192 * 1024;
    bytes[region..region + 4].copy_from_slice(b"regi");
    bytes[region + 8..region + 12].copy_from_slice(&2u32.to_le_bytes());
    let bat_guid = [
        0x66, 0x77, 0xc2, 0x2d, 0x23, 0xf6, 0x00, 0x42, 0x9d, 0x64, 0x11, 0x5e, 0x9b, 0xfd, 0x4a,
        0x08,
    ];
    let metadata_guid = [
        0x06, 0xa2, 0x7c, 0x8b, 0x90, 0x47, 0x9a, 0x4b, 0xb8, 0xfe, 0x57, 0x5f, 0x05, 0x0f, 0x88,
        0x6e,
    ];
    bytes[region + 16..region + 32].copy_from_slice(&bat_guid);
    bytes[region + 32..region + 40].copy_from_slice(&(1024u64 * 1024).to_le_bytes());
    bytes[region + 40..region + 44].copy_from_slice(&(1024u32 * 1024).to_le_bytes());
    bytes[region + 48..region + 64].copy_from_slice(&metadata_guid);
    bytes[region + 64..region + 72].copy_from_slice(&(256u64 * 1024).to_le_bytes());
    bytes[region + 72..region + 76].copy_from_slice(&(1024u32 * 1024).to_le_bytes());
    let metadata = 256 * 1024;
    bytes[metadata..metadata + 8].copy_from_slice(b"metadata");
    bytes[metadata + 10..metadata + 12].copy_from_slice(&3u16.to_le_bytes());
    let params_guid = [
        0x37, 0x67, 0xa1, 0xca, 0x36, 0xfa, 0x43, 0x4d, 0xb3, 0xb6, 0x33, 0xf0, 0xaa, 0x44, 0xe7,
        0x6b,
    ];
    let size_guid = [
        0x24, 0x42, 0xa5, 0x2f, 0x1b, 0xcd, 0x76, 0x48, 0xb2, 0x11, 0x5d, 0xbe, 0xd8, 0x3b, 0xf4,
        0xb8,
    ];
    let sector_guid = [
        0x1d, 0xbf, 0x41, 0x81, 0x6f, 0xa9, 0x09, 0x47, 0xba, 0x47, 0xf2, 0x33, 0xa8, 0xfa, 0xab,
        0x5f,
    ];
    for (index, guid) in [params_guid, size_guid, sector_guid]
        .into_iter()
        .enumerate()
    {
        let entry = metadata + 32 + index * 32;
        bytes[entry..entry + 16].copy_from_slice(&guid);
        bytes[entry + 16..entry + 20]
            .copy_from_slice(&(64 * 1024 + index as u32 * 8).to_le_bytes());
        bytes[entry + 20..entry + 24].copy_from_slice(&8u32.to_le_bytes());
    }
    bytes[metadata + 64 * 1024..metadata + 64 * 1024 + 4]
        .copy_from_slice(&1_048_576u32.to_le_bytes());
    bytes[metadata + 64 * 1024 + 4..metadata + 64 * 1024 + 8].copy_from_slice(&1u32.to_le_bytes());
    bytes[metadata + 64 * 1024 + 8..metadata + 64 * 1024 + 16]
        .copy_from_slice(&1_048_576u64.to_le_bytes());
    bytes[metadata + 64 * 1024 + 16..metadata + 64 * 1024 + 20]
        .copy_from_slice(&512u32.to_le_bytes());
    bytes[1024 * 1024..1024 * 1024 + 8].copy_from_slice(&(2u64 * 1024 * 1024 + 6).to_le_bytes());
    bytes[2 * 1024 * 1024..2 * 1024 * 1024 + 5].copy_from_slice(b"hello");
    std::fs::write(&path, bytes).unwrap();
    let mut reader = RawImageReader::open(&path).unwrap();
    assert_eq!(reader.info().kind, "vhdx");
    let mut output = [0u8; 5];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(&output, b"hello");
}

#[test]
fn opens_vhdx_differencing_child_from_relative_parent() {
    let dir = tempfile::tempdir().unwrap();
    let parent_path = dir.path().join("parent.vhdx");
    let child_path = dir.path().join("child.vhdx");
    write_synthetic_vhdx(&parent_path, b"parent", false, None);
    write_synthetic_vhdx(&child_path, b"ignored", true, Some("parent.vhdx"));

    let mut reader = RawImageReader::open(&child_path).unwrap();
    assert_eq!(reader.info().kind, "vhdx");
    let mut output = [0u8; 6];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(&output, b"parent");
    assert_eq!(reader.backing_paths().len(), 2);
}

fn write_synthetic_vhdx(
    path: &std::path::Path,
    payload: &[u8],
    differencing: bool,
    parent_name: Option<&str>,
) {
    let mut bytes = vec![0u8; 3 * 1024 * 1024];
    bytes[..8].copy_from_slice(b"vhdxfile");
    let region = 192 * 1024;
    bytes[region..region + 4].copy_from_slice(b"regi");
    bytes[region + 8..region + 12].copy_from_slice(&2u32.to_le_bytes());
    let bat_guid = [
        0x66, 0x77, 0xc2, 0x2d, 0x23, 0xf6, 0x00, 0x42, 0x9d, 0x64, 0x11, 0x5e, 0x9b, 0xfd, 0x4a,
        0x08,
    ];
    let metadata_guid = [
        0x06, 0xa2, 0x7c, 0x8b, 0x90, 0x47, 0x9a, 0x4b, 0xb8, 0xfe, 0x57, 0x5f, 0x05, 0x0f, 0x88,
        0x6e,
    ];
    bytes[region + 16..region + 32].copy_from_slice(&bat_guid);
    bytes[region + 32..region + 40].copy_from_slice(&(1024u64 * 1024).to_le_bytes());
    bytes[region + 40..region + 44].copy_from_slice(&(1024u32 * 1024).to_le_bytes());
    bytes[region + 48..region + 64].copy_from_slice(&metadata_guid);
    bytes[region + 64..region + 72].copy_from_slice(&(256u64 * 1024).to_le_bytes());
    bytes[region + 72..region + 76].copy_from_slice(&(1024u32 * 1024).to_le_bytes());

    let metadata = 256 * 1024;
    bytes[metadata..metadata + 8].copy_from_slice(b"metadata");
    let item_count = if parent_name.is_some() { 4u16 } else { 3u16 };
    bytes[metadata + 10..metadata + 12].copy_from_slice(&item_count.to_le_bytes());
    let params_guid = [
        0x37, 0x67, 0xa1, 0xca, 0x36, 0xfa, 0x43, 0x4d, 0xb3, 0xb6, 0x33, 0xf0, 0xaa, 0x44, 0xe7,
        0x6b,
    ];
    let size_guid = [
        0x24, 0x42, 0xa5, 0x2f, 0x1b, 0xcd, 0x76, 0x48, 0xb2, 0x11, 0x5d, 0xbe, 0xd8, 0x3b, 0xf4,
        0xb8,
    ];
    let sector_guid = [
        0x1d, 0xbf, 0x41, 0x81, 0x6f, 0xa9, 0x09, 0x47, 0xba, 0x47, 0xf2, 0x33, 0xa8, 0xfa, 0xab,
        0x5f,
    ];
    for (index, guid) in [params_guid, size_guid, sector_guid]
        .into_iter()
        .enumerate()
    {
        let entry = metadata + 32 + index * 32;
        bytes[entry..entry + 16].copy_from_slice(&guid);
        bytes[entry + 16..entry + 20]
            .copy_from_slice(&(64 * 1024 + index as u32 * 8).to_le_bytes());
        bytes[entry + 20..entry + 24].copy_from_slice(&8u32.to_le_bytes());
    }
    if let Some(parent_name) = parent_name {
        let entry = metadata + 32 + 3 * 32;
        let parent_guid = [
            0x2d, 0x5f, 0xd3, 0xa8, 0x0b, 0xb3, 0x4d, 0x45, 0xab, 0xf7, 0xd3, 0xd8, 0x48, 0x34,
            0xab, 0x00,
        ];
        bytes[entry..entry + 16].copy_from_slice(&parent_guid);
        bytes[entry + 16..entry + 20].copy_from_slice(&(64 * 1024 + 24u32).to_le_bytes());
        bytes[entry + 20..entry + 24].copy_from_slice(&128u32.to_le_bytes());
        for (index, unit) in parent_name.encode_utf16().enumerate() {
            bytes[metadata + 64 * 1024 + 24 + index * 2..metadata + 64 * 1024 + 26 + index * 2]
                .copy_from_slice(&unit.to_le_bytes());
        }
    }
    bytes[metadata + 64 * 1024..metadata + 64 * 1024 + 4]
        .copy_from_slice(&1_048_576u32.to_le_bytes());
    bytes[metadata + 64 * 1024 + 4..metadata + 64 * 1024 + 8]
        .copy_from_slice(&(if differencing { 2u32 } else { 1u32 }).to_le_bytes());
    bytes[metadata + 64 * 1024 + 8..metadata + 64 * 1024 + 16]
        .copy_from_slice(&1_048_576u64.to_le_bytes());
    bytes[metadata + 64 * 1024 + 16..metadata + 64 * 1024 + 20]
        .copy_from_slice(&512u32.to_le_bytes());
    let descriptor = if differencing { 0 } else { 2 * 1024 * 1024 + 6 };
    bytes[1024 * 1024..1024 * 1024 + 8].copy_from_slice(&(descriptor as u64).to_le_bytes());
    if !differencing {
        bytes[2 * 1024 * 1024..2 * 1024 * 1024 + payload.len()].copy_from_slice(payload);
    }
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn rejects_fixed_vhd_that_claims_bytes_beyond_data_before_footer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("truncated.vhd");
    fixed_vhd(&path, b"payload", 2);
    let mut bytes = std::fs::read(&path).unwrap();
    let footer = bytes.len() - 512;
    bytes[footer + 48..footer + 56].copy_from_slice(&(4096u64).to_be_bytes());
    std::fs::write(&path, bytes).unwrap();
    let error = RawImageReader::open(&path).unwrap_err();
    assert!(error.to_string().contains("fixed VHD is truncated"));
}

#[test]
fn unknown_extension_is_not_guessed_as_raw() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unknown.container");
    std::fs::write(&path, b"not a recognized image").unwrap();
    let result = probe::probe(&path).unwrap();
    assert!(result.candidates.is_empty());
    assert!(!result.can_read_sectors);
}
