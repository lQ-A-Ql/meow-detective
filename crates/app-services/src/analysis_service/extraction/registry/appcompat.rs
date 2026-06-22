use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use domain::Artifact;
use serde_json::Value;

pub(super) fn appcompat_layer_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::AppCompatLayerEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "executablePath".to_string(),
                Value::String(entry.executable_path.clone()),
            );
            attrs.insert(
                "layerString".to_string(),
                Value::String(entry.layer_string.clone()),
            );
            attrs.insert(
                "sourceHivePath".to_string(),
                Value::String(entry.source_hive_path.clone()),
            );
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            if let Some(ts) = entry.last_write.as_ref() {
                attrs.insert("lastWrite".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.appcompat.layer".to_string()),
            );
            make_artifact(
                "RegistryAppCompatLayer",
                format!("AppCompat Layer: {}", entry.executable_path),
                format!("{} -> {}", entry.executable_path, entry.layer_string),
                candidate,
                "registry.appcompat.layer.v1",
                attrs,
            )
        })
        .collect()
}
