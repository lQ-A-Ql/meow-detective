use crate::timeline_service::fjall::validate_resume_position;

#[test]
fn resumed_source_requires_a_position_until_it_is_exhausted() {
    assert!(validate_resume_position(5, 10, true).is_ok());
    assert!(validate_resume_position(5, 10, false).is_err());
    assert!(validate_resume_position(0, 10, false).is_ok());
    assert!(validate_resume_position(10, 10, false).is_ok());
    assert!(validate_resume_position(11, 10, true).is_err());
}
