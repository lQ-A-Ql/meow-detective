use std::io::{Read, Seek, SeekFrom, Write};

use evidence_core::{FileSystemReader, RawImageReader};
use fs_erofs::ErofsReader;
use fs_ext4::Ext4Reader;
use fs_f2fs::F2fsReader;
use image_android::{AndroidSparseReader, SPARSE_DONT_CARE_CHUNK, SPARSE_MAGIC, SPARSE_RAW_CHUNK};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use testing::builders::{
    erofs::minimal_erofs_image, ext4::minimal_ext4_image, f2fs::minimal_f2fs_image,
};
use volume_android::{
    probe_filesystem, AndroidFilesystemKind, GeometryCopy, LogicalPartitionReader, MetadataCopy,
    SuperMetadata, VolumeAndroidError, LP_METADATA_GEOMETRY_MAGIC, LP_METADATA_HEADER_MAGIC,
};

const GEOMETRY_PRIMARY: usize = 4096;
const GEOMETRY_BACKUP: usize = 8192;
const METADATA_PRIMARY: usize = 12_288;
const METADATA_BACKUP: usize = 16_384;
const DATA_OFFSET: usize = 64 * 512;
const IMAGE_SIZE: usize = 65_536;

#[test]
fn maps_linear_and_zero_extents_without_materializing_partition() {
    let image = valid_super_image(0);
    let file = write_temp(&image);
    let mut parser_source = std::fs::File::open(file.path()).expect("open parser source");
    let metadata = SuperMetadata::read_slot(&mut parser_source, 0).expect("read liblp metadata");
    assert_eq!(metadata.geometry.source_copy, GeometryCopy::Primary);
    assert_eq!(metadata.source_copy, MetadataCopy::Primary);
    let partition = metadata
        .partition("system")
        .expect("system partition")
        .clone();
    assert_eq!(partition.size, 1024);

    let source = RawImageReader::open(file.path()).expect("open raw super image");
    let mut reader = LogicalPartitionReader::new(Box::new(source), partition)
        .expect("build logical partition reader");
    let mut output = vec![0xff; 1024];
    reader
        .read_exact(&mut output)
        .expect("read logical partition");
    assert_eq!(&output[..512], vec![0x5a; 512]);
    assert_eq!(&output[512..], vec![0; 512]);

    reader.seek(SeekFrom::Start(508)).expect("seek boundary");
    let mut boundary = [0xff; 8];
    reader.read_exact(&mut boundary).expect("read boundary");
    assert_eq!(&boundary[..4], &[0x5a; 4]);
    assert_eq!(&boundary[4..], &[0; 4]);
}

#[test]
fn maps_a_logical_partition_directly_over_an_android_sparse_reader() {
    let sparse = sparse_wrap(&valid_super_image(0), 4096);
    let file = write_temp(&sparse);
    let mut parser_source = AndroidSparseReader::open(file.path()).expect("open sparse super");
    let metadata = SuperMetadata::read_slot(&mut parser_source, 0).expect("parse sparse super");
    let partition = metadata
        .partition("system")
        .expect("system partition")
        .clone();
    let source = AndroidSparseReader::open(file.path()).expect("reopen sparse super");
    let mut reader = LogicalPartitionReader::new(Box::new(source), partition)
        .expect("map sparse logical partition");
    let mut boundary = [0xff; 8];
    reader
        .read_range(508, &mut boundary)
        .expect("read across linear/zero extents");
    assert_eq!(&boundary[..4], &[0x5a; 4]);
    assert_eq!(&boundary[4..], &[0; 4]);
    assert!(sparse.len() < IMAGE_SIZE);
}

#[test]
fn reads_ext4_tree_and_ranges_through_sparse_super_without_expansion() {
    let raw = super_image_with_partition(&minimal_ext4_image());
    let sparse = sparse_wrap(&raw, 4096);
    assert!(sparse.len() < raw.len());
    let file = write_temp(&sparse);

    let mut parser_source = AndroidSparseReader::open(file.path()).expect("open sparse super");
    let metadata = SuperMetadata::read_slot(&mut parser_source, 0).expect("parse liblp metadata");
    let partition = metadata
        .partition("system")
        .expect("system partition")
        .clone();
    let source = AndroidSparseReader::open(file.path()).expect("reopen sparse super");
    let mut logical = LogicalPartitionReader::new(Box::new(source), partition)
        .expect("map ext4 logical partition");
    assert_eq!(
        probe_filesystem(&mut logical).expect("probe ext4 logical partition"),
        AndroidFilesystemKind::Ext4
    );

    let filesystem = Ext4Reader::open(Box::new(logical), 0).expect("open ext4 filesystem");
    let mut root_names = filesystem
        .list_children("")
        .expect("list ext4 root")
        .into_iter()
        .map(|node| node.name)
        .collect::<Vec<_>>();
    root_names.sort();
    assert_eq!(root_names, ["subdir", "test.txt"]);

    let nested = filesystem
        .list_children("subdir")
        .expect("list nested directory");
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].name, "hello.dat");
    assert_eq!(
        filesystem
            .read_file_range("test.txt", 6, 5)
            .expect("read bounded file range"),
        b"World"
    );
    let mut nested_file = filesystem
        .open_file("subdir/hello.dat")
        .expect("open nested file");
    let mut nested_content = String::new();
    nested_file
        .read_to_string(&mut nested_content)
        .expect("read nested file");
    assert_eq!(nested_content, "Hello subdir!");
}

