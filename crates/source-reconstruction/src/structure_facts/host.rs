use domain::{structure::StructureRelationKind, FileEntry};
use serde_json::json;
use uuid::Uuid;

use super::model::{key, Evidence, FactProjection};
use crate::structure_analysis::StructureAnalysisError;

const UUID_PATH: &str = "/sys/class/dmi/id/product_uuid";
const SERIAL_PATH: &str = "/sys/class/dmi/id/board_serial";
const BOOT_PATH: &str = "/proc/sys/kernel/random/boot_id";

pub(super) fn is_host_path(path: &str) -> bool {
    [UUID_PATH, SERIAL_PATH, BOOT_PATH]
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

pub(super) fn project(
    files: Vec<(FileEntry, Vec<u8>, Evidence)>,
    source: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    let mut by_root = std::collections::BTreeMap::<String, Vec<_>>::new();
    for (entry, bytes, evidence) in files {
        let path = super::model::normalized_path(&entry.path);
        let suffix = [UUID_PATH, SERIAL_PATH, BOOT_PATH]
            .into_iter()
            .find(|suffix| path.ends_with(suffix))
            .expect("host evidence path");
        by_root
            .entry(path[..path.len() - suffix.len()].to_owned())
            .or_default()
            .push((suffix, bytes, evidence));
    }
    for (root, files) in by_root {
        project_host(&root, &files, source, graph)?;
    }
    Ok(())
}

fn project_host(
    root: &str,
    files: &[(&str, Vec<u8>, Evidence)],
    source: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    let text = |suffix: &str| {
        files
            .iter()
            .find(|(path, _, _)| *path == suffix)
            .and_then(|(_, bytes, _)| std::str::from_utf8(bytes).ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let Some(uuid) = text(UUID_PATH)
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|uuid| !uuid.is_nil())
    else {
        return Ok(());
    };
    let uuid = uuid.to_string();
    let serial = text(SERIAL_PATH).filter(|value| {
        !matches!(
            value.to_ascii_lowercase().as_str(),
            "none" | "unknown" | "default string" | "to be filled by o.e.m." | "0"
        )
    });
    let boot = text(BOOT_PATH)
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|uuid| !uuid.is_nil())
        .map(|uuid| uuid.to_string());
    let identity = match (serial, boot.as_deref()) {
        (Some(serial), Some(_)) => key("host", &[&source.source_revision.case_id, &uuid, serial]),
        _ => key(
            "host-candidate",
            &[
                &source.source_revision.case_id,
                &source.acquisition_id,
                &source.source_revision.source_id,
                root,
                &uuid,
            ],
        ),
    };
    let host = graph.entity(
        "host-evidence",
        "physical_host",
        &identity,
        &format!("Host {uuid}"),
    );
    if serial.is_some() && boot.is_some() {
        graph.host_uuid(&uuid, &host);
    }
    let source_node = format!(
        "source:{}@{}",
        source.source_revision.source_id, source.source_revision.revision
    );
    for (suffix, _, evidence) in files {
        let mut evidence = evidence.clone();
        evidence.parser = "host.hardware_identity".into();
        evidence.assertion_kind = "captured_identity".into();
        graph.fact(&host, Some(&source_node), "host_evidence", &evidence, json!({
            "field": suffix,
            "hardwareUuid": uuid,
            "boardSerial": serial,
            "bootId": boot,
            "bindingState": if serial.is_some() && boot.is_some() { "corroborated" } else { "candidate" },
        }))?;
    }
    graph.edge(
        &host,
        &source_node,
        StructureRelationKind::DeploymentControl,
        "host_source",
    );
    Ok(())
}
