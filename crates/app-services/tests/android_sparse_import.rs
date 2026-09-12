use std::io::Write;

use app_services::{datasource_service, import_pipeline};
use domain::DataSourceKind;
use image_android::{AndroidSparseReader, SPARSE_MAGIC, SPARSE_RAW_CHUNK};
use tempfile::NamedTempFile;

const SPARSE_BLOCK_SIZE: usize = 4096;

#[test]
fn android_sparse_images_reach_the_matching_filesystem_reader() {
    assert_sparse_file_read(
        testing::builders::ext4::minimal_ext4_image(),
        datasource_service::ImageFilesystemKind::Ext4,
        "test.txt",
        b"Hello World",
    );
    assert_sparse_file_read(
        testing::builders::f2fs::minimal_f2fs_image(),
        datasource_service::ImageFilesystemKind::F2fs,
        "hello.txt",
        b"Hello F2FS",
    );
    assert_sparse_file_read(
        testing::builders::erofs::minimal_erofs_image(),
        datasource_service::ImageFilesystemKind::Erofs,
        "hello.txt",
        b"Hello EROFS!",
    );
}

#[test]
fn sparse_precheck_uses_logical_size_without_expansion() {
    let sparse = write_sparse_image(&testing::builders::ext4::minimal_ext4_image());
    let logical_size = AndroidSparseReader::open(sparse.path())
        .expect("open sparse image")
        .logical_size();

    let result = app_services::import_precheck::pre_import_check(
        sparse.path(),
        &DataSourceKind::AndroidSparse,
    );
    assert!(result.errors.is_empty());
    assert_eq!(result.plan.total_size, logical_size);
    assert!(result.plan.total_size > sparse.as_file().metadata().expect("sparse metadata").len());
}

fn assert_sparse_file_read(
    logical_image: Vec<u8>,
    expected_kind: datasource_service::ImageFilesystemKind,
    path: &str,
    expected_content: &[u8],
) {
    let sparse = write_sparse_image(&logical_image);
    let mut reader = AndroidSparseReader::open(sparse.path()).expect("open sparse image");
    assert!(
        sparse.as_file().metadata().expect("sparse metadata").len() < reader.logical_size(),
        "the test source must remain smaller than its logical address space"
    );
    let mut sparse_tail = [0xff; 1];
    reader
        .read_range(logical_image.len() as u64, &mut sparse_tail)
        .expect("read sparse zero tail");
    assert_eq!(sparse_tail, [0]);

    let probe = datasource_service::detect_image_filesystem(&mut reader).expect("probe sparse");
    let candidate = probe
        .candidates
        .iter()
        .find(|candidate| candidate.kind == expected_kind)
        .expect("matching filesystem candidate");
    let partition_index = candidate.partition_index.unwrap_or(0);
    let partition = probe
        .partitions
        .iter()
        .find(|partition| partition.index == partition_index)
        .expect("candidate partition metadata");
    let work = import_pipeline::build_partition_work(
        sparse.path(),
        &DataSourceKind::AndroidSparse,
        partition_index,
        &partition.name,
        &partition.kind_label,
        &probe.candidates,
    )
    .expect("build filesystem work from sparse image");

    assert_eq!(
        work.fs
            .read_file_range(path, 0, expected_content.len())
            .expect("read file range through application filesystem work"),
        expected_content
    );
}

fn write_sparse_image(logical_image: &[u8]) -> NamedTempFile {
    assert_eq!(logical_image.len() % SPARSE_BLOCK_SIZE, 0);
    let raw_blocks = u32::try_from(logical_image.len() / SPARSE_BLOCK_SIZE).expect("block count");
    let total_blocks = raw_blocks.checked_add(1).expect("total block count");
    let chunk_size = u32::try_from(12 + logical_image.len()).expect("chunk size");
    let mut bytes = Vec::with_capacity(52 + logical_image.len());
    bytes.extend(SPARSE_MAGIC.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(0u16.to_le_bytes());
    bytes.extend(28u16.to_le_bytes());
    bytes.extend(12u16.to_le_bytes());
    bytes.extend((SPARSE_BLOCK_SIZE as u32).to_le_bytes());
    bytes.extend(total_blocks.to_le_bytes());
    bytes.extend(2u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(SPARSE_RAW_CHUNK.to_le_bytes());
    bytes.extend(0u16.to_le_bytes());
    bytes.extend(raw_blocks.to_le_bytes());
    bytes.extend(chunk_size.to_le_bytes());
    bytes.extend(logical_image);
    bytes.extend(image_android::SPARSE_DONT_CARE_CHUNK.to_le_bytes());
    bytes.extend(0u16.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(12u32.to_le_bytes());

    let mut file = NamedTempFile::new().expect("create sparse image");
    file.write_all(&bytes).expect("write sparse image");
    file.flush().expect("flush sparse image");
    file
}
