//! Admission, lifecycle, and eviction contract for the hex search registry.

use forensics_desktop_lib::state::hex_search::{
    HexSearchAdmission, HexSearchJobRegistry, MAX_ACTIVE_HEX_SEARCHES, MAX_HEX_SEARCH_JOBS,
};
use transport::dto::{HexSearchModeDto, HexSearchPhaseDto};

fn start(registry: &HexSearchJobRegistry, case: &str, file: &str) -> String {
    match registry.admit(case, file, HexSearchModeDto::Hex, &[0x41]) {
        HexSearchAdmission::Start(status) => status.task_id,
        other => panic!("expected start for {file}, got {other:?}"),
    }
}

fn complete(registry: &HexSearchJobRegistry, case: &str, file: &str) {
    let current = registry.get(case, file).expect("entry must exist");
    assert!(
        registry.update_if_current(case, file, &current.task_id, |status| {
            status.phase = HexSearchPhaseDto::Completed;
        })
    );
}

#[test]
fn identical_active_query_is_reused() {
    let registry = HexSearchJobRegistry::default();
    let started = start(&registry, "case", "ds:a:1");
    let HexSearchAdmission::Reuse(reused) =
        registry.admit("case", "ds:a:1", HexSearchModeDto::Hex, &[0x41])
    else {
        panic!("identical query must reuse");
    };
    assert_eq!(reused.task_id, started);
    assert_eq!(reused.phase, HexSearchPhaseDto::Queued);
}

#[test]
fn different_query_replaces_the_active_search() {
    let registry = HexSearchJobRegistry::default();
    let previous = start(&registry, "case", "ds:a:1");
    let HexSearchAdmission::Replace {
        cancelled_task_id,
        status,
    } = registry.admit("case", "ds:a:1", HexSearchModeDto::Ascii, b"AB")
    else {
        panic!("different query must replace");
    };
    assert_eq!(cancelled_task_id, previous);
    assert_ne!(status.task_id, previous);
    assert_eq!(status.phase, HexSearchPhaseDto::Queued);
    assert!(
        !registry.update_if_current("case", "ds:a:1", &previous, |status| {
            status.scanned_bytes = 99;
        })
    );
    assert_eq!(
        registry.get("case", "ds:a:1").unwrap().task_id,
        status.task_id
    );
}

#[test]
fn admission_is_busy_at_the_active_limit() {
    let registry = HexSearchJobRegistry::default();
    for index in 0..MAX_ACTIVE_HEX_SEARCHES {
        start(&registry, "case", &format!("ds:a:{index}"));
    }
    assert_eq!(
        registry.admit("case", "ds:a:busy", HexSearchModeDto::Hex, &[0x41]),
        HexSearchAdmission::Busy
    );
    complete(&registry, "case", "ds:a:0");
    assert!(matches!(
        registry.admit("case", "ds:a:busy", HexSearchModeDto::Hex, &[0x41]),
        HexSearchAdmission::Start(_)
    ));
}

#[test]
fn restarting_a_terminal_entry_respects_the_active_limit_without_mutating_history() {
    let registry = HexSearchJobRegistry::default();
    let old = start(&registry, "case", "ds:old:entry");
    complete(&registry, "case", "ds:old:entry");
    for index in 0..MAX_ACTIVE_HEX_SEARCHES {
        start(&registry, "case", &format!("ds:active:{index}"));
    }
    assert_eq!(
        registry.admit("case", "ds:old:entry", HexSearchModeDto::Hex, &[0x42]),
        HexSearchAdmission::Busy
    );
    let unchanged = registry.get("case", "ds:old:entry").unwrap();
    assert_eq!(unchanged.task_id, old);
    assert_eq!(unchanged.phase, HexSearchPhaseDto::Completed);
    complete(&registry, "case", "ds:active:0");
    let HexSearchAdmission::Start(restarted) =
        registry.admit("case", "ds:old:entry", HexSearchModeDto::Hex, &[0x42])
    else {
        panic!("released capacity must admit the restarted search");
    };
    assert_ne!(restarted.task_id, old);
    assert!(
        !registry.update_if_current("case", "ds:old:entry", &old, |status| status.phase =
            HexSearchPhaseDto::Completed)
    );
}

#[test]
fn stale_task_updates_are_ignored() {
    let registry = HexSearchJobRegistry::default();
    let current = start(&registry, "case", "ds:a:1");
    assert!(
        !registry.update_if_current("case", "ds:a:1", "hex-search:other", |status| {
            status.scanned_bytes = 1;
        })
    );
    assert!(!registry.mark_cancelling("case", "ds:a:1", "hex-search:other"));
    assert_eq!(
        registry.get("case", "ds:a:1").unwrap().phase,
        HexSearchPhaseDto::Queued
    );
    assert!(registry.mark_cancelling("case", "ds:a:1", &current));
    assert_eq!(
        registry.get("case", "ds:a:1").unwrap().phase,
        HexSearchPhaseDto::Cancelling
    );
}

#[test]
fn reconcile_settles_only_stopped_non_terminal_tasks() {
    let registry = HexSearchJobRegistry::default();
    let first = start(&registry, "case", "ds:a:1");
    start(&registry, "case", "ds:a:2");
    registry.mark_cancelling("case", "ds:a:1", &first);
    let reconciled = registry
        .reconcile("case", "ds:a:1", |_| false)
        .expect("entry must exist");
    assert_eq!(reconciled.phase, HexSearchPhaseDto::Cancelled);
    assert_eq!(reconciled.error, None);

    let stalled = registry
        .reconcile("case", "ds:a:2", |_| false)
        .expect("entry must exist");
    assert_eq!(stalled.phase, HexSearchPhaseDto::Failed);
    assert_eq!(stalled.error.as_deref(), Some("task_interrupted"));

    complete(&registry, "case", "ds:a:1");
    let terminal = registry
        .reconcile("case", "ds:a:1", |_| false)
        .expect("entry must exist");
    assert_eq!(terminal.phase, HexSearchPhaseDto::Completed);
}

#[test]
fn oldest_terminal_entry_is_evicted_for_new_admissions() {
    let registry = HexSearchJobRegistry::default();
    for index in 0..MAX_HEX_SEARCH_JOBS {
        let file = format!("ds:a:{index}");
        start(&registry, "case", &file);
        complete(&registry, "case", &file);
    }
    assert!(registry.get("case", "ds:a:0").is_some());
    start(&registry, "case", "ds:b:new");
    assert!(
        registry.get("case", "ds:a:0").is_none(),
        "oldest terminal entry must be evicted"
    );
    assert!(registry.get("case", "ds:a:1").is_some());
    assert!(registry.get("case", "ds:b:new").is_some());
}

#[test]
fn active_entries_survive_eviction() {
    let registry = HexSearchJobRegistry::default();
    let active = start(&registry, "case", "ds:a:active");
    for index in 0..MAX_HEX_SEARCH_JOBS - 1 {
        let file = format!("ds:a:{index}");
        start(&registry, "case", &file);
        complete(&registry, "case", &file);
    }
    start(&registry, "case", "ds:b:new");
    assert_eq!(registry.get("case", "ds:a:active").unwrap().task_id, active);
    assert!(registry.get("case", "ds:a:0").is_none());
}
