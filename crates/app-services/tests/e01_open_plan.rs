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
    assert_eq!(second.stream_position().unwrap(), 8);
    assert_ne!(value, head);
    clear_e01_reader_cache();
}
