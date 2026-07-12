use super::super::context::RegistryExtractionContext;
use super::super::security::{
    cached_credential_artifacts, lsa_secret_artifacts, security_policy_artifacts,
};

pub(in crate::analysis_service::extraction::registry) fn extract(
    context: &mut RegistryExtractionContext<'_>,
) {
    extract_policy(context);
    extract_lsa_secrets(context);
    extract_cached_credentials(context);
}

fn extract_policy(context: &mut RegistryExtractionContext<'_>) {
    let result = if context.txlog1.is_some() || context.txlog2.is_some() {
        artifacts_windows::extract_security_policy_from_security_hive_with_txlog(
            context.bytes,
            &context.candidate.path,
            context.boot_key,
            context.txlog1,
            context.txlog2,
        )
    } else {
        artifacts_windows::extract_security_policy_from_security_hive(
            context.bytes,
            &context.candidate.path,
            context.boot_key,
        )
    };
    match result {
        Ok(entry) => context
            .outcome
            .artifacts
            .extend(security_policy_artifacts(context.candidate, &entry)),
        Err(err) => context.warnings.push(format!(
            "{} SECURITY policy extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_lsa_secrets(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_lsa_secrets_from_security_hive(
        context.bytes,
        &context.candidate.path,
        context.boot_key,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(lsa_secret_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} LSA secret extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_cached_credentials(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_cached_credentials_from_security_hive(
        context.bytes,
        &context.candidate.path,
        context.boot_key,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(cached_credential_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} cached credential extraction failed: {}",
            context.candidate.path, err
        )),
    }
}
