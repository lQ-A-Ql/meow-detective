use super::super::amcache::{amcache_application_artifacts, amcache_application_file_artifacts};
use super::super::context::RegistryExtractionContext;

pub(in crate::analysis_service::extraction::registry) fn extract(
    context: &mut RegistryExtractionContext<'_>,
) {
    match artifacts_windows::extract_amcache_entries(context.bytes, &context.candidate.path) {
        Ok(info) => {
            context
                .outcome
                .artifacts
                .extend(amcache_application_artifacts(
                    context.candidate,
                    &info.applications,
                ));
            context
                .outcome
                .artifacts
                .extend(amcache_application_file_artifacts(
                    context.candidate,
                    &info.application_files,
                ));
            context.warnings.extend(info.warnings);
        }
        Err(err) => context.warnings.push(format!(
            "{} Amcache extraction failed: {}",
            context.candidate.path, err
        )),
    }
}
