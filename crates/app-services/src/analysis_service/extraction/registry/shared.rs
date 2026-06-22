use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use domain::Artifact;
use serde_json::Value;

pub(super) fn registry_field_artifacts(
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

pub(super) fn format_size_kb(kb: u64) -> String {
    if kb < 1024 {
        format!("{kb} KB")
    } else if kb < 1024 * 1024 {
        format!("{:.1} MB", kb as f64 / 1024.0)
    } else {
        format!("{:.1} GB", kb as f64 / (1024.0 * 1024.0))
    }
}

pub(super) fn friendly_browser_name(prog_id: &str) -> &str {
    if prog_id.eq_ignore_ascii_case("ChromeHTML") {
        "Google Chrome"
    } else if prog_id.eq_ignore_ascii_case("MSEdgeHTM") {
        "Microsoft Edge"
    } else if prog_id.eq_ignore_ascii_case("FirefoxURL") {
        "Mozilla Firefox"
    } else if prog_id.eq_ignore_ascii_case("BraveHTML") {
        "Brave"
    } else if prog_id.eq_ignore_ascii_case("OperaStable")
        || prog_id.eq_ignore_ascii_case("OperaHTML")
    {
        "Opera"
    } else if prog_id.eq_ignore_ascii_case("SafariHTML") {
        "Safari"
    } else {
        prog_id
    }
}

pub(super) fn hive_meta_artifact(
    candidate: &EvidenceCandidate,
    txlog_merged: bool,
    deleted_keys_found: u32,
) -> Artifact {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "hiveName".to_string(),
        Value::String(
            candidate
                .path
                .replace('\\', "/")
                .rsplit('/')
                .next()
                .unwrap_or(&candidate.path)
                .to_string(),
        ),
    );
    attrs.insert(
        "recognized".to_string(),
        Value::String("v1 metadata only".to_string()),
    );
    attrs.insert("txlogMerged".to_string(), Value::Bool(txlog_merged));
    attrs.insert(
        "deletedKeysFound".to_string(),
        Value::Number(serde_json::Number::from(deleted_keys_found)),
    );
    make_artifact(
        "RegistryHive",
        format!("Registry Hive: {}", candidate.path),
        format!(
            "Recognized registry hive {} (txlog_merged={}, deleted_keys_found={})",
            candidate.path, txlog_merged, deleted_keys_found
        ),
        candidate,
        "registry.hive.v1",
        attrs,
    )
}
