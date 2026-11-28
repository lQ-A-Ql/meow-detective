use domain::Artifact;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::model::{Evidence, FactProjection};
use crate::structure_analysis::StructureAnalysisError;

pub(super) fn project(
    artifact: &Artifact,
    source: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    let parser = artifact.extractor_id.as_deref();
    if parser != Some("linux.pve_config") {
        return Ok(());
    }
    let mut evidence = source.clone();
    evidence.artifact_id = Some(artifact.id.0.clone());
    evidence.file_id = artifact.source_object_id.as_ref().map(|id| id.0.clone());
    evidence.parser = parser.expect("selected parser").into();
    evidence.parser_version = artifact.extractor_version.clone().unwrap_or_default();
    evidence.projection_sha256 = Some(hex::encode(Sha256::digest(serde_json::to_vec(
        &artifact.attrs,
    )?)));
    evidence.locator = artifact.source_attribution.clone();
    let value = |key: &str| artifact.attrs.get(key).and_then(Value::as_str);
    evidence.assertion_kind = "declared_configuration".into();
    let path = super::model::normalized_path(value("sourcePath").unwrap_or_default());
    let Some((root, filename)) = path.rsplit_once("/etc/pve/qemu-server/") else {
        return Ok(());
    };
    let Some(vm_id) = filename.strip_suffix(".conf") else {
        return Ok(());
    };
    if vm_id.is_empty() || !vm_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(());
    }
    let scope = super::model::key(
        "pve-scope",
        &[
            &source.acquisition_id,
            &source.source_revision.source_id,
            root,
        ],
    );
    let pve = graph.entity(&scope, "pve", "unproven", "PVE configuration");
    let vm = graph.entity(&scope, "vm", vm_id, &format!("VM {vm_id}"));
    graph.fact(
        &vm,
        Some(&pve),
        "configured_in",
        &evidence,
        json!({
            "configKey": value("key"),
            "lineNumber": artifact.attrs.get("lineNumber"),
        }),
    )?;
    let Some(disk_key) = value("key").filter(|key| is_disk_key(key)) else {
        return Ok(());
    };
    let Some(reference) = value("value") else {
        return Ok(());
    };
    let disk = graph.entity(
        &scope,
        "disk_reference",
        &format!("{vm_id}:{disk_key}"),
        reference,
    );
    graph.fact(
        &vm,
        Some(&disk),
        "declares_disk",
        &evidence,
        json!({
            "reference": reference,
            "backendState": "needs_evidence",
            "requiredEvidence": "scoped_native_backend_identity_and_published_version",
        }),
    )?;
    Ok(())
}

fn is_disk_key(key: &str) -> bool {
    ["scsi", "sata", "ide", "virtio", "efidisk", "tpmstate"]
        .iter()
        .any(|prefix| {
            key.strip_prefix(prefix).is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
}
