use transport::dto::{
    EmulationControlModeDto, EmulationGuestPhaseDto, EmulationSessionStatusDto, EmulationStateDto,
};

use crate::emulation_registry::{EmulationGuestPhase, EmulationSessionStatus, EmulationState};

pub(super) fn to_dto(status: EmulationSessionStatus) -> EmulationSessionStatusDto {
    EmulationSessionStatusDto {
        session_id: status.session_id,
        data_source_id: status.data_source_id,
        state: state_to_dto(status.state),
        logical_length: status.logical_length,
        control_mode: EmulationControlModeDto::InteractiveOnly,
        guest_phase: guest_phase_to_dto(status.guest_phase),
        maintenance_media: status.maintenance_media,
        error: status.error,
    }
}

fn guest_phase_to_dto(phase: EmulationGuestPhase) -> EmulationGuestPhaseDto {
    match phase {
        EmulationGuestPhase::Unknown => EmulationGuestPhaseDto::Unknown,
        EmulationGuestPhase::Booting => EmulationGuestPhaseDto::Booting,
        EmulationGuestPhase::FilesystemMounted => EmulationGuestPhaseDto::FilesystemMounted,
    }
}

pub(super) fn state_to_dto(state: EmulationState) -> EmulationStateDto {
    match state {
        EmulationState::DescriptorReady => EmulationStateDto::DescriptorReady,
        EmulationState::Running => EmulationStateDto::Running,
        EmulationState::Quiescing => EmulationStateDto::Quiescing,
        EmulationState::Released => EmulationStateDto::Released,
        EmulationState::FailedCleanupPending => EmulationStateDto::FailedCleanupPending,
    }
}
