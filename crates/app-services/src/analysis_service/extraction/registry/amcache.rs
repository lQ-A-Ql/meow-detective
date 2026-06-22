use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use domain::Artifact;
use serde_json::Value;

pub(super) fn amcache_application_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::AmcacheApplicationEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            if let Some(program_id) = entry.program_id.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("programId".to_string(), Value::String(program_id.clone()));
            }
            if let Some(name) = entry.name.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("name".to_string(), Value::String(name.clone()));
            }
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
            if let Some(source) = entry.source.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("source".to_string(), Value::String(source.clone()));
            }
            if let Some(os_version) = entry
                .os_version_at_install_time
                .as_ref()
                .filter(|s| !s.is_empty())
            {
                attrs.insert(
                    "osVersionAtInstallTime".to_string(),
                    Value::String(os_version.clone()),
                );
            }
            attrs.insert(
                "registryKeyPath".to_string(),
                Value::String(entry.registry_key_path.clone()),
            );
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.amcache.application".to_string()),
            );
            let title = entry
                .name
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown");
            make_artifact(
                "RegistryAmcacheApplication",
                format!("Amcache Application: {title}"),
                format!(
                    "{} {} by {} (source: {})",
                    title,
                    entry.version.as_deref().unwrap_or(""),
                    entry.publisher.as_deref().unwrap_or("unknown"),
                    entry.source.as_deref().unwrap_or("unknown")
                ),
                candidate,
                "registry.amcache.application.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn amcache_application_file_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::AmcacheApplicationFileEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            if let Some(program_id) = entry.program_id.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("programId".to_string(), Value::String(program_id.clone()));
            }
            if let Some(path) = entry
                .lower_case_long_path
                .as_ref()
                .filter(|s| !s.is_empty())
            {
                attrs.insert("lowerCaseLongPath".to_string(), Value::String(path.clone()));
            }
            if let Some(hash) = entry.long_path_hash.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("longPathHash".to_string(), Value::String(hash.clone()));
            }
            if let Some(size) = entry.file_size {
                attrs.insert("fileSize".to_string(), Value::Number(size.into()));
            }
            if let Some(product_name) = entry.product_name.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert(
                    "productName".to_string(),
                    Value::String(product_name.clone()),
                );
            }
            if let Some(company_name) = entry.company_name.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert(
                    "companyName".to_string(),
                    Value::String(company_name.clone()),
                );
            }
            if let Some(file_version) = entry.file_version.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert(
                    "fileVersion".to_string(),
                    Value::String(file_version.clone()),
                );
            }
            if let Some(is_pe) = entry.is_pe_file {
                attrs.insert("isPeFile".to_string(), Value::Bool(is_pe));
            }
            if let Some(link_date) = entry.link_date.as_ref().filter(|s| !s.is_empty()) {
                attrs.insert("linkDate".to_string(), Value::String(link_date.clone()));
            }
            attrs.insert(
                "registryKeyPath".to_string(),
                Value::String(entry.registry_key_path.clone()),
            );
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.amcache.applicationfile".to_string()),
            );
            let title = entry
                .lower_case_long_path
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown");
            make_artifact(
                "RegistryAmcacheApplicationFile",
                format!("Amcache Application File: {title}"),
                format!(
                    "{} (program {}, size {})",
                    title,
                    entry.program_id.as_deref().unwrap_or("-"),
                    entry
                        .file_size
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".to_string())
                ),
                candidate,
                "registry.amcache.applicationfile.v1",
                attrs,
            )
        })
        .collect()
}
