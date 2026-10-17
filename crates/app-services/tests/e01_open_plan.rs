use app_services::{clear_e01_reader_cache, open_e01_reader_cached};
use std::io::{Read, Seek, SeekFrom};

#[test]
fn cached_e01_plan_returns_independent_handles_after_template_eviction() {
    let fixture = testing::fixtures::tiny_e01_image();
    clear_e01_reader_cache();
    let mut first = open_e01_reader_cached(&fixture, "plan-test").unwrap();
    first.seek(SeekFrom::Start(12)).unwrap();
    let mut value = [0u8; 8];
    first.read_exact(&mut value).unwrap();
    let mut second = open_e01_reader_cached(&fixture, "plan-test").unwrap();
    let mut head = [0u8; 8];
    second.read_exact(&mut head).unwrap();
    clear_e01_reader_cache();
    assert_eq!(first.stream_position().unwrap(), 20);
    assert_eq!(second.stream_position().unwrap(), 8);
    assert_ne!(value, head);
    clear_e01_reader_cache();
}

#[path = "support/import_ntfs_fixture.rs"]
mod fixture;

#[test]
fn factory_readers_match_raw_bytes_during_overlapping_random_reads() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("oracle.E01");
    let bytes = fixture::image(128);
    fixture::write_e01(&path, &bytes).unwrap();
    let factory =
        app_services::source_runtime::open_factory(&path, &domain::DataSourceKind::E01).unwrap();
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let factory = &factory;
            let bytes = &bytes;
            scope.spawn(move || {
                let mut reader = factory.open_reader().unwrap();
                for step in 0..100 {
                    let offset = (step * 7919 + worker * 4093) % (bytes.len() - 9000);
                    let mut actual = vec![0; 9000];
                    reader.seek(SeekFrom::Start(offset as u64)).unwrap();
                    reader.read_exact(&mut actual).unwrap();
                    assert_eq!(actual, bytes[offset..offset + actual.len()]);
                }
            });
        }
    });
}

#[test]
fn e01_plan_rejects_a_changed_segment_before_reusing_its_chunk_table() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("changed.E01");
    fixture::write_e01(&path, &fixture::image(32)).unwrap();
    let plan = image_e01::E01Reader::open(&path)
        .unwrap()
        .open_plan()
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(64)
        .unwrap();
    assert!(plan.open_reader().is_err());
    assert!(plan.validate_snapshot().is_err());
}
