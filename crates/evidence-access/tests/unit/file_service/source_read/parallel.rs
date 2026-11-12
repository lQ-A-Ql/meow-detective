use super::*;

#[test]
fn parallel_policy_requires_large_file_and_two_cpus() {
    assert_eq!(parallel_worker_count(512 * 1024 * 1024 - 1, 16, 0, 0), 1);
    assert_eq!(parallel_worker_count(512 * 1024 * 1024, 1, 0, 0), 1);
    assert_eq!(parallel_worker_count(512 * 1024 * 1024, 2, 0, 0), 2);
}

#[test]
fn parallel_policy_falls_back_when_memory_headroom_is_insufficient() {
    assert_eq!(parallel_worker_count(1024 * 1024 * 1024, 16, 4000, 4096), 1);
    assert_eq!(parallel_worker_count(1024 * 1024 * 1024, 16, 3968, 4096), 2);
}
