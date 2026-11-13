use persistence_fjall::instrumentation::{self, ProcessMeasurement};

#[test]
fn process_measurement_collects_workers_without_resetting_local_counters() {
    instrumentation::enable();
    instrumentation::reset();
    let measurement = ProcessMeasurement::start().unwrap();
    assert!(ProcessMeasurement::start().is_err());
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..25 {
                    instrumentation::record_path_normalization();
                }
            });
        }
    });
    assert_eq!(measurement.snapshot().path_normalizations, 100);
    assert_eq!(instrumentation::snapshot().path_normalizations, 0);
    instrumentation::record_search_index_open();
    assert_eq!(measurement.snapshot().search_index_opens, 1);
    assert_eq!(instrumentation::snapshot().search_index_opens, 1);
    instrumentation::reset();
    assert_eq!(measurement.snapshot().search_index_opens, 1);
    drop(measurement);
    let fresh = ProcessMeasurement::start().unwrap();
    assert_eq!(fresh.snapshot().path_normalizations, 0);
    instrumentation::disable();
}
