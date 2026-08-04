use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use image_android::{
    AndroidSparseReader, SparseImage, SparseImageError, SPARSE_CRC32_CHUNK, SPARSE_DONT_CARE_CHUNK,
    SPARSE_FILL_CHUNK, SPARSE_MAGIC, SPARSE_RAW_CHUNK,
};
use tempfile::NamedTempFile;

const BLOCK_SIZE: u32 = 4;

#[test]
fn parses_and_reads_all_data_chunk_kinds() {
    let bytes = sparse_image(
        &[
            raw_chunk(b"ABCD"),
            fill_chunk(2, [b'X', b'Y', b'Z', b'!']),
            dont_care_chunk(1),
            crc_chunk(0x1234_5678),
        ],
        4,
    );
    let mut source = Cursor::new(bytes);
    let image = SparseImage::parse(&mut source).expect("valid sparse image");
    assert_eq!(image.logical_size(), 16);
    assert_eq!(image.chunks().len(), 3);
    assert_eq!(image.checksums()[0].value, 0x1234_5678);

    let file = write_temp(&sparse_image(
        &[
            raw_chunk(b"ABCD"),
            fill_chunk(2, [b'X', b'Y', b'Z', b'!']),
            dont_care_chunk(100),
        ],
        103,
    ));
    let mut reader = AndroidSparseReader::open(file.path()).expect("open sparse reader");
    let mut output = [0u8; 16];
    reader.read_exact(&mut output).expect("read logical image");
    assert_eq!(&output[..4], b"ABCD");
    assert_eq!(&output[4..12], b"XYZ!XYZ!");
    assert_eq!(&output[12..], &[0, 0, 0, 0]);
}

#[test]
fn range_reads_cross_chunk_boundaries_without_expanding_source() {
    let file = write_temp(&sparse_image(
        &[
            raw_chunk(b"ABCD"),
            fill_chunk(2, [b'1', b'2', b'3', b'4']),
            dont_care_chunk(100),
        ],
        103,
    ));
    let source_size = file.as_file().metadata().expect("metadata").len();
    let mut reader = AndroidSparseReader::open(file.path()).expect("open sparse reader");
    let mut output = [0u8; 8];
    reader.read_range(2, &mut output).expect("range read");
    assert_eq!(&output, b"CD123412");
    assert!(source_size < reader.logical_size());
    reader
        .seek(SeekFrom::Start(reader.logical_size() - 2))
        .expect("seek");
    let mut tail = [0u8; 4];
    assert_eq!(reader.read(&mut tail).expect("tail read"), 2);
    assert_eq!(&tail[..2], &[0, 0]);
}

#[test]
fn rejects_bad_magic_and_chunk_length() {
    let mut bad_magic = sparse_image(&[raw_chunk(b"ABCD")], 1);
    bad_magic[0] = 0;
    assert!(matches!(
        SparseImage::parse(&mut Cursor::new(bad_magic)),
        Err(SparseImageError::InvalidHeader(_))
    ));

    let mut bad_chunk = sparse_image(&[raw_chunk(b"ABCD")], 1);
    bad_chunk[36..40].copy_from_slice(&13u32.to_le_bytes());
    assert!(matches!(
        SparseImage::parse(&mut Cursor::new(bad_chunk)),
        Err(SparseImageError::InvalidChunk { .. })
    ));
}

#[test]
fn rejects_chunks_that_exceed_declared_logical_blocks() {
    let bytes = sparse_image(&[raw_chunk(b"ABCD")], 0);
    let error = SparseImage::parse(&mut Cursor::new(bytes)).expect_err("invalid block count");
    assert!(matches!(error, SparseImageError::InvalidChunk { .. }));
}

fn raw_chunk(data: &[u8]) -> Vec<u8> {
    assert_eq!(data.len() % BLOCK_SIZE as usize, 0);
    chunk_header(
        SPARSE_RAW_CHUNK,
        (data.len() / BLOCK_SIZE as usize) as u32,
        data.len() as u32,
    )
    .into_iter()
    .chain(data.iter().copied())
    .collect()
}

fn fill_chunk(blocks: u32, pattern: [u8; 4]) -> Vec<u8> {
    chunk_header(SPARSE_FILL_CHUNK, blocks, 4)
        .into_iter()
        .chain(pattern)
        .collect()
}

fn dont_care_chunk(blocks: u32) -> Vec<u8> {
    chunk_header(SPARSE_DONT_CARE_CHUNK, blocks, 0)
}

fn crc_chunk(value: u32) -> Vec<u8> {
    chunk_header(SPARSE_CRC32_CHUNK, 1, 4)
        .into_iter()
        .chain(value.to_le_bytes())
        .collect()
}

fn chunk_header(kind: u16, blocks: u32, payload_size: u32) -> Vec<u8> {
    let total_size = 12 + payload_size;
    kind.to_le_bytes()
        .into_iter()
        .chain(0u16.to_le_bytes())
        .chain(blocks.to_le_bytes())
        .chain(total_size.to_le_bytes())
        .collect()
}

fn sparse_image(chunks: &[Vec<u8>], total_blocks: u32) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend(SPARSE_MAGIC.to_le_bytes());
    output.extend(1u16.to_le_bytes());
    output.extend(0u16.to_le_bytes());
    output.extend(28u16.to_le_bytes());
    output.extend(12u16.to_le_bytes());
    output.extend(BLOCK_SIZE.to_le_bytes());
    output.extend(total_blocks.to_le_bytes());
    output.extend((chunks.len() as u32).to_le_bytes());
    output.extend(0u32.to_le_bytes());
    for chunk in chunks {
        output.extend(chunk);
    }
    output
}

fn write_temp(bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("temp file");
    file.write_all(bytes).expect("write sparse image");
    file.as_file_mut().flush().expect("flush sparse image");
    file
}
