use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use domain::Artifact;
use serde_json::Value;

pub(super) fn extract_registry_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    if !bytes.starts_with(b"regf") {
        outcome
            .warnings
            .push(format!("{} is not a regf registry hive", candidate.path));
        return outcome;
    }

    let normalized = normalize_evidence_path(&candidate.path);
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
                outcome.warnings.extend(info.warnings);
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} registry parse failed: {}", candidate.path, err)),
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
                outcome.warnings.extend(info.warnings);
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} registry parse failed: {}", candidate.path, err)),
        }
    } else {
        outcome.warnings.push(format!(
            "{} found as registry hive; v1 extracts key values only from SYSTEM/SOFTWARE",
            candidate.path
        ));
    }
    outcome
}

fn registry_field_artifacts(
    candidate: &EvidenceCandidate,
    fields: Vec<(&str, Option<artifacts_windows::ParsedRegistryField>)>,
) -> Vec<Artifact> {
    fields
        .into_iter()
        .filter_map(|(field_name, parsed)| parsed.map(|parsed| (field_name, parsed)))
        .map(|(field_name, parsed)| {
            let mut attrs = base_attrs(candidate);
            attrs.insert("field".to_string(), Value::String(field_name.to_string()));
            attrs.insert(
                "hivePath".to_string(),
                Value::String(parsed.hive_path.clone()),
            );
            attrs.insert(
                "keyPath".to_string(),
                Value::String(parsed.key_path.clone()),
            );
            attrs.insert(
                "valueName".to_string(),
                Value::String(parsed.value_name.clone()),
            );
            attrs.insert("valueType".to_string(), Value::String("string".to_string()));
            attrs.insert("data".to_string(), Value::String(parsed.value.clone()));
            attrs.insert("parser".to_string(), Value::String(parsed.parser.clone()));
            make_artifact(
                "RegistryValue",
                format!("Registry {}: {}", field_name, parsed.value),
                format!(
                    "{}\\{} = {}",
                    parsed.key_path, parsed.value_name, parsed.value
                ),
                candidate,
                "registry.v1",
                attrs,
            )
        })
        .collect()
}
