use super::super::context::RegistryExtractionContext;
use super::super::sam::sam_user_artifacts;

pub(in crate::analysis_service::extraction::registry) fn extract(
    context: &mut RegistryExtractionContext<'_>,
) {
    let result = if context.txlog1.is_some() || context.txlog2.is_some() {
        artifacts_windows::extract_sam_fields_with_txlog(
            context.bytes,
            &context.candidate.path,
            context.boot_key,
            context.txlog1,
            context.txlog2,
        )
    } else {
        artifacts_windows::extract_sam_fields(
            context.bytes,
            &context.candidate.path,
            context.boot_key,
        )
    };
    match result {
        Ok(info) => {
            let outcome = sam_user_artifacts(context.candidate, &info);
            context.outcome.artifacts.extend(outcome.artifacts);
            context
                .outcome
                .timeline_events
                .extend(outcome.timeline_events);
            context.warnings.extend(info.warnings);
        }
        Err(err) => context.outcome.warnings.push(format!(
            "{} registry parse failed: {}",
            context.candidate.path, err
        )),
    }
}
