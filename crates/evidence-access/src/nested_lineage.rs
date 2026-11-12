//! Parser-record projection for nested evidence lineage.
//!
//! The import pipeline projects nested-evidence lineage rows into the source
//! parser record store. Building the record needs only the domain lineage type,
//! so it lives here instead of in the nesting service that owns the queries.

use domain::NestedEvidenceLineage;
use persistence_source::fjall::ParserRecord;

const NESTED_LINEAGE_FAMILY: &str = "nested";
const NESTED_LINEAGE_KIND: &str = "lineage";

/// Builds the source-scoped parser record for one nested evidence lineage row.
pub fn lineage_parser_record(lineage: &NestedEvidenceLineage) -> ParserRecord {
    let payload = serde_json::json!({
        "parentDataSourceId": lineage.parent_data_source_id.0,
        "nestedFilePath": lineage.nested_file_path,
        "parentFileId": lineage.parent_file_id,
        "derivedDataSourceId": lineage.derived_data_source_id.as_ref().map(|id| id.0.clone()),
        "offset": lineage.offset,
        "length": lineage.length,
        "probeKind": lineage.probe_kind,
    });
    ParserRecord {
        id: format!("lineage:{}", lineage.nested_file_path),
        data_source_id: lineage.parent_data_source_id.0.clone(),
        family: NESTED_LINEAGE_FAMILY.to_owned(),
        kind: NESTED_LINEAGE_KIND.to_owned(),
        status: "ready".to_owned(),
        summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
        payload,
    }
}