#[test]
fn reads_f2fs_tree_and_ranges_through_sparse_super_without_expansion() {
    let raw = super_image_with_partition(&minimal_f2fs_image());
    let sparse = sparse_wrap(&raw, 4096);
    assert!(sparse.len() < raw.len());
    let file = write_temp(&sparse);

    let mut parser_source = AndroidSparseReader::open(file.path()).expect("open sparse super");
    let metadata = SuperMetadata::read_slot(&mut parser_source, 0).expect("parse liblp metadata");
    let partition = metadata
        .partition("system")
        .expect("system partition")
        .clone();
    let source = AndroidSparseReader::open(file.path()).expect("reopen sparse super");
    let mut logical = LogicalPartitionReader::new(Box::new(source), partition)
        .expect("map F2FS logical partition");
    assert_eq!(
        probe_filesystem(&mut logical).expect("probe F2FS logical partition"),
        AndroidFilesystemKind::F2fs
    );

    let filesystem = F2fsReader::open(Box::new(logical), 0).expect("open F2FS filesystem");
    let children = filesystem.list_children("").expect("list F2FS root");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "hello.txt");
    assert_eq!(
        filesystem
            .read_file_range("hello.txt", 6, 4)
            .expect("read bounded F2FS range"),
        b"F2FS"
    );
}

#[test]
fn reads_erofs_tree_and_ranges_through_sparse_super_without_expansion() {
    let raw = super_image_with_partition(&minimal_erofs_image());
    let sparse = sparse_wrap(&raw, 4096);
    assert!(sparse.len() < raw.len());
    let file = write_temp(&sparse);

    let mut parser_source = AndroidSparseReader::open(file.path()).expect("open sparse super");
    let metadata = SuperMetadata::read_slot(&mut parser_source, 0).expect("parse liblp metadata");
    let partition = metadata
        .partition("system")
        .expect("system partition")
        .clone();
    let source = AndroidSparseReader::open(file.path()).expect("reopen sparse super");
    let mut logical = LogicalPartitionReader::new(Box::new(source), partition)
        .expect("map EROFS logical partition");
    assert_eq!(
        probe_filesystem(&mut logical).expect("probe EROFS logical partition"),
        AndroidFilesystemKind::Erofs
    );

    let filesystem = ErofsReader::open(Box::new(logical), 0).expect("open EROFS filesystem");
    let children = filesystem.list_children("").expect("list EROFS root");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "hello.txt");
    assert_eq!(
        filesystem
            .read_file_range("hello.txt", 6, 6)
            .expect("read bounded EROFS range"),
        b"EROFS!"
    );
}

#[test]
fn falls_back_to_valid_geometry_and_metadata_backups() {
    let mut geometry_fallback = valid_super_image(0);
    geometry_fallback[GEOMETRY_PRIMARY + 8] ^= 0xff;
    let mut source = std::io::Cursor::new(geometry_fallback);
    let metadata = SuperMetadata::read_slot(&mut source, 0).expect("backup geometry");
    assert_eq!(metadata.geometry.source_copy, GeometryCopy::Backup);

    let mut metadata_fallback = valid_super_image(0);
    metadata_fallback[METADATA_PRIMARY + 12] ^= 0xff;
    let mut source = std::io::Cursor::new(metadata_fallback);
    let metadata = SuperMetadata::read_slot(&mut source, 0).expect("backup metadata");
    assert_eq!(metadata.source_copy, MetadataCopy::Backup);
}

#[test]
fn rejects_both_metadata_copies_when_tables_are_tampered() {
    let mut image = valid_super_image(0);
    image[METADATA_PRIMARY + 128] ^= 0x01;
    image[METADATA_BACKUP + 128] ^= 0x01;
    let error = SuperMetadata::read_slot(&mut std::io::Cursor::new(image), 0)
        .expect_err("corrupt table checksums");
    assert!(matches!(
        error,
        VolumeAndroidError::MetadataCopiesInvalid { slot: 0, .. }
    ));
}

