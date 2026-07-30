use std::io::Write;

use crate::{find_processor_start_blocks, MemoryWindowsError, RawMemoryImage, X64AddressSpace};
use tempfile::NamedTempFile;

fn write_image(bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("temp image");
    file.write_all(bytes).expect("write image");
    file.flush().expect("flush image");
    file
}

#[test]
fn raw_memory_read_budget_fails_before_excess_physical_io() {
    let file = write_image(&[0u8; 16]);
    let mut image = RawMemoryImage::open(file.path()).expect("open image");
    image.set_read_budget(1, 4).expect("set read budget");

    image
        .read_exact_at(0, &mut [0u8; 4])
        .expect("read within budget");
    let error = image
        .read_exact_at(4, &mut [0u8; 1])
        .expect_err("second read must exceed the operation budget");

    assert!(matches!(
        error,
        MemoryWindowsError::TargetedScanBudgetExceeded {
            resource: "physical-read-operation",
            limit: 1
        }
    ));
    assert_eq!(image.read_stats().operations, 1);
    assert_eq!(image.read_stats().bytes_read, 4);
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

fn write_entry(image: &mut [u8], table: usize, index: usize, address: u64) {
    write_u64(image, table + index * 8, address | 1);
}

fn write_u64(image: &mut [u8], offset: usize, value: u64) {
    image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn write_start_block(image: &mut [u8], offset: usize, target: u64, cr3: u64) {
    write_u64(image, offset, 0x0000_0001_0006_00E9);
    write_u64(image, offset + 0x70, target);
    write_u64(image, offset + 0xA0, cr3);
}
