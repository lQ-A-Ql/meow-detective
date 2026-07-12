use transport::dto::CancellationStateDto;

use super::super::cancellation::job_cancellation_dto;

#[test]
fn job_cancellation_dto_maps_requested_and_draining_states() {
    let requested = job_cancellation_dto(
        "job-cancel-1",
        CancellationStateDto::Requested,
        false,
        "Cancel requested by user",
    );
    assert_eq!(requested.job_id, "job-cancel-1");
    assert_eq!(requested.state, CancellationStateDto::Requested);
    assert!(!requested.safe_to_close);
    assert!(requested.requested_at.is_some());
    assert!(requested.acknowledged_at.is_none());

    let draining = job_cancellation_dto(
        "job-cancel-1",
        CancellationStateDto::Draining,
        false,
        "Cancellation acknowledged; draining workers",
    );
    assert_eq!(draining.state, CancellationStateDto::Draining);
    assert!(!draining.safe_to_close);
    assert!(draining.requested_at.is_some());
    assert!(draining.acknowledged_at.is_some());
}
