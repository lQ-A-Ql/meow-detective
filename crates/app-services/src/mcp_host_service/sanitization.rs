use serde_json::{json, Value};
use transport::dto::{DataSourceSummaryDto, PluginModuleDto};

pub(super) fn source_summary(source: DataSourceSummaryDto) -> Value {
    json!({
        "id": source.id, "name": source.name, "kind": source.kind,
        "platform": source.platform, "importedAt": source.imported_at,
        "importState": source.import_state, "fileCount": source.file_count,
        "evidenceSize": source.evidence_size, "hashStatus": source.hash_status,
        "sourceHash": source.source_hash, "readerKind": source.reader_kind,
        "provenanceStatus": source.provenance_status,
        "processing": source.processing.as_ref().map(|summary| &summary.state),
        "partitions": source.partitions.into_iter().map(|partition| json!({
            "index":partition.index, "name":partition.name, "kindLabel":partition.kind_label,
            "status":partition.status, "filesystem":partition.filesystem,
            "offset":partition.offset, "length":partition.length,
        })).collect::<Vec<_>>()
    })
}

pub(super) fn plugin_summary(module: PluginModuleDto) -> Value {
    json!({
        "pluginId": module.plugin_id, "displayName": module.display_name,
        "pluginVersion": module.plugin_version, "evidencePlatform": module.evidence_platform,
        "families": module.families, "totalCount": module.total_count,
        "warningCount": module.warnings.len(),
    })
}

#[cfg(test)]
#[path = "../../tests/unit/mcp_host_service/sanitization.rs"]
mod tests;
