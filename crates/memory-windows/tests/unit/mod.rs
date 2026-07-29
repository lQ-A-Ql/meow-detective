use std::io::Write;

use crate::{
    aes_schedule::is_valid_aes_schedule, discover_kernel, find_kdbg_candidates,
    find_processor_start_blocks, scan_bitlocker_key_candidates, scan_pool_tag, AesKeyBits,
    BitLockerPoolTag, RawMemoryImage, X64AddressSpace,
};
use tempfile::NamedTempFile;

fn write_image(bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("temp image");
    file.write_all(bytes).expect("write image");
    file.flush().expect("flush image");
    file
}

#[test]
fn x64_address_space_reads_a_mapped_page() {
    let mut image = vec![0u8; 0x6000];
    write_entry(&mut image, 0x1000, 0, 0x2000);
    write_entry(&mut image, 0x2000, 0, 0x3000);
    write_entry(&mut image, 0x3000, 2, 0x4000);
    write_entry(&mut image, 0x4000, 0, 0x5000);
    image[0x5000..0x5004].copy_from_slice(b"test");

    let file = write_image(&image);
    let raw = RawMemoryImage::open(file.path()).expect("open image");
    let mut address_space = X64AddressSpace::new(raw, 0x1000).expect("address space");
    let mut bytes = [0u8; 4];
    address_space
        .read_virtual_exact(0x0040_0000, &mut bytes)
        .expect("virtual read");
    assert_eq!(&bytes, b"test");
}

#[test]
fn x64_alias_walk_preserves_shared_page_table_aliases() {
    let mut image = vec![0u8; 0x6000];
    write_entry(&mut image, 0x1000, 0, 0x2000);
    write_entry(&mut image, 0x1000, 1, 0x2000);
    write_entry(&mut image, 0x2000, 0, 0x3000);
    write_entry(&mut image, 0x3000, 0, 0x4000);
    write_entry(&mut image, 0x4000, 0, 0x5000);

    let file = write_image(&image);
    let raw = RawMemoryImage::open(file.path()).expect("open image");
    let mut address_space = X64AddressSpace::new(raw, 0x1000).expect("address space");
    let aliases = address_space
        .find_virtual_aliases(0x5000, 8)
        .expect("find shared aliases");

    assert_eq!(aliases, vec![0, 1_u64 << 39]);
}

#[test]
fn x64_alias_walk_does_not_treat_a_large_data_page_as_a_page_table() {
    let mut image = vec![0u8; 0x220000];
    write_entry(&mut image, 0x1000, 0, 0x2000);
    write_entry(&mut image, 0x2000, 0, 0x3000);
    write_u64(&mut image, 0x3000, 1 | (1 << 7));
    write_entry(&mut image, 0, 0, 0x210000);

    let file = write_image(&image);
    let raw = RawMemoryImage::open(file.path()).expect("open image");
    let mut address_space = X64AddressSpace::new(raw, 0x1000).expect("address space");

    assert!(address_space
        .find_virtual_aliases(0x210000, 8)
        .expect("bounded alias walk")
        .is_empty());
}

#[test]
fn processor_start_block_recovers_only_a_mapped_cr3() {
    const TARGET: u64 = 0xFFFF_F800_0040_0000;
    let mut image = vec![0u8; 0x9000];
    write_entry(&mut image, 0x1000, 496, 0x2000);
    write_entry(&mut image, 0x2000, 0, 0x3000);
    write_entry(&mut image, 0x3000, 2, 0x4000);
    write_entry(&mut image, 0x4000, 0, 0x5000);
    write_start_block(&mut image, 0x6000, TARGET, 0x1000);
    write_start_block(&mut image, 0x7000, TARGET, 0x8000);

    let file = write_image(&image);
    let mut raw = RawMemoryImage::open(file.path()).expect("open image");
    let blocks = find_processor_start_blocks(&mut raw).expect("find start block");

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].physical_address, 0x6000);
    assert_eq!(blocks[0].long_mode_target, TARGET);
    assert_eq!(blocks[0].directory_table_base, 0x1000);
}

#[test]
fn aes_schedule_validation_matches_fips_197_and_rejects_one_bit_damage() {
    let mut schedule = decode_hex(concat!(
        "2b7e151628aed2a6abf7158809cf4f3c",
        "a0fafe1788542cb123a339392a6c7605",
        "f2c295f27a96b9435935807a7359f67f",
        "3d80477d4716fe3e1e237e446d7a883b",
        "ef44a541a8525b7fb671253bdb0bad00",
        "d4d1c6f87c839d87caf2b8bc11f915bc",
        "6d88a37a110b3efddbf98641ca0093fd",
        "4e54f70e5f5fc9f384a64fb24ea6dc4f",
        "ead27321b58dbad2312bf5607f8d292f",
        "ac7766f319fadc2128d12941575c006e",
        "d014f9a8c9ee2589e13f0cc8b6630ca6"
    ));
    assert!(is_valid_aes_schedule(&schedule, 16));
    schedule[175] ^= 1;
    assert!(!is_valid_aes_schedule(&schedule, 16));
}

