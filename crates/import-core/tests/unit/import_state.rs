use super::*;

#[test]
fn test_import_state_new() {
    let state = ImportState::new("job-1".to_string(), "ds-1".to_string());
    assert_eq!(state.job_id, "job-1");
    assert_eq!(state.data_source_id, "ds-1");
    assert_eq!(state.phase, ImportPhase::Classifying);
    assert_eq!(state.processed_files, 0);
    assert_eq!(state.total_files, 0);
    assert!(state.errors.is_empty());
}

#[test]
fn test_import_state_progress() {
    let mut state = ImportState::new("job-1".to_string(), "ds-1".to_string());
    state.update_progress(50, 100);
    assert_eq!(state.processed_files, 50);
    assert_eq!(state.total_files, 100);
    assert_eq!(state.progress_percent(), 50);
}

#[test]
fn test_import_state_progress_zero() {
    let state = ImportState::new("job-1".to_string(), "ds-1".to_string());
    assert_eq!(state.progress_percent(), 0);
}

#[test]
fn test_import_state_can_resume() {
    let mut state = ImportState::new("job-1".to_string(), "ds-1".to_string());
    assert!(!state.can_resume());

    state.set_phase(ImportPhase::Paused);
    assert!(state.can_resume());

    state.set_phase(ImportPhase::Failed);
    assert!(state.can_resume());

    state.set_phase(ImportPhase::Completed);
    assert!(!state.can_resume());
}

#[test]
fn test_import_state_errors() {
    let mut state = ImportState::new("job-1".to_string(), "ds-1".to_string());
    state.add_error("test error".to_string(), Some("/path".to_string()));
    assert_eq!(state.errors.len(), 1);
    assert_eq!(state.errors[0].message, "test error");
}

#[test]
fn test_import_plan_sequential() {
    let plan = ImportPlan::new(ImportStrategy::Sequential, 1000, 1024 * 1024);
    assert_eq!(plan.strategy, ImportStrategy::Sequential);
    assert!(plan.estimated_time_secs > 0);
}

#[test]
fn test_import_plan_parallel() {
    let plan = ImportPlan::new(
        ImportStrategy::Parallel { workers: 4 },
        10000,
        1024 * 1024 * 100,
    );
    assert!(plan.estimated_time_secs > 0);
}

#[test]
fn test_import_plan_streaming() {
    let plan = ImportPlan::new(
        ImportStrategy::Streaming,
        1000,
        10 * 1024 * 1024 * 1024, // 10GB
    );
    assert!(plan.estimated_time_secs > 0);
}

#[test]
fn test_import_state_serialization() {
    let state = ImportState::new("job-1".to_string(), "ds-1".to_string());
    let json = serde_json::to_string(&state).unwrap();
    let deserialized: ImportState = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.job_id, "job-1");
}
