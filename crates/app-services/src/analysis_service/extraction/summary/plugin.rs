//! Generic plugin analysis summary (design doc §3).
//!
//! Plugin artifacts are grouped by `extractor_id` (= plugin id, enforced by
//! the host at extraction time) and `artifact_type` (= family). All SQL stays
//! in this helper module; the use-case layer supplies the loaded plugin
//! metadata and the extraction diagnostics.

use crate::analysis_service::error::AnalysisServiceError;
use crate::plugin_loader::PluginModuleMeta;
use rusqlite::Connection;
use std::collections::BTreeMap;
use transport::dto::{
    PluginArtifactEntryDto, PluginFamilyCountDto, PluginFamilyEntriesDto, PluginModuleDto,
};

/// List the loaded plugin modules for one source database with per-family
/// artifact counts. `warnings_by_plugin` carries extraction diagnostics keyed
/// by plugin id (from the case audit trail); plugins without diagnostics get
/// an empty list.
pub fn list_plugin_modules(
    conn: &Connection,
    plugins: &[PluginModuleMeta],
    warnings_by_plugin: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<PluginModuleDto>, AnalysisServiceError> {
    let mut modules = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let mut families = Vec::with_capacity(plugin.families.len());
        let mut total_count = 0_u64;
        for family in &plugin.families {
            let count = count_plugin_family_artifacts(conn, &plugin.plugin_id, family)?;
            total_count = total_count.saturating_add(count);
            families.push(PluginFamilyCountDto {
                family: family.clone(),
                count,
            });
        }
        modules.push(PluginModuleDto {
            plugin_id: plugin.plugin_id.clone(),
            display_name: plugin.display_name.clone(),
            plugin_version: plugin.plugin_version.clone(),
            evidence_platform: plugin.evidence_platform.clone(),
            families,
            total_count,
            warnings: warnings_by_plugin
                .get(&plugin.plugin_id)
                .cloned()
                .unwrap_or_default(),
        });
    }
    Ok(modules)
}

/// One page of generic artifact entries for one declared plugin family.
pub fn get_plugin_family_entries(
    conn: &Connection,
    plugin: &PluginModuleMeta,
    family: &str,
    offset: u64,
    limit: u32,
) -> Result<PluginFamilyEntriesDto, AnalysisServiceError> {
    if !plugin.families.iter().any(|declared| declared == family) {
        return Err(AnalysisServiceError::InvalidInput(format!(
            "plugin '{}' does not declare family '{}'",
            plugin.plugin_id, family
        )));
    }
    let total_count = count_plugin_family_artifacts(conn, &plugin.plugin_id, family)?;
    let entries = query_plugin_family_entries(conn, &plugin.plugin_id, family, offset, limit)?;
    let truncated = offset.saturating_add(entries.len() as u64) < total_count;
    Ok(PluginFamilyEntriesDto {
        plugin_id: plugin.plugin_id.clone(),
        family: family.to_string(),
        total_count,
        truncated,
        entries,
    })
}

fn count_plugin_family_artifacts(
    conn: &Connection,
    plugin_id: &str,
    family: &str,
) -> Result<u64, AnalysisServiceError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM artifacts WHERE extractor_id = ?1 AND artifact_type = ?2",
        rusqlite::params![plugin_id, family],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

fn query_plugin_family_entries(
    conn: &Connection,
    plugin_id: &str,
    family: &str,
    offset: u64,
    limit: u32,
) -> Result<Vec<PluginArtifactEntryDto>, AnalysisServiceError> {
    let mut statement = conn.prepare(
        "SELECT id, source_object_id, source_attribution, title, summary, confidence, attrs, created_at
         FROM artifacts
         WHERE extractor_id = ?1 AND artifact_type = ?2
         ORDER BY created_at DESC, id ASC
         LIMIT ?3 OFFSET ?4",
    )?;
    let rows = statement.query_map(
        rusqlite::params![plugin_id, family, i64::from(limit), offset as i64],
        |row| {
            let attrs_text: String = row.get(6)?;
            Ok(PluginArtifactEntryDto {
                artifact_id: row.get(0)?,
                file_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                source_path: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                title: row.get(3)?,
                summary: row.get(4)?,
                confidence: row.get::<_, Option<f64>>(5)?.map(|value| value as f32),
                attrs: serde_json::from_str(&attrs_text).unwrap_or_default(),
                created_at: row.get(7)?,
            })
        },
    )?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/summary/plugin.rs"]
mod tests;
