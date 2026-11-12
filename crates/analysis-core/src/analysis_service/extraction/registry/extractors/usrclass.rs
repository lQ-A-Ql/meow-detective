use super::super::context::RegistryExtractionContext;
use super::super::usrclass::{muicache_artifacts, shellbag_artifacts};

pub(in crate::analysis_service::extraction::registry) fn extract(
    context: &mut RegistryExtractionContext<'_>,
) {
    extract_shellbags(context);
    extract_muicache(context);
}

fn extract_shellbags(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_shellbags_from_usrclass_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(shellbag_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} Shellbag extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_muicache(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_muicache_from_usrclass_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(muicache_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} MuiCache extraction failed: {}",
            context.candidate.path, err
        )),
    }
}
