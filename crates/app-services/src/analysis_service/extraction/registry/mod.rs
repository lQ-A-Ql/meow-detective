use super::ExtractionOutcome;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use std::collections::BTreeSet;

mod amcache;
mod appcompat;
mod ntuser;
mod sam;
mod security;
mod shared;
mod software;
mod system;
mod usrclass;

use amcache::{amcache_application_artifacts, amcache_application_file_artifacts};
use appcompat::appcompat_layer_artifacts;
use ntuser::{
    default_browser_artifacts, last_visited_mru_artifacts, open_save_mru_artifacts,
    run_mru_artifacts, user_assist_artifacts,
};
use sam::sam_user_artifacts;
use security::{cached_credential_artifacts, lsa_secret_artifacts, security_policy_artifacts};
use shared::{hive_meta_artifact, registry_field_artifacts};
use software::{
    installed_software_artifacts, machine_run_key_artifacts, network_profile_artifacts,
    winlogon_config_artifacts,
};
use system::{
    lsa_package_artifacts, mounted_device_artifacts, network_adapter_artifacts,
    shimcache_artifacts, shutdown_time_artifacts, system_service_artifacts, usb_device_artifacts,
};
use usrclass::{muicache_artifacts, shellbag_artifacts};

pub fn extract_registry_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    boot_key: Option<[u8; 16]>,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let mut raw_warnings = Vec::new();
    if !bytes.starts_with(b"regf") {
        raw_warnings.push(format!("{} is not a regf registry hive", candidate.path));
        outcome.warnings = govern_registry_warnings(&candidate.path, raw_warnings);
        return outcome;
    }

    let normalized = normalize_evidence_path(&candidate.path);

    let txlog_merged = txlog_merge_status(&candidate.path, txlog1, txlog2, &mut raw_warnings);
    let deleted_keys_found =
        count_deleted_registry_cells(&candidate.path, bytes, &mut raw_warnings);

    if normalized.ends_with("/windows/system32/config/system") {
        match artifacts_windows::extract_system_hive_fields(bytes, &candidate.path) {
            Ok(info) => {
                outcome.artifacts.extend(registry_field_artifacts(
                    candidate,
                    vec![
                        ("computerName", info.computer_name),
                        ("timezone", info.timezone),
                    ],
                ));
                raw_warnings.extend(info.warnings);
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} registry parse failed: {}", candidate.path, err)),
        }

        match artifacts_windows::extract_network_adapters_from_system_hive(bytes, &candidate.path) {
            Ok(adapters) => {
                outcome
                    .artifacts
                    .extend(network_adapter_artifacts(candidate, &adapters));
            }
            Err(err) => raw_warnings.push(format!(
                "{} network adapter extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_services_from_system_hive(bytes, &candidate.path) {
            Ok(info) => {
                outcome
                    .artifacts
                    .extend(system_service_artifacts(candidate, &info.services));
                raw_warnings.extend(info.warnings);
            }
            Err(err) => raw_warnings.push(format!(
                "{} system service extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_usb_devices_from_system_hive(bytes, &candidate.path) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(usb_device_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} USB device extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_mounted_devices_from_system_hive(bytes, &candidate.path) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(mounted_device_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} mounted device extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_shutdown_time_from_system_hive(bytes, &candidate.path) {
            Ok(entries) => {
                let shutdown_outcome = shutdown_time_artifacts(candidate, &entries);
                outcome.artifacts.extend(shutdown_outcome.artifacts);
                outcome
                    .timeline_events
                    .extend(shutdown_outcome.timeline_events);
            }
            Err(err) => raw_warnings.push(format!(
                "{} shutdown time extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_shimcache_from_system_hive(bytes, &candidate.path) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(shimcache_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} ShimCache extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_lsa_packages_from_system_hive(bytes, &candidate.path) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(lsa_package_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} LSA package extraction failed: {}",
                candidate.path, err
            )),
        }
    } else if normalized.ends_with("/windows/system32/config/software") {
        match artifacts_windows::extract_software_hive_fields(bytes, &candidate.path) {
            Ok(info) => {
                outcome.artifacts.extend(registry_field_artifacts(
                    candidate,
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
                raw_warnings.extend(info.warnings);
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} registry parse failed: {}", candidate.path, err)),
        }

        match artifacts_windows::extract_installed_software(bytes, &candidate.path) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(installed_software_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} installed software extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_machine_run_keys_from_software_hive(bytes, &candidate.path)
        {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(machine_run_key_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} machine Run key extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_winlogon_fields_from_software_hive(bytes, &candidate.path)
        {
            Ok(config) => {
                outcome
                    .artifacts
                    .extend(winlogon_config_artifacts(candidate, &config));
            }
            Err(err) => raw_warnings.push(format!(
                "{} Winlogon extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_network_profiles_from_software_hive(bytes, &candidate.path)
        {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(network_profile_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} NetworkList profile extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_appcompat_layers_from_software_hive(bytes, &candidate.path)
        {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(appcompat_layer_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} AppCompatFlags Layers extraction failed: {}",
                candidate.path, err
            )),
        }
    } else if normalized.ends_with("/windows/system32/config/sam") {
        let sam_result = if txlog1.is_some() || txlog2.is_some() {
            artifacts_windows::extract_sam_fields_with_txlog(
                bytes,
                &candidate.path,
                boot_key,
                txlog1,
                txlog2,
            )
        } else {
            artifacts_windows::extract_sam_fields(bytes, &candidate.path, boot_key)
        };
        match sam_result {
            Ok(info) => {
                let sam_outcome = sam_user_artifacts(candidate, &info);
                outcome.artifacts.extend(sam_outcome.artifacts);
                outcome.timeline_events.extend(sam_outcome.timeline_events);
                raw_warnings.extend(info.warnings);
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} registry parse failed: {}", candidate.path, err)),
        }
    } else if normalized.ends_with("/ntuser.dat") {
        match artifacts_windows::extract_ntuser_fields(bytes, &candidate.path) {
            Ok(info) => {
                let ua_outcome = user_assist_artifacts(candidate, &info);
                outcome.artifacts.extend(ua_outcome.artifacts);
                outcome.timeline_events.extend(ua_outcome.timeline_events);
                outcome
                    .artifacts
                    .extend(default_browser_artifacts(candidate, &info));
                outcome
                    .artifacts
                    .extend(open_save_mru_artifacts(candidate, &info));
                outcome
                    .artifacts
                    .extend(last_visited_mru_artifacts(candidate, &info));
                outcome
                    .artifacts
                    .extend(run_mru_artifacts(candidate, &info));
                raw_warnings.extend(info.warnings);
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} registry parse failed: {}", candidate.path, err)),
        }

        match artifacts_windows::extract_appcompat_layers_from_ntuser_hive(bytes, &candidate.path) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(appcompat_layer_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} AppCompatFlags Layers extraction failed: {}",
                candidate.path, err
            )),
        }
    } else if normalized.ends_with("/usrclass.dat") {
        match artifacts_windows::extract_shellbags_from_usrclass_hive(bytes, &candidate.path) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(shellbag_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} Shellbag extraction failed: {}",
                candidate.path, err
            )),
        }
        match artifacts_windows::extract_muicache_from_usrclass_hive(bytes, &candidate.path) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(muicache_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} MuiCache extraction failed: {}",
                candidate.path, err
            )),
        }
    } else if normalized.ends_with("/amcache.hve") {
        match artifacts_windows::extract_amcache_entries(bytes, &candidate.path) {
            Ok(info) => {
                outcome
                    .artifacts
                    .extend(amcache_application_artifacts(candidate, &info.applications));
                outcome.artifacts.extend(amcache_application_file_artifacts(
                    candidate,
                    &info.application_files,
                ));
                raw_warnings.extend(info.warnings);
            }
            Err(err) => raw_warnings.push(format!(
                "{} Amcache extraction failed: {}",
                candidate.path, err
            )),
        }
    } else if normalized.ends_with("/security") {
        let policy_result = if txlog1.is_some() || txlog2.is_some() {
            artifacts_windows::extract_security_policy_from_security_hive_with_txlog(
                bytes,
                &candidate.path,
                boot_key,
                txlog1,
                txlog2,
            )
        } else {
            artifacts_windows::extract_security_policy_from_security_hive(
                bytes,
                &candidate.path,
                boot_key,
            )
        };
        match policy_result {
            Ok(entry) => {
                outcome
                    .artifacts
                    .extend(security_policy_artifacts(candidate, &entry));
            }
            Err(err) => raw_warnings.push(format!(
                "{} SECURITY policy extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_lsa_secrets_from_security_hive(
            bytes,
            &candidate.path,
            boot_key,
        ) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(lsa_secret_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} LSA secret extraction failed: {}",
                candidate.path, err
            )),
        }

        match artifacts_windows::extract_cached_credentials_from_security_hive(
            bytes,
            &candidate.path,
            boot_key,
        ) {
            Ok(entries) => {
                outcome
                    .artifacts
                    .extend(cached_credential_artifacts(candidate, &entries));
            }
            Err(err) => raw_warnings.push(format!(
                "{} cached credential extraction failed: {}",
                candidate.path, err
            )),
        }
    }

    // Every registry candidate (parsed or just recognized) gets a lightweight
    // hive meta artifact carrying real txlog/deleted-cell statistics.
    outcome.artifacts.push(hive_meta_artifact(
        candidate,
        txlog_merged,
        deleted_keys_found,
    ));
    outcome.warnings = govern_registry_warnings(&candidate.path, raw_warnings);
    outcome
}

