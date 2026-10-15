use super::common::insert_opt;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;
fn insert_string_array(
    attrs: &mut std::collections::BTreeMap<String, Value>,
    key: &str,
    values: &[String],
) {
    if values.is_empty() {
        return;
    }
    attrs.insert(
        key.to_string(),
        Value::Array(values.iter().cloned().map(Value::String).collect()),
    );
}

pub(in crate::analysis_service::extraction) fn append_resolved_unit(
    candidate: &EvidenceCandidate,
    unit: &artifacts_linux::LinuxServiceUnit,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "configKind".to_string(),
        Value::String("systemdServiceUnit".to_string()),
    );
    attrs.insert("line".to_string(), Value::String(unit.name.clone()));
    attrs.insert("lineNumber".to_string(), Value::Number(1.into()));
    insert_opt(&mut attrs, "name", Some(unit.name.clone()));
    insert_opt(&mut attrs, "description", unit.description.clone());
    insert_opt(&mut attrs, "state", Some(unit.state.clone()));
    attrs.insert("enabled".to_string(), Value::Bool(unit.enabled));
    attrs.insert("masked".to_string(), Value::Bool(unit.masked));
    attrs.insert("static".to_string(), Value::Bool(unit.static_unit));
    attrs.insert(
        "unitFile".to_string(),
        Value::String(unit.unit_file.clone()),
    );
    insert_string_array(&mut attrs, "execStart", &unit.exec_start);
    insert_string_array(&mut attrs, "execStop", &unit.exec_stop);
    insert_opt(&mut attrs, "user", unit.user.clone());
    insert_opt(&mut attrs, "group", unit.group.clone());
    insert_opt(
        &mut attrs,
        "workingDirectory",
        unit.working_directory.clone(),
    );
    insert_opt(&mut attrs, "restart", unit.restart.clone());
    insert_string_array(&mut attrs, "wantedBy", &unit.wanted_by);
    insert_string_array(&mut attrs, "requiredBy", &unit.required_by);
    insert_opt(
        &mut attrs,
        "enablementSymlink",
        unit.enablement_symlink.clone(),
    );
    insert_string_array(&mut attrs, "dropIns", &unit.drop_ins);
    insert_string_array(&mut attrs, "aliases", &unit.aliases);
    insert_string_array(&mut attrs, "sourcePaths", &unit.source_paths);
    let title = unit
        .description
        .clone()
        .unwrap_or_else(|| unit.name.clone());
    outcome.artifacts.push(make_artifact(
        "LinuxSystemConfig",
        format!("systemd unit: {}: {title}", unit.name),
        format!("{} ({})", unit.name, unit.state),
        candidate,
        "linux.systemd_unit",
        attrs,
    ));
}
