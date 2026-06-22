use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::{DateTime, Utc};
use domain::Artifact;
use serde_json::Value;

pub(super) fn network_adapter_artifacts(
    candidate: &EvidenceCandidate,
    adapters: &[artifacts_windows::NetworkAdapterInfo],
) -> Vec<Artifact> {
    adapters
        .iter()
        .map(|adapter| {
            let mut attrs = base_attrs(candidate);
            attrs.insert("guid".to_string(), Value::String(adapter.guid.clone()));
            attrs.insert(
                "name".to_string(),
                Value::String(adapter.name.clone().unwrap_or_else(|| adapter.guid.clone())),
            );
            if let Some(mac) = &adapter.mac_address {
                attrs.insert("macAddress".to_string(), Value::String(mac.clone()));
            }
            if let Some(ip) = &adapter.ip_address {
                attrs.insert("ipAddress".to_string(), Value::String(ip.clone()));
            }
            if let Some(gateway) = &adapter.gateway {
                attrs.insert("gateway".to_string(), Value::String(gateway.clone()));
            }
            if let Some(dhcp_server) = &adapter.dhcp_server {
                attrs.insert("dhcpServer".to_string(), Value::String(dhcp_server.clone()));
            }
            if let Some(enabled) = adapter.dhcp_enabled {
                attrs.insert("dhcpEnabled".to_string(), Value::Bool(enabled));
            }
            attrs.insert(
                "dnsServers".to_string(),
                Value::Array(
                    adapter
                        .dns_servers
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            );
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.system.network".to_string()),
            );
            make_artifact(
                "RegistryNetworkAdapter",
                format!(
                    "Network Adapter: {}",
                    adapter.name.as_deref().unwrap_or(&adapter.guid)
                ),
                format!(
                    "Adapter {} (IP: {}, MAC: {})",
                    adapter.name.as_deref().unwrap_or(&adapter.guid),
                    adapter.ip_address.as_deref().unwrap_or("-"),
                    adapter.mac_address.as_deref().unwrap_or("-")
                ),
                candidate,
                "registry.system.network.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn system_service_artifacts(
    candidate: &EvidenceCandidate,
    services: &[artifacts_windows::SystemServiceEntry],
) -> Vec<Artifact> {
    services
        .iter()
        .map(|svc| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "serviceName".to_string(),
                Value::String(svc.service_name.clone()),
            );
            if let Some(name) = svc.display_name.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("displayName".to_string(), Value::String(name.clone()));
            }
            if let Some(path) = svc.image_path.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("imagePath".to_string(), Value::String(path.clone()));
            }
            if let Some(dll) = svc.service_dll.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("serviceDll".to_string(), Value::String(dll.clone()));
            }
            attrs.insert(
                "serviceType".to_string(),
                Value::String(svc.service_type.as_str().to_string()),
            );
            attrs.insert(
                "startType".to_string(),
                Value::String(svc.start_type.as_str().to_string()),
            );
            attrs.insert(
                "delayedAutoStart".to_string(),
                Value::Bool(svc.delayed_auto_start),
            );
            if let Some(ec) = svc.error_control {
                attrs.insert("errorControl".to_string(), Value::Number(ec.into()));
            }
            if let Some(group) = svc.group.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("group".to_string(), Value::String(group.clone()));
            }
            if let Some(obj) = svc.object_name.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("objectName".to_string(), Value::String(obj.clone()));
            }
            if !svc.depend_on_service.is_empty() {
                attrs.insert(
                    "dependOnService".to_string(),
                    string_array_value(&svc.depend_on_service),
                );
            }
            if !svc.depend_on_group.is_empty() {
                attrs.insert(
                    "dependOnGroup".to_string(),
                    string_array_value(&svc.depend_on_group),
                );
            }
            if let Some(cmd) = svc.failure_command.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("failureCommand".to_string(), Value::String(cmd.clone()));
            }
            if !svc.required_privileges.is_empty() {
                attrs.insert(
                    "requiredPrivileges".to_string(),
                    string_array_value(&svc.required_privileges),
                );
            }
            if let Some(ts) = svc.key_last_write.as_ref() {
                attrs.insert("keyLastWrite".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.system.service".to_string()),
            );
            let display_name = svc
                .display_name
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&svc.service_name);
            make_artifact(
                "RegistrySystemService",
                format!("Service: {}", display_name),
                format!(
                    "{} ({}, {})",
                    svc.service_name,
                    svc.service_type.as_str(),
                    svc.start_type.as_str()
                ),
                candidate,
                "registry.system.service.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn usb_device_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::UsbDeviceHistoryEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "deviceName".to_string(),
                Value::String(entry.device_name.clone()),
            );
            attrs.insert(
                "serialNumber".to_string(),
                Value::String(entry.serial_number.clone()),
            );
            attrs.insert(
                "rawSerialNumber".to_string(),
                Value::String(entry.raw_serial_number.clone()),
            );
            if let Some(vendor) = entry.vendor.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("vendor".to_string(), Value::String(vendor.clone()));
            }
            if let Some(product) = entry.product.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("product".to_string(), Value::String(product.clone()));
            }
            if let Some(revision) = entry.revision.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("revision".to_string(), Value::String(revision.clone()));
            }
            if let Some(ts) = entry.first_connect.as_ref() {
                attrs.insert("firstConnect".to_string(), Value::String(ts.clone()));
            }
            if let Some(ts) = entry.last_connect.as_ref() {
                attrs.insert("lastConnect".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.system.usb".to_string()),
            );
            make_artifact(
                "RegistryUsbDevice",
                format!("USB Device: {}", entry.device_name),
                format!("{} (serial {})", entry.device_name, entry.serial_number),
                candidate,
                "registry.system.usb.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn mounted_device_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::MountedDeviceEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "deviceName".to_string(),
                Value::String(entry.device_name.clone()),
            );
            if let Some(letter) = entry.drive_letter.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("driveLetter".to_string(), Value::String(letter.clone()));
            }
            if let Some(guid) = entry.volume_guid.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("volumeGuid".to_string(), Value::String(guid.clone()));
            }
            if let Some(sig) = entry.disk_signature_hex.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("diskSignatureHex".to_string(), Value::String(sig.clone()));
            }
            if let Some(target) = entry.target_name.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("targetName".to_string(), Value::String(target.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.system.mounted".to_string()),
            );
            make_artifact(
                "RegistryMountedDevice",
                format!("Mounted Device: {}", entry.device_name),
                format!(
                    "{} (drive {}, volume {})",
                    entry.device_name,
                    entry.drive_letter.as_deref().unwrap_or("-"),
                    entry.volume_guid.as_deref().unwrap_or("-")
                ),
                candidate,
                "registry.system.mounted.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn shutdown_time_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::ShutdownTimeEntry],
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    for entry in entries {
        let mut attrs = base_attrs(candidate);
        attrs.insert("keyPath".to_string(), Value::String(entry.key_path.clone()));
        attrs.insert(
            "shutdownTime".to_string(),
            Value::String(entry.shutdown_time.clone()),
        );
        attrs.insert(
            "parser".to_string(),
            Value::String("registry.system.shutdown".to_string()),
        );
        outcome.artifacts.push(make_artifact(
            "RegistryShutdownTime",
            format!("Shutdown Time: {}", entry.shutdown_time),
            format!(
                "System shutdown at {} ({})",
                entry.shutdown_time, entry.key_path
            ),
            candidate,
            "registry.system.shutdown.v1",
            attrs.clone(),
        ));
        if let Ok(ts) = DateTime::parse_from_rfc3339(&entry.shutdown_time) {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "REGISTRY_SHUTDOWN",
                ts.with_timezone(&Utc),
                format!("System shutdown: {}", entry.shutdown_time),
                format!(
                    "System shutdown recorded at {} ({})",
                    entry.shutdown_time, entry.key_path
                ),
                attrs,
                "registry.system.shutdown.v1",
            ));
        }
    }
    outcome
}