/// Determine whether any supplied transaction log parses successfully.
fn txlog_merge_status(
    path: &str,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
    warnings: &mut Vec<String>,
) -> bool {
    let mut merged = false;
    for (label, data) in [("LOG1", txlog1), ("LOG2", txlog2)] {
        if let Some(d) = data {
            match artifacts_windows::parse_transaction_log(d) {
                Ok(_) => merged = true,
                Err(err) => {
                    warnings.push(format!("{} {} txlog parse failed: {}", path, label, err))
                }
            }
        }
    }
    merged
}

/// Count deleted registry keys/values recovered from free cells.
fn count_deleted_registry_cells(path: &str, bytes: &[u8], warnings: &mut Vec<String>) -> u32 {
    match artifacts_windows::scan_deleted_registry_cells(bytes, path) {
        Ok(result) => (result.recovered_keys.len() + result.recovered_values.len()) as u32,
        Err(err) => {
            warnings.push(format!("{} deleted cell scan failed: {}", path, err));
            0
        }
    }
}

const MAX_REGISTRY_WARNINGS: usize = 64;

/// Apply governance to raw registry warnings: prefix warning codes, deduplicate,
/// cap the total number, and redact absolute filesystem paths.
fn govern_registry_warnings(path: &str, raw: Vec<String>) -> Vec<String> {
    let sanitized = sanitize_registry_path(path);
    let mut seen = BTreeSet::new();
    let mut governed = Vec::with_capacity(raw.len().min(MAX_REGISTRY_WARNINGS));
    for message in raw {
        let code = warning_code_for(&message);
        let entry = format!("[{}] {}: {}", code, sanitized, message);
        if !seen.insert(entry.clone()) {
            continue;
        }
        if governed.len() >= MAX_REGISTRY_WARNINGS {
            if !governed.iter().any(|w: &String| w.starts_with("[REG-CAP]")) {
                governed.push(format!(
                    "[REG-CAP] {}: additional registry warnings suppressed",
                    sanitized
                ));
            }
            break;
        }
        governed.push(entry);
    }
    governed
}

fn sanitize_registry_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.contains(":/") || normalized.starts_with('/') {
        normalized
            .rsplit('/')
            .next()
            .unwrap_or(&normalized)
            .to_string()
    } else {
        normalized
    }
}

fn warning_code_for(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("txlog") || lower.contains("log1") || lower.contains("log2") {
        "REG-TXLOG"
    } else if lower.contains("deleted") || lower.contains("recovery") || lower.contains("free cell")
    {
        "REG-RECOVERY"
    } else if lower.contains("security") || lower.contains("lsa") || lower.contains("cached") {
        "REG-SEC"
    } else if lower.contains("sam") {
        "REG-SAM"
    } else if lower.contains("ntuser") || lower.contains("userassist") || lower.contains("run mru")
    {
        "REG-NTUSER"
    } else if lower.contains("usrclass") || lower.contains("shellbag") || lower.contains("muicache")
    {
        "REG-USRCLASS"
    } else if lower.contains("amcache") {
        "REG-AMCACHE"
    } else if lower.contains("software") {
        "REG-SOFTWARE"
    } else if lower.contains("system") {
        "REG-SYSTEM"
    } else {
        "REG-WARN"
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/registry/mod.rs"]
mod tests;
