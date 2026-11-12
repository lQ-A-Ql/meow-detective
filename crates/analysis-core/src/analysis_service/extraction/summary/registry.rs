mod structured;

use self::structured::load_registry_structured_summary;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_family_prefix, query_artifact_rows, status_from_total,
};
use crate::analysis_service::extraction::attr_mapping::string_attr;
use chrono::Utc;
use rusqlite::Connection;
use transport::dto::{
    RegistryExtractionSummaryDto, RegistryStructuredSummaryDto, RegistryValueDto,
};

pub fn get_registry_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<RegistryExtractionSummaryDto, AnalysisServiceError> {
    let total = count_artifacts_by_family_prefix(conn, "Registry")?;
    let rows = query_artifact_rows(conn, &["RegistryValue"], offset, limit)?;
    let values = rows
        .into_iter()
        .map(|row| RegistryValueDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            hive_path: string_attr(&row.attrs, "hivePath"),
            key_path: string_attr(&row.attrs, "keyPath"),
            value_name: string_attr(&row.attrs, "valueName"),
            value_type: string_attr(&row.attrs, "valueType"),
            data: string_attr(&row.attrs, "data"),
            parser: row
                .extractor_id
                .unwrap_or_else(|| "registry.v1".to_string()),
            created_at: row.created_at,
        })
        .collect::<Vec<_>>();
    Ok(RegistryExtractionSummaryDto {
        status: status_from_total(total),
        total,
        values,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

pub fn get_registry_structured_summary(
    conn: &Connection,
) -> Result<RegistryStructuredSummaryDto, AnalysisServiceError> {
    load_registry_structured_summary(conn)
}
