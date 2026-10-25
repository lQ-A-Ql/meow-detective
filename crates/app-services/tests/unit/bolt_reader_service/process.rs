use super::*;

#[test]
fn worker_deadline_terminates_real_database_read() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../containers-bolt/tests/fixtures/go-bbolt.db"
    ))
    .unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("input.db");
    std::fs::write(&path, &bytes).unwrap();
    let mut request = crate::bolt_reader_service::profile::request(
        crate::bolt_reader_service::BoltReadMode::EtcdMetadata,
        bytes.len() as u64,
    );
    request.input_path = path.to_str().unwrap().into();
    request.input_sha256 = infrastructure::hashing::sha256_bytes(&bytes);
    request.limits.max_runtime_millis = 1;
    let started = Instant::now();
    let result = execute(&request, &AtomicBool::new(false));
    assert!(matches!(
        result,
        Err(BoltReadError::Timeout | BoltReadError::WorkerFailed)
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
    std::fs::remove_file(path).expect("terminated worker releases input handles");
}