#[test]
fn bitlocker_pool_scan_returns_secret_candidate_metadata() {
    let schedule = decode_hex(concat!(
        "2b7e151628aed2a6abf7158809cf4f3c",
        "a0fafe1788542cb123a339392a6c7605",
        "f2c295f27a96b9435935807a7359f67f",
        "3d80477d4716fe3e1e237e446d7a883b",
        "ef44a541a8525b7fb671253bdb0bad00",
        "d4d1c6f87c839d87caf2b8bc11f915bc",
        "6d88a37a110b3efddbf98641ca0093fd",
        "4e54f70e5f5fc9f384a64fb24ea6dc4f",
        "ead27321b58dbad2312bf5607f8d292f",
        "ac7766f319fadc2128d12941575c006e",
        "d014f9a8c9ee2589e13f0cc8b6630ca6"
    ));
    let mut image = vec![0u8; 0x1000];
    let header = 0x100usize;
    image[header + 2] = 16;
    image[header + 4..header + 8].copy_from_slice(b"FVEc");
    image[header + 0x10 + 8..header + 0x10 + 8 + schedule.len()].copy_from_slice(&schedule);
    let file = write_image(&image);
    let mut raw = RawMemoryImage::open(file.path()).expect("open image");

    let candidates = scan_bitlocker_key_candidates(&mut raw, 8, 8).expect("scan candidates");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].bits(), AesKeyBits::Aes128);
    assert_eq!(candidates[0].pool_tag(), BitLockerPoolTag::FveContext);
    assert_eq!(candidates[0].pool_physical_address(), header as u64);
    assert_eq!(candidates[0].schedule_offset(), 8);
    assert_eq!(candidates[0].recovered_key().len(), 16);
}

#[test]
fn bitlocker_pool_scan_rejects_damaged_aes_schedules() {
    let mut image = vec![0u8; 0x1000];
    let header = 0x100usize;
    image[header + 2] = 16;
    image[header + 4..header + 8].copy_from_slice(b"FVEc");
    image[header + 0x18..header + 0x18 + 176].fill(0xA5);
    let file = write_image(&image);
    let mut raw = RawMemoryImage::open(file.path()).expect("open image");

    let candidates = scan_bitlocker_key_candidates(&mut raw, 8, 8).expect("scan candidates");
    assert!(candidates.is_empty());
}

#[test]
fn kdbg_scan_rejects_non_structural_tag_hits() {
    let mut image = vec![0u8; 0x3000];
    image[0x100..0x104].copy_from_slice(b"KDBG");
    let file = write_image(&image);
    let mut raw = RawMemoryImage::open(file.path()).expect("open image");
    assert!(find_kdbg_candidates(&mut raw).is_err());
}

#[test]
fn pool_inventory_uses_validated_header_size() {
    let mut image = vec![0u8; 0x1000];
    let header = 0x100usize;
    image[header + 2] = 2;
    image[header + 4..header + 8].copy_from_slice(b"FVE2");
    let file = write_image(&image);
    let mut raw = RawMemoryImage::open(file.path()).expect("open image");

    let allocations = scan_pool_tag(&mut raw, *b"FVE2", 8).expect("scan pool tag");
    assert_eq!(allocations.len(), 1);
    assert_eq!(allocations[0].header_physical_address, header as u64);
    assert_eq!(allocations[0].body_physical_address, header as u64 + 0x10);
    assert_eq!(allocations[0].allocation_bytes, 0x20);
}

