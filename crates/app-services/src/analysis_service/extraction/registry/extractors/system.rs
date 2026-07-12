use super::super::context::RegistryExtractionContext;
use super::super::shared::registry_field_artifacts;
use super::super::system::{
    lsa_package_artifacts, mounted_device_artifacts, network_adapter_artifacts,
    shimcache_artifacts, shutdown_time_artifacts, system_service_artifacts, usb_device_artifacts,
};

pub(in crate::analysis_service::extraction::registry) fn extract(
    context: &mut RegistryExtractionContext<'_>,
) {
    extract_fields(context);
    extract_network_adapters(context);
    extract_services(context);
    extract_usb_devices(context);
    extract_mounted_devices(context);
    extract_shutdown_time(context);
    extract_shimcache(context);
    extract_lsa_packages(context);
}

fn extract_fields(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_system_hive_fields(context.bytes, &context.candidate.path) {
        Ok(info) => {
            context.outcome.artifacts.extend(registry_field_artifacts(
                context.candidate,
                vec![
                    ("computerName", info.computer_name),
                    ("timezone", info.timezone),
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

fn extract_network_adapters(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_network_adapters_from_system_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(adapters) => context
            .outcome
            .artifacts
            .extend(network_adapter_artifacts(context.candidate, &adapters)),
        Err(err) => context.warnings.push(format!(
            "{} network adapter extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_services(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_services_from_system_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(info) => {
            context
                .outcome
                .artifacts
                .extend(system_service_artifacts(context.candidate, &info.services));
            context.warnings.extend(info.warnings);
        }
        Err(err) => context.warnings.push(format!(
            "{} system service extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_usb_devices(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_usb_devices_from_system_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(usb_device_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} USB device extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_mounted_devices(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_mounted_devices_from_system_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(mounted_device_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} mounted device extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_shutdown_time(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_shutdown_time_from_system_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => {
            let outcome = shutdown_time_artifacts(context.candidate, &entries);
            context.outcome.artifacts.extend(outcome.artifacts);
            context
                .outcome
                .timeline_events
                .extend(outcome.timeline_events);
        }
        Err(err) => context.warnings.push(format!(
            "{} shutdown time extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_shimcache(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_shimcache_from_system_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(shimcache_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} ShimCache extraction failed: {}",
            context.candidate.path, err
        )),
    }
}

fn extract_lsa_packages(context: &mut RegistryExtractionContext<'_>) {
    match artifacts_windows::extract_lsa_packages_from_system_hive(
        context.bytes,
        &context.candidate.path,
    ) {
        Ok(entries) => context
            .outcome
            .artifacts
            .extend(lsa_package_artifacts(context.candidate, &entries)),
        Err(err) => context.warnings.push(format!(
            "{} LSA package extraction failed: {}",
            context.candidate.path, err
        )),
    }
}