#[test]
fn applies_slot_suffix_without_guessing_an_active_slot() {
    let image = valid_super_image(1 << 1);
    let metadata = SuperMetadata::read_slot(&mut std::io::Cursor::new(image), 0)
        .expect("slot-suffixed metadata");
    assert!(metadata.partition("system_a").is_some());
    assert!(metadata.partition("system").is_none());
}

fn valid_super_image(partition_attributes: u32) -> Vec<u8> {
    let mut image = vec![0u8; IMAGE_SIZE];
    let geometry = geometry_bytes();
    image[GEOMETRY_PRIMARY..GEOMETRY_PRIMARY + geometry.len()].copy_from_slice(&geometry);
    image[GEOMETRY_BACKUP..GEOMETRY_BACKUP + geometry.len()].copy_from_slice(&geometry);
    let metadata = metadata_bytes(partition_attributes);
    image[METADATA_PRIMARY..METADATA_PRIMARY + metadata.len()].copy_from_slice(&metadata);
    image[METADATA_BACKUP..METADATA_BACKUP + metadata.len()].copy_from_slice(&metadata);
    image[DATA_OFFSET..DATA_OFFSET + 512].fill(0x5a);
    image
}

fn super_image_with_partition(payload: &[u8]) -> Vec<u8> {
    let image_size = DATA_OFFSET + payload.len();
    let mut image = vec![0u8; image_size];
    let geometry = geometry_bytes();
    image[GEOMETRY_PRIMARY..GEOMETRY_PRIMARY + geometry.len()].copy_from_slice(&geometry);
    image[GEOMETRY_BACKUP..GEOMETRY_BACKUP + geometry.len()].copy_from_slice(&geometry);
    let metadata = linear_partition_metadata_bytes(
        "system",
        payload.len() as u64 / 512,
        DATA_OFFSET as u64 / 512,
        image_size as u64,
    );
    image[METADATA_PRIMARY..METADATA_PRIMARY + metadata.len()].copy_from_slice(&metadata);
    image[METADATA_BACKUP..METADATA_BACKUP + metadata.len()].copy_from_slice(&metadata);
    image[DATA_OFFSET..DATA_OFFSET + payload.len()].copy_from_slice(payload);
    image
}

fn geometry_bytes() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(52);
    bytes.extend(LP_METADATA_GEOMETRY_MAGIC.to_le_bytes());
    bytes.extend(52u32.to_le_bytes());
    bytes.extend([0u8; 32]);
    bytes.extend(4096u32.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(4096u32.to_le_bytes());
    let checksum = Sha256::digest(&bytes);
    bytes[8..40].copy_from_slice(&checksum);
    bytes
}

fn metadata_bytes(partition_attributes: u32) -> Vec<u8> {
    let tables = metadata_tables(partition_attributes);
    let mut header = vec![0u8; 128];
    write_u32(&mut header, 0, LP_METADATA_HEADER_MAGIC);
    write_u16(&mut header, 4, 10);
    write_u16(&mut header, 6, 0);
    write_u32(&mut header, 8, 128);
    write_u32(&mut header, 44, tables.len() as u32);
    header[48..80].copy_from_slice(&Sha256::digest(&tables));
    write_descriptor(&mut header, 80, 0, 1, 52);
    write_descriptor(&mut header, 92, 52, 2, 24);
    write_descriptor(&mut header, 104, 100, 1, 48);
    write_descriptor(&mut header, 116, 148, 1, 64);
    let checksum = Sha256::digest(&header);
    header[12..44].copy_from_slice(&checksum);
    header.extend(tables);
    header
}

