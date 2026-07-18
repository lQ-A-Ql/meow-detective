use persistence_sqlite::repositories::processing_phase_repo::{
    ProcessingPhase, ProcessingPhaseState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedFinalizationPhaseOutcome {
    pub phase: ProcessingPhase,
    pub state: ProcessingPhaseState,
    pub stats_json: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DerivedFinalizationReport {
    pub phases: Vec<DerivedFinalizationPhaseOutcome>,
}

impl DerivedFinalizationReport {
    pub fn push(&mut self, outcome: DerivedFinalizationPhaseOutcome) {
        self.phases.push(outcome);
    }

    pub fn failed_count(&self) -> usize {
        self.phases
            .iter()
            .filter(|outcome| outcome.state == ProcessingPhaseState::Failed)
            .count()
    }

    pub fn deferred_count(&self) -> usize {
        self.phases
            .iter()
            .filter(|outcome| outcome.state == ProcessingPhaseState::Deferred)
            .count()
    }

    pub fn unfinished_count(&self) -> usize {
        let observed = self
            .phases
            .iter()
            .filter(|outcome| {
                !matches!(
                    outcome.state,
                    ProcessingPhaseState::Ready
                        | ProcessingPhaseState::Failed
                        | ProcessingPhaseState::Deferred
                )
            })
            .count();
        observed + (ProcessingPhase::ALL.len() - 1).saturating_sub(self.phases.len())
    }

    pub fn all_ready(&self) -> bool {
        self.phases.len() == ProcessingPhase::ALL.len() - 1
            && self
                .phases
                .iter()
                .all(|outcome| outcome.state == ProcessingPhaseState::Ready)
    }
}
