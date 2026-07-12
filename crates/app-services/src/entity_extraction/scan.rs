use std::collections::BTreeMap;
use std::sync::LazyLock;

use persistence_sqlite::repositories::entity_repo;
use regex::Regex;
use rusqlite::Connection;

use super::EntityExtractionError;

pub(super) type EntityMap = BTreeMap<(String, String), Vec<String>>;

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").expect("valid email regex")
});
static SID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"S-1-5-21-\d+-\d+-\d+-\d+").expect("valid SID regex"));

pub(super) fn artifact_ids_for_case(
    conn: &Connection,
    case_id: &str,
) -> Result<Vec<String>, EntityExtractionError> {
    let mut ids =
        entity_repo::get_artifact_ids_for_case(conn, case_id).map_err(EntityExtractionError::Db)?;
    ids.sort();
    ids.dedup();
    Ok(ids)
}

pub(super) fn scan_artifacts(
    conn: &Connection,
    case_id: &str,
) -> Result<EntityMap, EntityExtractionError> {
    let rows = entity_repo::get_artifact_rows_for_case(conn, case_id)
        .map_err(EntityExtractionError::Db)?;
    let mut entities = EntityMap::new();

    for (artifact_id, title, summary, attrs_json) in rows {
        scan_text(&mut entities, &artifact_id, &title, &summary, &attrs_json);
        scan_device_fields(&mut entities, &artifact_id, &attrs_json);
    }

    normalize_sources(&mut entities);
    Ok(entities)
}

fn scan_text(
    entities: &mut EntityMap,
    artifact_id: &str,
    title: &str,
    summary: &str,
    attrs_json: &str,
) {
    let combined = format!("{title} {summary} {attrs_json}");
    for capture in EMAIL_RE.captures_iter(&combined) {
        add_source(entities, capture[0].to_lowercase(), "person", artifact_id);
    }
    for capture in SID_RE.captures_iter(&combined) {
        add_source(entities, capture[0].to_string(), "account", artifact_id);
    }
}

fn scan_device_fields(entities: &mut EntityMap, artifact_id: &str, attrs_json: &str) {
    let attrs: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(attrs_json).unwrap_or_default();
    for key in [
        "hostname",
        "computer_name",
        "computerName",
        "machine_name",
        "machineName",
    ] {
        let Some(serde_json::Value::String(value)) = attrs.get(key) else {
            continue;
        };
        let value = value.trim().to_lowercase();
        if !value.is_empty() {
            add_source(entities, value, "device", artifact_id);
        }
    }
}

fn add_source(entities: &mut EntityMap, value: String, entity_type: &str, artifact_id: &str) {
    entities
        .entry((value, entity_type.to_string()))
        .or_default()
        .push(artifact_id.to_string());
}

fn normalize_sources(entities: &mut EntityMap) {
    for source_ids in entities.values_mut() {
        source_ids.sort();
        source_ids.dedup();
    }
}
