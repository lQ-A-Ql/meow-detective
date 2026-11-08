//! Generic plugin analysis summary (design doc §3).
//!
//! Plugin artifacts are grouped by `extractor_id` (= plugin id, enforced by
//! the host at extraction time) and `artifact_type` (= family). All SQL stays
//! in this helper module; the use-case layer supplies the loaded plugin
//! metadata and the extraction diagnostics.

use crate::analysis_service::error::AnalysisServiceError;
use crate::plugin_loader::PluginModuleMeta;
use domain::Artifact;
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use std::cmp::Ordering;
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
    let mut count = 0;
    ArtifactRepo::new(conn).visit_by_family(Some(family), |artifact| {
        if artifact.extractor_id.as_deref() == Some(plugin_id) {
            count += 1;
        }
        Ok(())
    })?;
    Ok(count)
}

fn query_plugin_family_entries(
    conn: &Connection,
    plugin_id: &str,
    family: &str,
    offset: u64,
    limit: u32,
) -> Result<Vec<PluginArtifactEntryDto>, AnalysisServiceError> {
    let window = offset
        .checked_add(u64::from(limit))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            AnalysisServiceError::InvalidInput("plugin page range exceeds platform limits".into())
        })?;
    let mut candidates = Vec::new();
    ArtifactRepo::new(conn).visit_by_family(Some(family), |artifact| {
        if artifact.extractor_id.as_deref() != Some(plugin_id) {
            return Ok(());
        }
        candidates.push(artifact);
        if candidates.len() > window {
            candidates.sort_by(|left, right| plugin_artifact_cmp(left, right, plugin_id, family));
            candidates.pop();
        }
        Ok(())
    })?;
    candidates.sort_by(|left, right| plugin_artifact_cmp(left, right, plugin_id, family));
    Ok(candidates
        .into_iter()
        .skip(usize::try_from(offset).map_err(|_| {
            AnalysisServiceError::InvalidInput("plugin offset exceeds platform limits".into())
        })?)
        .take(limit as usize)
        .map(to_plugin_entry)
        .collect())
}

fn to_plugin_entry(artifact: Artifact) -> PluginArtifactEntryDto {
    PluginArtifactEntryDto {
        artifact_id: artifact.id.0,
        file_id: artifact.source_object_id.map(|id| id.0).unwrap_or_default(),
        source_path: artifact.source_attribution.unwrap_or_default(),
        title: artifact.title,
        summary: artifact.summary,
        confidence: artifact.confidence,
        attrs: artifact.attrs.into_iter().collect(),
        created_at: artifact.created_at.to_rfc3339(),
    }
}

fn plugin_artifact_cmp(
    left: &Artifact,
    right: &Artifact,
    plugin_id: &str,
    family: &str,
) -> Ordering {
    if plugin_id == "meow.plugin.wechat" {
        let left_time = wechat_time(left, family);
        let right_time = wechat_time(right, family);
        return right_time
            .cmp(&left_time)
            .then_with(|| wechat_sequence(right, family).cmp(&wechat_sequence(left, family)))
            .then_with(|| left.id.0.cmp(&right.id.0));
    }
    right
        .created_at
        .cmp(&left.created_at)
        .then_with(|| left.id.0.cmp(&right.id.0))
}

fn wechat_time(artifact: &Artifact, family: &str) -> String {
    let field = match family {
        "WeChatMessage" | "WeChatMoment" => "createTimeUtc",
        "WeChatSession" => "lastTimestampUtc",
        "WeChatFavorite" => "updateTimeUtc",
        _ => "createTimeUtc",
    };
    artifact
        .attrs
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn wechat_sequence(artifact: &Artifact, family: &str) -> i64 {
    let field = match family {
        "WeChatMessage" => "localId",
        "WeChatMoment" => "tid",
        _ => "",
    };
    artifact
        .attrs
        .get(field)
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/summary/plugin.rs"]
mod tests;
