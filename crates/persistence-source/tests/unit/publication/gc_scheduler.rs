use super::{enqueue_request, State};
use std::path::PathBuf;

#[test]
fn pending_request_count_is_not_capped_at_sixty_four() {
    let mut state = State::default();
    for index in 0..80 {
        enqueue_request(&mut state, PathBuf::from(format!("source-{index}")));
    }
    assert_eq!(state.requests.len(), 80);
    enqueue_request(&mut state, PathBuf::from("source-0"));
    assert_eq!(state.requests.len(), 80);
    assert_eq!(state.requests[&PathBuf::from("source-0")].revision, 81);
}