fn linear_partition_metadata_bytes(
    name: &str,
    sectors: u64,
    source_sector: u64,
    image_size: u64,
) -> Vec<u8> {
    let mut tables = Vec::with_capacity(188);
    tables.extend(fixed_name::<36>(name));
    tables.extend(0u32.to_le_bytes());
    tables.extend(0u32.to_le_bytes());
    tables.extend(1u32.to_le_bytes());
    tables.extend(0u32.to_le_bytes());

    tables.extend(sectors.to_le_bytes());
    tables.extend(0u32.to_le_bytes());
    tables.extend(source_sector.to_le_bytes());
    tables.extend(0u32.to_le_bytes());

    tables.extend(fixed_name::<36>("default"));
    tables.extend(0u32.to_le_bytes());
    tables.extend(0u64.to_le_bytes());

    tables.extend(64u64.to_le_bytes());
    tables.extend(4096u32.to_le_bytes());
    tables.extend(0u32.to_le_bytes());
    tables.extend(image_size.to_le_bytes());
    tables.extend(fixed_name::<36>("super"));
    tables.extend(0u32.to_le_bytes());
    assert_eq!(tables.len(), 188);

    let mut header = vec![0u8; 128];
    write_u32(&mut header, 0, LP_METADATA_HEADER_MAGIC);
    write_u16(&mut header, 4, 10);
    write_u16(&mut header, 6, 0);
    write_u32(&mut header, 8, 128);
    write_u32(&mut header, 44, tables.len() as u32);
    header[48..80].copy_from_slice(&Sha256::digest(&tables));
    write_descriptor(&mut header, 80, 0, 1, 52);
    write_descriptor(&mut header, 92, 52, 1, 24);
    write_descriptor(&mut header, 104, 76, 1, 48);
    write_descriptor(&mut header, 116, 124, 1, 64);
    let checksum = Sha256::digest(&header);
    header[12..44].copy_from_slice(&checksum);
    header.extend(tables);
    header
}

fn metadata_tables(partition_attributes: u32) -> Vec<u8> {
    let mut tables = Vec::with_capacity(212);
    tables.extend(fixed_name::<36>("system"));
    tables.extend(partition_attributes.to_le_bytes());
    tables.extend(0u32.to_le_bytes());
    tables.extend(2u32.to_le_bytes());
    tables.extend(0u32.to_le_bytes());

    tables.extend(1u64.to_le_bytes());
    tables.extend(0u32.to_le_bytes());
    tables.extend(64u64.to_le_bytes());
    tables.extend(0u32.to_le_bytes());
    tables.extend(1u64.to_le_bytes());
    tables.extend(1u32.to_le_bytes());
    tables.extend(0u64.to_le_bytes());
    tables.extend(0u32.to_le_bytes());

    tables.extend(fixed_name::<36>("default"));
    tables.extend(0u32.to_le_bytes());
    tables.extend(0u64.to_le_bytes());

    tables.extend(64u64.to_le_bytes());
    tables.extend(4096u32.to_le_bytes());
    tables.extend(0u32.to_le_bytes());
    tables.extend((IMAGE_SIZE as u64).to_le_bytes());
    tables.extend(fixed_name::<36>("super"));
    tables.extend(0u32.to_le_bytes());
    assert_eq!(tables.len(), 212);
    tables
}

fn fixed_name<const N: usize>(name: &str) -> [u8; N] {
    let mut bytes = [0u8; N];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    bytes
}

fn write_descriptor(header: &mut [u8], at: usize, offset: u32, entries: u32, entry_size: u32) {
    write_u32(header, at, offset);
    write_u32(header, at + 4, entries);
    write_u32(header, at + 8, entry_size);
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_temp(bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("temp file");
    file.write_all(bytes).expect("write super image");
    file.as_file_mut().flush().expect("flush super image");
    file
}

fn sparse_wrap(raw: &[u8], block_size: usize) -> Vec<u8> {
    assert_eq!(raw.len() % block_size, 0);
    let blocks = raw.len() / block_size;
    let mut chunks = Vec::with_capacity(blocks);
    for block in raw.chunks_exact(block_size) {
        if block.iter().all(|byte| *byte == 0) {
            chunks.push(chunk_header(SPARSE_DONT_CARE_CHUNK, 1, &[]));
        } else {
            chunks.push(chunk_header(SPARSE_RAW_CHUNK, 1, block));
        }
    }
    let mut output = Vec::new();
    output.extend(SPARSE_MAGIC.to_le_bytes());
    output.extend(1u16.to_le_bytes());
    output.extend(0u16.to_le_bytes());
    output.extend(28u16.to_le_bytes());
    output.extend(12u16.to_le_bytes());
    output.extend((block_size as u32).to_le_bytes());
    output.extend((blocks as u32).to_le_bytes());
    output.extend((chunks.len() as u32).to_le_bytes());
    output.extend(0u32.to_le_bytes());
    for chunk in chunks {
        output.extend(chunk);
    }
    output
}

fn chunk_header(kind: u16, blocks: u32, payload: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::with_capacity(12 + payload.len());
    chunk.extend(kind.to_le_bytes());
    chunk.extend(0u16.to_le_bytes());
    chunk.extend(blocks.to_le_bytes());
    chunk.extend((12u32 + payload.len() as u32).to_le_bytes());
    chunk.extend(payload);
    chunk
}
