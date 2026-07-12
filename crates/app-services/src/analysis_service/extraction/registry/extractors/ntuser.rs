use super::super::appcompat::appcompat_layer_artifacts;
use super::super::context::RegistryExtractionContext;
use super::super::ntuser::{
    default_browser_artifacts, last_visited_mru_artifacts, open_save_mru_artifacts,
    run_mru_artifacts, user_assist_artifacts,
};

pub(in crate::analysis_service::extraction::registry) fn extract(
    context: &mut RegistryExtractionContext<'_>,
) {
    extract_fields(context);
    extract_appcompat_layers(context);
}

fn extract_fields(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_ntuser_fields(context.bytes, &context.candidate.path) {
        Ok(info) => {
            let outcome = user_assist_artifacts(context.candidate, &info);
            context.outcome.artifacts.extend(outcome.artifacts);
            context
                .outcome
                .timeline_events
                .extend(outcome.timeline_events);
            context
                .outcome
                .artifacts
                .extend(default_browser_artifacts(context.candidate, &info));
            context
                .outcome
                .artifacts
                .extend(open_save_mru_artifacts(context.candidate, &info));
            context
                .outcome
                .artifacts
                .extend(last_visited_mru_artifacts(context.candidate, &info));
            context
                .outcome
                .artifacts
                .extend(run_mru_artifacts(context.candidate, &info));
            context.warnings.extend(info.warnings);
        }
        Err(err) => context.outcome.warnings.push(format!(
            "{} registry parse failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_appcompat_layers(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_appcompat_layers_from_ntuser_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(appcompat_layer_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} AppCompatFlags Layers extraction failed: {}",
            context.candidate.path, err
        )),
    }
}
