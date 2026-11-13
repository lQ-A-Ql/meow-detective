use super::{source_preparation, CandidateProcessingContext, CandidateWorkItem};

pub(super) fn estimated_input_weight(
    context: &CandidateProcessingContext<'_>,
    item: &CandidateWorkItem,
) -> usize {
    if context
        .checkpoints
        .has_candidate(&item.candidate, item.capability)
        || context
            .preloads
            .registry
            .dependency_read_failure(&item.candidate)
            .is_some()
        || context
            .preloads
            .browser
            .dependency_read_failure(&item.candidate)
            .is_some()
    {
        0
    } else {
        source_preparation::estimated_input_weight(item)
    }
}
