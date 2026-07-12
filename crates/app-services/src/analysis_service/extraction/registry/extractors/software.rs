use super::super::appcompat::appcompat_layer_artifacts;
use super::super::context::RegistryExtractionContext;
use super::super::shared::registry_field_artifacts;
use super::super::software::{
    installed_software_artifacts, machine_run_key_artifacts, network_profile_artifacts,
    winlogon_config_artifacts,
};

pub(in crate::analysis_service::extraction::registry) fn extract(
    context: &mut RegistryExtractionContext<'_>,
) {
    extract_fields(context);
    extract_installed_software(context);
    extract_machine_run_keys(context);
    extract_winlogon(context);
    extract_network_profiles(context);
    extract_appcompat_layers(context);
}

fn extract_fields(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_software_hive_fields(context.bytes, &context.candidate.path) {
        Ok(info) => {
            context.outcome.artifacts.extend(registry_field_artifacts(
                context.candidate,
                vec![
                    ("productName", info.product_name),
                    ("currentBuild", info.current_build),
                    ("currentVersion", info.current_version),
                    ("displayVersion", info.display_version),
                    ("installDate", info.install_date),
                    ("registeredOwner", info.registered_owner),
                    ("registeredOrganization", info.registered_organization),
                    ("productId", info.product_id),
                ],
            ));
            context.warnings.extend(info.warnings);
        }
        Err(err) => context.outcome.warnings.push(format!(
            "{} registry parse failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_installed_software(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_installed_software(context.bytes, &context.candidate.path) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(installed_software_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} installed software extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_machine_run_keys(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_machine_run_keys_from_software_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(machine_run_key_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} machine Run key extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_winlogon(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_winlogon_fields_from_software_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(config) => context
            .outcome
            .artifacts
            .extend(winlogon_config_artifacts(context.candidate, &config)),
        Err(err) => context.warnings.push(format!(
            "{} Winlogon extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_network_profiles(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_network_profiles_from_software_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(network_profile_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} NetworkList profile extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_appcompat_layers(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_appcompat_layers_from_software_hive(
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
