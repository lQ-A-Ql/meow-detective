use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use domain::Artifact;
use serde_json::Value;

pub(super) fn network_profile_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::NetworkProfileEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "profileGuid".to_string(),
                Value::String(entry.profile_guid.clone()),
            );
            attrs.insert(
                "profileName".to_string(),
                Value::String(entry.profile_name.clone()),
            );
            if let Some(description) = entry.description.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert(
                    "description".to_string(),
                    Value::String(description.clone()),
                );
            }
            if let Some(ts) = entry.date_created.as_ref() {
                attrs.insert("dateCreated".to_string(), Value::String(ts.clone()));
            }
            if let Some(ts) = entry.date_last_connected.as_ref() {
                attrs.insert("dateLastConnected".to_string(), Value::String(ts.clone()));
            }
            if let Some(name_type) = entry.name_type {
                attrs.insert("nameType".to_string(), Value::Number(name_type.into()));
            }
            attrs.insert("managed".to_string(), Value::Bool(entry.managed));
            if let Some(first_network) = entry.first_network.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert(
                    "firstNetwork".to_string(),
                    Value::String(first_network.clone()),
                );
            }
            if let Some(mac) = entry
                .default_gateway_mac_hex
                .as_ref()
                .filter(|s| !s.is_empty())
            {
                attrs.insert(
                    "defaultGatewayMacHex".to_string(),
                    Value::String(mac.clone()),
                );
            }
            if let Some(dns_suffix) = entry.dns_suffix.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("dnsSuffix".to_string(), Value::String(dns_suffix.clone()));
            }
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.software.networkprofile".to_string()),
            );
            make_artifact(
                "RegistryNetworkProfile",
                format!("Network Profile: {}", entry.profile_name),
                format!(
                    "{} ({}, managed={})",
                    entry.profile_name, entry.profile_guid, entry.managed
                ),
                candidate,
                "registry.software.networkprofile.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn installed_software_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::InstalledSoftwareInfo],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "displayName".to_string(),
                Value::String(entry.display_name.clone()),
            );
            if let Some(version) = entry.version.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("version".to_string(), Value::String(version.clone()));
            }
            if let Some(publisher) = entry.publisher.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("publisher".to_string(), Value::String(publisher.clone()));
            }
            if let Some(install_date) = entry.install_date.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert(
                    "installDate".to_string(),
                    Value::String(install_date.clone()),
                );
            }
            if let Some(size_kb) = entry.estimated_size_kb {
                attrs.insert(
                    "estimatedSize".to_string(),
                    Value::String(super::shared::format_size_kb(size_kb)),
                );
            }
            attrs.insert(
                "sourceKey".to_string(),
                Value::String(entry.source_key.clone()),
            );
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.software.uninstall".to_string()),
            );
            make_artifact(
                "RegistryInstalledSoftware",
                format!("Installed Software: {}", entry.display_name),
                format!(
                    "{} {} by {}",
                    entry.display_name,
                    entry.version.as_deref().unwrap_or(""),
                    entry.publisher.as_deref().unwrap_or("unknown")
                ),
                candidate,
                "registry.software.uninstall.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn machine_run_key_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::RegistryRunKey],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert("keyPath".to_string(), Value::String(entry.key_path.clone()));
            attrs.insert(
                "valueName".to_string(),
                Value::String(entry.value_name.clone()),
            );
            attrs.insert("command".to_string(), Value::String(entry.command.clone()));
            if let Some(ts) = entry.timestamp.as_ref() {
                attrs.insert("timestamp".to_string(), Value::String(ts.clone()));
            }
            attrs.insert("scope".to_string(), Value::String(entry.scope.clone()));
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.software.run".to_string()),
            );
            make_artifact(
                "RegistryMachineRunKey",
                format!("Machine Run Key: {}", entry.value_name),
                format!("{} = {}", entry.value_name, entry.command),
                candidate,
                "registry.software.run.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn winlogon_config_artifacts(
    candidate: &EvidenceCandidate,
    config: &artifacts_windows::WinlogonConfig,
) -> Vec<Artifact> {
    let has_any = config.shell.is_some()
        || config.userinit.is_some()
        || config.notify.is_some()
        || config.auto_admin_logon.is_some()
        || config.default_domain_name.is_some()
        || config.default_user_name.is_some();
    if !has_any {
        return Vec::new();
    }

    let mut attrs = base_attrs(candidate);
    if let Some(v) = config.shell.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert("shell".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = config.userinit.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert("userinit".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = config.notify.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert("notify".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = config.auto_admin_logon.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert("autoAdminLogon".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = config
        .default_domain_name
        .as_ref()
        .filter(|s| !s.is_empty())
    {
        attrs.insert("defaultDomainName".to_string(), Value::String(v.clone()));
    }
    if let Some(v) = config.default_user_name.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert("defaultUserName".to_string(), Value::String(v.clone()));
    }
    attrs.insert(
        "keyPath".to_string(),
        Value::String(config.key_path.clone()),
    );
    attrs.insert(
        "parser".to_string(),
        Value::String("registry.software.winlogon".to_string()),
    );

    vec![make_artifact(
        "RegistryWinlogonConfig",
        "Winlogon Configuration".to_string(),
        format!(
            "Shell: {}, Userinit: {}",
            config.shell.as_deref().unwrap_or("-"),
            config.userinit.as_deref().unwrap_or("-")
        ),
        candidate,
        "registry.software.winlogon.v1",
        attrs,
    )]
}
