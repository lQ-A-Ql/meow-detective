use super::{
    status_dto::{state_to_dto, to_dto},
    validate_session_id,
};
use crate::emulation_registry::{EmulationGuestPhase, EmulationSessionStatus, EmulationState};

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

#[test]
fn session_status_mapping_preserves_observed_guest_phase() {
    let status = to_dto(EmulationSessionStatus {
        session_id: "session-1".to_string(),
        data_source_id: "source-1".to_string(),
        state: EmulationState::Running,
        guest_phase: EmulationGuestPhase::Booting,
        logical_length: 1024,
        maintenance_media: false,
        error: None,
    });

    assert_eq!(
        status.guest_phase,
        transport::dto::EmulationGuestPhaseDto::Booting
    );
}
