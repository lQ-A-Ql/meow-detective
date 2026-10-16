use super::memory_hard_limit_exceeded_for_rss;
use super::process_counters::{ProcessCounterSnapshot, ProcessIoCounters};
use std::time::Duration;

#[test]
fn workflow_counters_keep_units_and_do_not_confuse_transfer_bytes_with_disk_bytes() {
    let start = ProcessCounterSnapshot {
        user_cpu_ns: Some(100),
        kernel_cpu_ns: Some(200),
        io: Some(ProcessIoCounters {
            read_operations: 1,
            write_operations: 2,
            read_bytes: 10,
            write_bytes: 20,
            ..Default::default()
        }),
        handles: Some(10),
    };
    let end = ProcessCounterSnapshot {
        user_cpu_ns: Some(300),
        kernel_cpu_ns: Some(400),
        io: Some(ProcessIoCounters {
            read_operations: 4,
            write_operations: 8,
            read_bytes: 30,
            write_bytes: 70,
            ..Default::default()
        }),
        handles: Some(11),
    };
    let usage = start.usage_until(end, Duration::from_nanos(200));
    assert_eq!(usage.user_cpu_ns, Some(200));
    assert_eq!(usage.kernel_cpu_ns, Some(200));
    assert_eq!(usage.cpu_core_equivalents, Some(2.0));
    assert_eq!(usage.process_read_transfer_bytes, Some(20));
    assert_eq!(usage.process_write_transfer_bytes, Some(50));
    assert_eq!(usage.process_read_operations, Some(3));
    assert_eq!(usage.handles_at_start, Some(10));
    assert_eq!(usage.handles_at_end, Some(11));
}

#[test]
fn unavailable_decreasing_and_overflowing_counters_are_not_fabricated_as_zero() {
    let absent = ProcessCounterSnapshot::default()
        .usage_until(ProcessCounterSnapshot::default(), Duration::ZERO);
    assert_eq!(absent.user_cpu_ns, None);
    assert_eq!(absent.process_read_transfer_bytes, None);
    assert_eq!(absent.cpu_core_equivalents, None);
    let start = ProcessCounterSnapshot {
        user_cpu_ns: Some(10),
        ..Default::default()
    };
    let end = ProcessCounterSnapshot {
        user_cpu_ns: Some(9),
        kernel_cpu_ns: Some(u64::MAX),
        ..Default::default()
    };
    assert_eq!(
        start.usage_until(end, Duration::from_secs(1)).user_cpu_ns,
        None
    );
    let zero = ProcessCounterSnapshot {
        user_cpu_ns: Some(0),
        kernel_cpu_ns: Some(0),
        io: Some(Default::default()),
        ..Default::default()
    };
    let overflow = ProcessCounterSnapshot {
        user_cpu_ns: Some(u64::MAX),
        kernel_cpu_ns: Some(1),
        ..Default::default()
    };
    assert_eq!(
        zero.usage_until(overflow, Duration::from_secs(1))
            .cpu_core_equivalents,
        None
    );
    assert_eq!(
        zero.usage_until(zero, Duration::from_secs(1))
            .process_read_transfer_bytes,
        Some(0)
    );
    assert_eq!(
        zero.usage_until(zero, Duration::ZERO).cpu_core_equivalents,
        None
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_current_process_provider_exposes_supported_counters() {
    let snapshot = ProcessCounterSnapshot::capture();
    assert!(snapshot.user_cpu_ns.is_some());
    assert!(snapshot.kernel_cpu_ns.is_some());
    assert!(snapshot.io.is_some());
    assert!(snapshot.handles.is_some_and(|handles| handles > 0));
}

#[test]
fn hard_limit_requires_a_nonzero_rss_at_or_above_the_limit() {
    assert!(memory_hard_limit_exceeded_for_rss(3, 2));
    assert!(memory_hard_limit_exceeded_for_rss(2, 2));
    assert!(!memory_hard_limit_exceeded_for_rss(1, 2));
    assert!(!memory_hard_limit_exceeded_for_rss(0, 0));
}
