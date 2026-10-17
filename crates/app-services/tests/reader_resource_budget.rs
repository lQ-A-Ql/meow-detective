use app_services::{
    clear_e01_reader_cache_for_case, open_e01_reader_cached, resource_budget::global_pool,
    source_runtime::open_factory,
};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::Mutex,
};
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn raw_readers_own_permits_after_the_factory_has_dropped() {
    let _lock = SERIAL.lock().unwrap();
    let before = global_pool().snapshot().0;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, [3; 8192]).unwrap();
    let factory = open_factory(&path, &domain::DataSourceKind::Raw).unwrap();
    let mut a = factory.open_reader().unwrap();
    let b = factory.open_reader().unwrap();
    drop(factory);
    assert_eq!(global_pool().snapshot().0.readers, before.readers + 2);
    let mut bytes = [0; 10];
    a.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes, [3; 10]);
    drop(a);
    assert_eq!(global_pool().snapshot().0.readers, before.readers + 1);
    drop(b);
    assert_eq!(global_pool().snapshot().0, before);
}
#[test]
fn e01_metadata_is_charged_once_and_survives_eviction_until_the_last_reader_drops() {
    let _lock = SERIAL.lock().unwrap();
    let before = global_pool().snapshot().0;
    let path = testing::fixtures::tiny_e01_image();
    let case = "resource-sharing";
    let mut a = open_e01_reader_cached(&path, case).unwrap();
    let first = global_pool().snapshot().0;
    let mut b = open_e01_reader_cached(&path, case).unwrap();
    let second = global_pool().snapshot().0;
    assert_eq!(first.metadata, second.metadata);
    assert_eq!(second.readers, before.readers + 2);
    clear_e01_reader_cache_for_case(case);
    assert_eq!(global_pool().snapshot().0.metadata, first.metadata);
    let mut x = [0; 8];
    let mut y = [0; 8];
    a.seek(SeekFrom::Start(0)).unwrap();
    b.seek(SeekFrom::Start(4)).unwrap();
    a.read_exact(&mut x).unwrap();
    b.read_exact(&mut y).unwrap();
    assert_ne!(x, y);
    drop(a);
    assert_eq!(global_pool().snapshot().0.metadata, first.metadata);
    drop(b);
    assert_eq!(global_pool().snapshot().0, before);
}
#[test]
fn failed_open_releases_the_reader_and_handle_reservation() {
    let _lock = SERIAL.lock().unwrap();
    let before = global_pool().snapshot().0;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, [0; 512]).unwrap();
    let factory = open_factory(&path, &domain::DataSourceKind::Raw).unwrap();
    let planned = global_pool().snapshot().0;
    std::fs::remove_file(path).unwrap();
    assert!(factory.open_reader().is_err());
    assert_eq!(global_pool().snapshot().0, planned);
    drop(factory);
    assert_eq!(global_pool().snapshot().0, before);
}

#[test]
fn cache_miss_returns_busy_instead_of_waiting_on_an_active_import_workspace() {
    use app_services::resource_budget::{Resources, MIB};
    let _lock = SERIAL.lock().unwrap();
    let before = global_pool().snapshot().0;
    let workspace = global_pool()
        .acquire(
            Resources {
                bytes: 1024 * MIB,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let path = testing::fixtures::tiny_e01_image();
    let case = "resource-miss";
    let result = open_e01_reader_cached(&path, case);
    assert_eq!(result.err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    drop(workspace);
    let reader = open_e01_reader_cached(&path, case).unwrap();
    drop(reader);
    clear_e01_reader_cache_for_case(case);
    assert_eq!(global_pool().snapshot().0, before);
}