pub(super) fn shimcache_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::ShimCacheEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert("path".to_string(), Value::String(entry.path.clone()));
            if let Some(ts) = entry.last_modified.as_ref() {
                attrs.insert("lastModified".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.system.shimcache".to_string()),
            );
            make_artifact(
                "RegistryShimCache",
                format!("ShimCache: {}", entry.path),
                format!(
                    "AppCompatCache entry {} (modified {})",
                    entry.path,
                    entry.last_modified.as_deref().unwrap_or("unknown")
                ),
                candidate,
                "registry.system.shimcache.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn lsa_package_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::LsaPackages],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "controlSet".to_string(),
                Value::String(entry.control_set.clone()),
            );
            if !entry.authentication_packages.is_empty() {
                attrs.insert(
                    "authenticationPackages".to_string(),
                    string_array_value(&entry.authentication_packages),
                );
            }
            if !entry.notification_packages.is_empty() {
                attrs.insert(
                    "notificationPackages".to_string(),
                    string_array_value(&entry.notification_packages),
                );
            }
            if !entry.security_packages.is_empty() {
                attrs.insert(
                    "securityPackages".to_string(),
                    string_array_value(&entry.security_packages),
                );
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.system.lsa".to_string()),
            );
            make_artifact(
                "RegistryLsaPackage",
                format!("LSA Packages: {}", entry.control_set),
                format!(
                    "auth={}, notify={}, sec={}",
                    entry.authentication_packages.len(),
                    entry.notification_packages.len(),
                    entry.security_packages.len()
                ),
                candidate,
                "registry.system.lsa.v1",
                attrs,
            )
        })
        .collect()
}
