use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use domain::Artifact;
use serde_json::Value;

pub(super) fn shellbag_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::ShellbagEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert("path".to_string(), Value::String(entry.path.clone()));
            attrs.insert(
                "rawPidlHex".to_string(),
                Value::String(entry.raw_pidl_hex.clone()),
            );
            if let Some(slot) = entry.node_slot {
                attrs.insert("nodeSlot".to_string(), Value::Number(slot.into()));
            }
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            if let Some(ts) = entry.last_write.as_ref() {
                attrs.insert("lastWrite".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.usrclass.shellbag".to_string()),
            );
            make_artifact(
                "RegistryShellbag",
                format!("Shellbag: {}", entry.path),
                format!(
                    "{} (node_slot {}, key {})",
                    entry.path,
                    entry
                        .node_slot
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    entry.source_key_path
                ),
                candidate,
                "registry.usrclass.shellbag.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn muicache_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::MuiCacheEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "programPath".to_string(),
                Value::String(entry.program_path.clone()),
            );
            attrs.insert(
                "friendlyName".to_string(),
                Value::String(entry.friendly_name.clone()),
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
                Value::String("registry.usrclass.muicache".to_string()),
            );
            make_artifact(
                "RegistryMuiCache",
                format!("MuiCache: {}", entry.friendly_name),
                format!("{} = {}", entry.program_path, entry.friendly_name),
                candidate,
                "registry.usrclass.muicache.v1",
                attrs,
            )
        })
        .collect()
}
