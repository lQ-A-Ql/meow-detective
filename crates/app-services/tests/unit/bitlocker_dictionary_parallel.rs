use super::*;

#[test]
fn worker_count_is_bounded_and_never_zero() {
    assert_eq!(worker_count(0), 1);
    assert!(
        worker_count(10_000)
            <= std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
    );
    assert!(worker_count(3) <= 3);
}

#[test]
fn parallel_attacks_share_one_cpu_pool() {
    let first = DictionaryWorkers::new().expect("workers");
    let second = DictionaryWorkers::new().expect("workers");
    assert!(Arc::ptr_eq(&first.pool, &second.pool));
    assert_eq!(first.pool.current_num_threads(), worker_count(usize::MAX));
}

#[test]
fn cancelled_batch_does_not_count_unstarted_candidates() {
    let workers = DictionaryWorkers::new().expect("workers");
    let cancel = AtomicBool::new(true);
    let result = workers.try_batch(&[], vec![Zeroizing::new("unused".to_string())], &cancel);
    assert_eq!(result.attempted, 0);
    assert!(result
        .verified
        .expect("cancelled without metadata access")
        .is_none());
}

#[test]
fn failed_batch_counts_attempts_before_reporting_the_error() {
    let workers = DictionaryWorkers::new().expect("workers");
    let cancel = AtomicBool::new(false);
    let result = workers.try_batch(&[], vec![Zeroizing::new("unused".to_string())], &cancel);
    assert_eq!(result.attempted, 1);
    assert!(matches!(
        result.verified,
        Err(BitLockerError::MetadataUnreadable { .. })
    ));
}
