use domain::FileEntry;
use transport::dto::analysis::{AnalysisFieldProvenanceDto, AnalysisProvenanceDto};
use transport::dto::AnalysisParseStatusDto;

pub(crate) const REGISTRY_SYSTEM_PARSER: &str = "registry.system";
pub(crate) const REGISTRY_SOFTWARE_PARSER: &str = "registry.software";
pub(crate) const EVTX_BOOT_SHUTDOWN_PARSER: &str = "evtx.boot_shutdown";
const MAGIC_CLASSIFICATION_PARSER: &str = "analysis.magic";

pub(crate) fn registry_field_provenance(
    field: &str,
    parsed: artifacts_windows::ParsedRegistryField,
) -> AnalysisFieldProvenanceDto {
    AnalysisFieldProvenanceDto {
        field: field.to_string(),
        value_name: parsed.value_name,
        key_path: parsed.key_path,
        hive_path: parsed.hive_path,
        parser: parsed.parser,
    }
}

pub(crate) fn file_classification_provenance<E: std::fmt::Display>(
    entry: &FileEntry,
    parsed_at: &str,
    read_result: &Result<Vec<u8>, E>,
) -> AnalysisProvenanceDto {
    let (status, warnings) = match read_result {
        Ok(_) => (AnalysisParseStatusDto::Parsed, Vec::new()),
        Err(err) => (
            AnalysisParseStatusDto::Unavailable,
            vec![format!("header read failed: {}", err)],
        ),
    };

    entry_provenance(
        entry,
        MAGIC_CLASSIFICATION_PARSER,
        parsed_at,
        status,
        warnings,
    )
}

pub(crate) fn metadata_classification_provenance(
    entry: &FileEntry,
    parsed_at: &str,
) -> AnalysisProvenanceDto {
    AnalysisProvenanceDto {
        data_source_id: entry.data_source_id.0.clone(),
        artifact_path: entry.path.clone(),
        parser: "metadata.extension_path".to_string(),
        parsed_at: parsed_at.to_string(),
        status: AnalysisParseStatusDto::Parsed,
        warnings: vec![
            "metadata-only classification; file content/header was not read".to_string(),
        ],
    }
}

pub(crate) fn entry_provenance(
    entry: &FileEntry,
    parser: &str,
    parsed_at: &str,
    status: AnalysisParseStatusDto,
    warnings: Vec<String>,
) -> AnalysisProvenanceDto {
    AnalysisProvenanceDto {
        data_source_id: entry.data_source_id.0.clone(),
        artifact_path: entry.path.clone(),
        parser: parser.to_string(),
        parsed_at: parsed_at.to_string(),
        status,
        warnings,
    }
}

pub(crate) fn unknown_provenance(
    parser: &str,
    parsed_at: &str,
    status: AnalysisParseStatusDto,
    warnings: Vec<String>,
) -> AnalysisProvenanceDto {
    AnalysisProvenanceDto {
        data_source_id: String::new(),
        artifact_path: String::new(),
        parser: parser.to_string(),
        parsed_at: parsed_at.to_string(),
        status,
        warnings,
    }
}
