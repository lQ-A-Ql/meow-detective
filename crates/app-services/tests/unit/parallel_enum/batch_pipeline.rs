use super::*;

fn batch(start: u64, count: u64) -> MftBatch {
    MftBatch {
        start_record: start,
        scanned_records: count,
        logical_bytes: count * 1024,
        records: Vec::new(),
    }
}

#[test]
fn window_reorders_batches_and_releases_capacity_after_ordered_consume() {
    let window = OrderedMftWindow::new(2);
    assert!(window.reserve(0));
    assert!(window.reserve(10));
    assert!(!window.reserve(20));
    window.submit(batch(10, 10)).unwrap();
    assert!(window.take_next().is_none());
    window.submit(batch(0, 10)).unwrap();
    assert_eq!(window.take_next().unwrap().start_record, 0);
    assert_eq!(window.take_next().unwrap().start_record, 10);
    assert_eq!(window.outstanding(), 0);
    assert!(window.reserve(20));
}

#[test]
fn duplicate_batches_are_rejected_without_losing_the_first_batch() {
    let window = OrderedMftWindow::new(2);
    assert!(window.reserve(0));
    window.submit(batch(0, 5)).unwrap();
    let duplicate = window.submit(batch(0, 5));
    assert!(duplicate.is_err());
    assert_eq!(window.take_next().unwrap().scanned_records, 5);
}
