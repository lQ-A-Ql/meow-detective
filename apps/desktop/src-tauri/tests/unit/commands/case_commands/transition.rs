use std::time::Duration;

use app_services::case_service;
use uuid::Uuid;

use super::super::transition::{
    active_case_identity, begin_active_case_transition, clear_active_case_if_matches,
};
use crate::state::AppState;

#[test]
fn rollback_restores_previous_active_case() {
    let parent = test_parent("rollback");
    let first = case_service::create_case(&parent, "first", Some("tester")).unwrap();
    let first_id = first.meta.id.0.clone();
    let second = case_service::create_case(&parent, "second", Some("tester")).unwrap();
    let second_id = second.meta.id.0.clone();
    let state = AppState::default();
    *state.active_case.lock().unwrap() = Some(first);

    let transition =
        begin_active_case_transition(&state, second, Duration::from_millis(100)).unwrap();
    assert_eq!(
        active_case_identity(&state).unwrap().unwrap().case_id,
        second_id
    );

    transition.rollback(&state, Duration::from_millis(100));

    assert_eq!(
        active_case_identity(&state).unwrap().unwrap().case_id,
        first_id
    );
    std::fs::remove_dir_all(parent).ok();
}

#[test]
fn conditional_clear_does_not_remove_a_different_case() {
    let parent = test_parent("conditional-clear");
    let first = case_service::create_case(&parent, "first", Some("tester")).unwrap();
    let first_identity = {
        let state = AppState::default();
        *state.active_case.lock().unwrap() = Some(first);
        active_case_identity(&state).unwrap().unwrap()
    };
    let second = case_service::create_case(&parent, "second", Some("tester")).unwrap();
    let second_id = second.meta.id.0.clone();
    let state = AppState::default();
    *state.active_case.lock().unwrap() = Some(second);

    assert!(!clear_active_case_if_matches(&state, &first_identity).unwrap());
    assert_eq!(
        active_case_identity(&state).unwrap().unwrap().case_id,
        second_id
    );
    std::fs::remove_dir_all(parent).ok();
}

fn test_parent(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "Meow_Detective-case-transition-{label}-{}",
        Uuid::new_v4()
    ))
}
