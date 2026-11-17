use super::for_entry;
use std::sync::Arc;

#[test]
fn callers_for_the_same_loaded_entry_share_one_gate() {
    let entry = callers_for_the_same_loaded_entry_share_one_gate as *const () as usize;
    let first = for_entry(entry).expect("first gate");
    let second = for_entry(entry).expect("second gate");
    assert!(Arc::ptr_eq(&first, &second));
    let held = first.lock().expect("hold first gate");
    assert!(second.try_lock().is_err());
    drop(held);
    assert!(second.try_lock().is_ok());
}

#[test]
fn distinct_entries_can_run_independently() {
    let first = for_entry(distinct_entries_can_run_independently as *const () as usize)
        .expect("first gate");
    let second = for_entry(for_entry as *const () as usize).expect("second gate");
    assert!(!Arc::ptr_eq(&first, &second));
    let _held = first.lock().expect("hold first gate");
    assert!(second.try_lock().is_ok());
}
