use std::path::Path;

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use rusqlite::Connection;
use std::collections::BTreeMap;
use transport::dto::{
    BrowserHistorySummaryDto, EmailExtractionSummaryDto, EvtxEventSummaryDto, EvtxEventViewDto,
    LinuxArtifactSummaryDto, PluginFamilyEntriesDto, PluginModuleDto, RegistryExtractionSummaryDto,
    RegistryStructuredSummaryDto,
};

use super::source::open_ready_analysis_source;
use crate::analysis_service::capability::PLUGIN_CAPABILITY_KEY;
use crate::analysis_service::extraction::{get_plugin_family_entries, list_plugin_modules};
use crate::analysis_service::{
    get_browser_history_summary, get_email_extraction_summary, get_evtx_event_summary,
    get_linux_artifact_summary, get_registry_extraction_summary, get_registry_structured_summary,
    validate_analysis_categories, AnalysisServiceError,
};
use crate::plugin_loader::PluginModuleMeta;

fn open_for_capability(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    capability: &str,
) -> Result<super::source::AnalysisSource, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    validate_analysis_categories(source.platform, &[capability])?;
    Ok(source)
}

pub fn get_source_registry_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
) -> Result<RegistryExtractionSummaryDto, AnalysisServiceError> {
    let source = open_for_capability(case_conn, case_root, case_id, data_source_id, "Registry")?;
    get_registry_extraction_summary(&source.connection, offset, limit)
}

pub fn get_source_registry_structured_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<RegistryStructuredSummaryDto, AnalysisServiceError> {
    let source = open_for_capability(case_conn, case_root, case_id, data_source_id, "Registry")?;
    get_registry_structured_summary(&source.connection)
}

macro_rules! paged_summary {
    ($name:ident, $capability:literal, $return_type:ty, $query:path) => {
        pub fn $name(
            case_conn: &Connection,
            case_root: &Path,
            case_id: &CaseId,
            data_source_id: &DataSourceId,
            offset: u64,
            limit: u32,
        ) -> Result<$return_type, AnalysisServiceError> {
            let source =
                open_for_capability(case_conn, case_root, case_id, data_source_id, $capability)?;
            $query(&source.connection, offset, limit)
        }
    };
}

paged_summary!(
    get_source_browser_summary,
    "BrowserHistory",
    BrowserHistorySummaryDto,
    get_browser_history_summary
);
paged_summary!(
    get_source_email_summary,
    "Email",
    EmailExtractionSummaryDto,
    get_email_extraction_summary
);
pub fn get_source_evtx_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    view: Option<EvtxEventViewDto>,
    offset: u64,
    limit: u32,
) -> Result<EvtxEventSummaryDto, AnalysisServiceError> {
    let source = open_for_capability(case_conn, case_root, case_id, data_source_id, "EventLogs")?;
    get_evtx_event_summary(&source.connection, view, offset, limit)
}
paged_summary!(
    get_source_linux_summary,
    "LinuxArtifacts",
    LinuxArtifactSummaryDto,
    get_linux_artifact_summary
);

/// Cap on how many historical extraction-failure audit entries are folded
/// into one module's warnings.
const PLUGIN_DIAGNOSTIC_AUDIT_LIMIT: u32 = 50;

/// List the plugin modules of one data source: every loaded plugin matching
/// the source platform, joined with its source-database artifact counts and
/// the extraction diagnostics recorded in the case audit trail.
pub fn get_source_plugin_modules(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<Vec<PluginModuleDto>, AnalysisServiceError> {
    let source = open_for_capability(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        PLUGIN_CAPABILITY_KEY,
    )?;
    let metas = plugin_metas_for_platform(source.platform);
    if metas.is_empty() {
        return Ok(Vec::new());
    }
    let warnings = plugin_extraction_warnings(case_conn, &case_id.0);
    list_plugin_modules(&source.connection, &metas, &warnings)
}

/// One page of generic artifact entries for one plugin family of one source.
#[allow(clippy::too_many_arguments)]
pub fn get_source_plugin_family_entries(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    plugin_id: &str,
    family: &str,
    offset: u64,
    limit: u32,
) -> Result<PluginFamilyEntriesDto, AnalysisServiceError> {
    let source = open_for_capability(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        PLUGIN_CAPABILITY_KEY,
    )?;
    let metas = plugin_metas_for_platform(source.platform);
    let plugin = metas
        .iter()
        .find(|meta| meta.plugin_id == plugin_id)
        .ok_or_else(|| {
            AnalysisServiceError::InvalidInput(format!(
                "plugin '{plugin_id}' is not loaded for this source platform"
            ))
        })?;
    get_plugin_family_entries(&source.connection, plugin, family, offset, limit)
}

/// Metadata of the currently loaded plugins for one evidence platform. An
/// empty result means plugins are disabled, absent, or all refused — the
/// module group simply does not render.
fn plugin_metas_for_platform(platform: DataSourcePlatform) -> Vec<PluginModuleMeta> {
    let expected = match platform {
        DataSourcePlatform::Windows => "windows",
        DataSourcePlatform::Linux => "linux",
        DataSourcePlatform::Unknown => return Vec::new(),
    };
    crate::plugin_loader::load_all()
        .iter()
        .map(|plugin| plugin.module_meta())
        .filter(|meta| meta.evidence_platform == expected)
        .collect()
}

/// Recent plugin extraction failures from the case audit trail, grouped by
/// plugin id. Audit reads degrade to empty on any problem: module listings
/// must stay available even when the audit log is unreadable.
fn plugin_extraction_warnings(
    case_conn: &Connection,
    case_id: &str,
) -> BTreeMap<String, Vec<String>> {
    let mut warnings: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let entries = AuditRepo::new(case_conn).query(
        Some(case_id),
        Some(AuditAction::PluginExtractFailed.as_str()),
        PLUGIN_DIAGNOSTIC_AUDIT_LIMIT,
        0,
    );
    let Ok(entries) = entries else {
        return warnings;
    };
    for entry in entries {
        let Some(plugin_id) = entry.resource_id else {
            continue;
        };
        warnings
            .entry(plugin_id)
            .or_default()
            .push(plugin_diagnostic_text(&entry.details));
    }
    warnings
}

/// Render one `plugin.extract_failed` audit details payload as a single
/// human-readable diagnostic line.
fn plugin_diagnostic_text(details: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(details) else {
        return details.to_string();
    };
    let path = value.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let error = value.get("error").and_then(|v| v.as_str()).unwrap_or("");
    if path.is_empty() {
        error.to_string()
    } else {
        format!("{path}: {error}")
    }
}