#[test]
fn kernel_discovery_finds_fvevol_without_external_tools() {
    const KERNEL_BASE: u64 = 0xFFFF_F800_0000_0000;
    const LIST_HEAD: u64 = KERNEL_BASE + 0x1000;
    const ENTRY: u64 = KERNEL_BASE + 0x2000;
    const NAME: u64 = KERNEL_BASE + 0x3000;
    const FVEVOL: u64 = KERNEL_BASE + 0x4000;

    let mut image = vec![0u8; 0xB000];
    map_kernel_page(&mut image, KERNEL_BASE, 0x5000);
    map_kernel_page(&mut image, LIST_HEAD, 0x6000);
    map_kernel_page(&mut image, ENTRY, 0x7000);
    map_kernel_page(&mut image, NAME, 0x8000);
    map_kernel_page(&mut image, FVEVOL, 0x9000);

    write_pe_with_codeview(&mut image, 0x5000, 0x1234_5678, 0x144F000, "ntkrnlmp.pdb");
    write_pe_with_codeview(&mut image, 0x9000, 0xCAFEBABE, 0xE1000, "fvevol.pdb");
    write_u64(&mut image, 0x6000, ENTRY);
    write_u64(&mut image, 0x7000, LIST_HEAD);
    write_u64(&mut image, 0x7000 + 0x30, FVEVOL);
    write_u32(&mut image, 0x7000 + 0x40, 0xE1000);
    let name = "fvevol.sys"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    write_u16(&mut image, 0x7000 + 0x58, name.len() as u16);
    write_u64(&mut image, 0x7000 + 0x60, NAME);
    image[0x8000..0x8000 + name.len()].copy_from_slice(&name);

    let kdbg = 0xA000usize;
    image[kdbg + 0x10..kdbg + 0x14].copy_from_slice(b"KDBG");
    write_u32(&mut image, kdbg + 0x14, 0x300);
    write_u64(&mut image, kdbg + 0x18, KERNEL_BASE);
    write_u64(&mut image, kdbg + 0x48, LIST_HEAD);

    let file = write_image(&image);
    let discovery = discover_kernel(file.path()).expect("discover kernel");
    assert_eq!(discovery.directory_table_base, 0x1000);
    assert_eq!(discovery.kernel_base, KERNEL_BASE);
    assert_eq!(discovery.fvevol.expect("fvevol").base, FVEVOL);
    assert_eq!(
        discovery.fvevol_identity.expect("identity").pdb_name,
        "fvevol.pdb"
    );
}

fn map_kernel_page(image: &mut [u8], virtual_address: u64, physical_address: u64) {
    let pml4 = ((virtual_address >> 39) & 0x1FF) as usize;
    let pdpt = ((virtual_address >> 30) & 0x1FF) as usize;
    let pd = ((virtual_address >> 21) & 0x1FF) as usize;
    let pt = ((virtual_address >> 12) & 0x1FF) as usize;
    write_entry(image, 0x1000, pml4, 0x2000);
    write_entry(image, 0x2000, pdpt, 0x3000);
    write_entry(image, 0x3000, pd, 0x4000);
    write_entry(image, 0x4000, pt, physical_address);
}

fn write_pe_with_codeview(
    image: &mut [u8],
    offset: usize,
    timestamp: u32,
    image_size: u32,
    pdb_name: &str,
) {
    image[offset..offset + 2].copy_from_slice(b"MZ");
    write_u32(image, offset + 0x3C, 0x80);
    image[offset + 0x80..offset + 0x84].copy_from_slice(b"PE\0\0");
    write_u16(image, offset + 0x84, 0x8664);
    write_u32(image, offset + 0x88, timestamp);
    write_u16(image, offset + 0x98, 0x20B);
    write_u32(image, offset + 0x98 + 56, image_size);
    let debug = offset + 0x98 + 112 + 6 * 8;
    write_u32(image, debug, 0x200);
    write_u32(image, debug + 4, 28);
    write_u32(image, offset + 0x200 + 12, 2);
    write_u32(image, offset + 0x200 + 16, (24 + pdb_name.len() + 1) as u32);
    write_u32(image, offset + 0x200 + 20, 0x300);
    image[offset + 0x300..offset + 0x304].copy_from_slice(b"RSDS");
    image[offset + 0x304..offset + 0x314].copy_from_slice(&[1; 16]);
    write_u32(image, offset + 0x314, 1);
    image[offset + 0x318..offset + 0x318 + pdb_name.len()].copy_from_slice(pdb_name.as_bytes());
}

fn write_entry(image: &mut [u8], table: usize, index: usize, address: u64) {
    write_u64(image, table + index * 8, address | 1);
}

fn write_u16(image: &mut [u8], offset: usize, value: u16) {
    image[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(image: &mut [u8], offset: usize, value: u32) {
    image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(image: &mut [u8], offset: usize, value: u64) {
    image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn write_start_block(image: &mut [u8], offset: usize, target: u64, cr3: u64) {
    write_u64(image, offset, 0x0000_0001_0006_00E9);
    write_u64(image, offset + 0x70, target);
    write_u64(image, offset + 0xA0, cr3);
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("hex high");
            let low = (pair[1] as char).to_digit(16).expect("hex low");
            ((high << 4) | low) as u8
        })
        .collect()
}
