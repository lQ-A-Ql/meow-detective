use super::{state_to_dto, validate_session_id};
use crate::emulation_registry::EmulationState;

#[test]
fn session_id_validation_rejects_blank_values() {
    assert!(validate_session_id("   ").is_err());
    assert!(validate_session_id("emulation-1").is_ok());
}

#[test]
fn emulation_state_mapping_preserves_terminal_states() {
    assert_eq!(
        state_to_dto(EmulationState::Released),
        transport::dto::EmulationStateDto::Released
    );
    assert_eq!(
        state_to_dto(EmulationState::FailedCleanupPending),
        transport::dto::EmulationStateDto::FailedCleanupPending
    );
}
